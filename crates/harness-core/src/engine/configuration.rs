//! Focused workflow routing, settings and onboarding diagnostics.
use super::*;

impl Engine {
    pub(super) fn settings(&self) -> Result<Value> {
        let mut settings = json!({"schema_version":1,"timezone":"UTC","primary_cv":null,"preferred_browser":null,"answer_sources":["applications/approved-answers.md","context/profile/application-decisions.md"]});
        let path = self.text_path("config/career-ledger.json")?;
        if path.exists() {
            let supplied: Value = serde_json::from_slice(&fs::read(path)?)?;
            let object = supplied.as_object().ok_or_else(|| {
                Error::new("invalid_arguments", "Invalid career-ledger configuration")
            })?;
            if supplied["schema_version"] != 1 || object.keys().any(|k| settings.get(k).is_none()) {
                return Err(Error::new(
                    "invalid_arguments",
                    "Invalid career-ledger configuration",
                ));
            }
            contracts::validate(&contracts::ledger_schema(), &supplied)?;
            for (k, v) in object {
                settings[k] = v.clone();
            }
        }
        s(&settings, "timezone")?
            .parse::<chrono_tz::Tz>()
            .map_err(|_| Error::new("invalid_arguments", "Configure an IANA timezone"))?;
        if !settings["primary_cv"].is_null() {
            self.artifact_path(s(&settings, "primary_cv")?, &["pdf"])?;
        }
        if !settings["preferred_browser"].is_null() {
            s(&settings, "preferred_browser")?;
        }
        let sources = settings["answer_sources"]
            .as_array()
            .ok_or_else(|| Error::new("invalid_arguments", "Answer sources must be an array"))?;
        if sources.is_empty() || sources.len() > 20 {
            return Err(Error::new(
                "invalid_arguments",
                "Configure one to twenty canonical answer sources",
            ));
        }
        for source in sources {
            self.safe_path(
                source.as_str().ok_or_else(|| {
                    Error::new("invalid_arguments", "Answer source must be a text path")
                })?,
                &["context", "applications"],
                &["md"],
            )?;
        }
        Ok(settings)
    }
    pub(super) fn today(&self) -> Result<String> {
        let settings = self.settings()?;
        let tz = s(&settings, "timezone")?
            .parse::<chrono_tz::Tz>()
            .map_err(|_| Error::new("invalid_arguments", "Configure an IANA timezone"))?;
        Ok(chrono::Utc::now()
            .with_timezone(&tz)
            .date_naive()
            .to_string())
    }
    pub(super) fn operations(&self) -> Result<Value> {
        let path = self.text_path("config/career-workflows.json")?;
        let data = if path.exists() {
            fs::read(path)?
        } else {
            include_bytes!("../../../../resources/workflows.json").to_vec()
        };
        let mut config: Value = serde_json::from_slice(&data)?;
        let defaults: Value =
            serde_json::from_slice(include_bytes!("../../../../resources/workflows.json"))?;
        if let Some(workflows) = config["workflows"].as_object_mut() {
            for name in ["daily_check", "daily_update"] {
                workflows
                    .entry(name)
                    .or_insert_with(|| defaults["workflows"][name].clone());
            }
        }
        contracts::validate(&contracts::workflow_schema(), &config)?;
        if config["schema_version"] != 1
            || !config["shared_reads"].is_array()
            || !config["workflows"].is_object()
        {
            return Err(Error::new(
                "invalid_arguments",
                "Invalid workflow configuration",
            ));
        }
        for operation in [
            "review_inbox",
            "tidy_inbox",
            "discover",
            "apply",
            "schedule",
            "prepare_interview",
            "update_knowledge",
            "reconcile",
        ] {
            s(&config["workflows"][operation], "skill")?;
            s(&config["workflows"][operation], "result")?;
        }
        Ok(config)
    }
    pub(super) fn plan(&self, operation: &str, scope: Value) -> Result<Value> {
        let operations = self.operations()?;
        let workflow = operations["workflows"]
            .get(operation)
            .ok_or_else(|| Error::new("invalid_arguments", "Unknown workflow"))?;
        let mut reads = operations["shared_reads"].as_array().unwrap().clone();
        for path in self.settings()?["answer_sources"].as_array().unwrap() {
            if !reads.contains(path) {
                reads.push(path.clone());
            }
        }
        Ok(
            json!({"operation":operation,"scope":scope,"skill":workflow["skill"],"result":workflow["result"],"shared_reads":reads,"settings":self.settings()?,"ledger":self.root,"authority":"User request controls scope. A plan or run is not external-action permission."}),
        )
    }
    pub(super) fn bootstrap(&self, args: &Value, state: &Value) -> Result<Value> {
        let plan = args
            .get("operation")
            .and_then(Value::as_str)
            .map(|op| self.plan(op, args.get("scope").cloned().unwrap_or(json!({}))))
            .transpose()?;
        let health = self.compact(
            "reconcile_tracker",
            self.reconcile(state)?,
            &json!({}),
            state,
        )?;
        let supplied: BTreeSet<String> = args
            .get("capabilities")
            .map(string_array)
            .transpose()?
            .unwrap_or_default()
            .into_iter()
            .collect();
        let required = match args.get("operation").and_then(Value::as_str) {
            Some("review_inbox") => vec!["mail.search", "mail.read"],
            Some("tidy_inbox") => vec!["mail.search", "mail.read", "mail.modify"],
            Some("discover" | "prepare_interview") => vec!["web.read"],
            Some("apply") => vec![
                "web.read",
                "browser.form",
                "browser.upload",
                "browser.submit",
            ],
            Some("schedule") => vec!["mail.read", "calendar.read", "calendar.book"],
            _ => vec![],
        };
        let mut missing: Vec<&str> = required
            .into_iter()
            .filter(|s| !supplied.contains(*s))
            .collect();
        missing.sort_unstable();
        let settings = self.settings()?;
        let mut paths = string_array(&settings["answer_sources"])?;
        if let Some(plan) = &plan {
            paths.extend(string_array(&plan["shared_reads"])?);
            paths.push(s(plan, "skill")?.into());
        } else {
            paths.push("AGENTS.md".into());
        }
        let mut issues = vec![];
        let mut seen = BTreeSet::new();
        for path in paths {
            if !seen.insert(path.clone()) {
                continue;
            }
            let exists = self.text_path(&path).is_ok_and(|p| p.is_file())
                || workspace::resource_text(&path).is_some();
            if !exists {
                issues.push(json!({"code":"missing_source","path":path,"message":"Supply this canonical source before acting on its facts/instructions."}));
            }
        }
        if args["operation"] == "apply" {
            let cv = &settings["primary_cv"];
            if cv.as_str().is_none_or(|s| {
                self.artifact_path(s, &["pdf"])
                    .map_or(true, |p| !p.is_file())
            }) {
                issues.push(json!({"code":"missing_primary_cv","path":cv,"message":"Configure primary_cv and supply the approved PDF before preparing an application."}));
            }
        }
        let route = plan.map(|mut p| {
            p.as_object_mut().unwrap().retain(|k, _| {
                ["operation", "scope", "skill", "result", "shared_reads"].contains(&k.as_str())
            });
            p
        });
        let binary = std::env::current_exe().ok();
        Ok(
            json!({"contract_version":"2.0","ledger":self.root,"settings":settings,"plan":route,"health":health,"available_capabilities":supplied,"missing_capabilities":missing,"configuration_issues":issues,"cli_fallback":binary.map(|p|json!({"command":p,"args":["--root",self.root.to_string_lossy(),"call"],"cwd":self.root})),"paths":{"toolkit":"embedded","contract":"docs/job-harness-contract.md","onboarding":"docs/job-harness-onboarding.md"}}),
        )
    }
    pub(super) fn bootstrap_next(&self, args: &Value, data: &Value) -> Result<Vec<Value>> {
        let mut steps = vec![];
        if data["health"]["pipeline_stale"] == true {
            steps.push(json!({"tool":"rebuild_pipeline","arguments":{"idempotency_key":format!("repair:{}",domain::digest(&domain::encoded(&json!(pipeline::build(&self.root)?.records))?))},"reason":"Regenerate stale local views."}));
        }
        if args.get("operation").is_some() {
            steps.push(json!({"tool":"read_record","arguments":{"path":data["plan"]["skill"]},"reason":"Read the focused workflow."}));
        }
        Ok(steps)
    }
    pub(super) fn inspect_tools(&self, args: &Value) -> Result<Value> {
        if args["include_schemas"] == true && args.get("names").is_none() {
            return Err(Error::new(
                "invalid_arguments",
                "Request named tools when fetching schemas",
            ));
        }
        let specs = contracts::specs();
        let names = args
            .get("names")
            .map(string_array)
            .transpose()?
            .unwrap_or_else(|| specs.keys().cloned().collect());
        let mut rows = vec![];
        for name in names {
            let spec = specs
                .get(&name)
                .ok_or_else(|| Error::new("unknown_tool", "Unknown requested tool"))?;
            let mut row = json!({"name":name,"description":spec["description"],"read_only":spec["annotations"]["readOnlyHint"]});
            if args["include_schemas"] == true {
                row["inputSchema"] = spec["inputSchema"].clone();
                row["outputSchema"] = spec["outputSchema"].clone();
            }
            rows.push(row);
        }
        Ok(json!({"tools":rows}))
    }
}
