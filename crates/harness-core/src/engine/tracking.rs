//! Evidence-backed canonical vacancy changes, protected by the parent journal.
use super::*;

pub(super) struct TrackingUpdate<'a> {
    pub record: &'a str,
    pub tracking: &'a Value,
    pub summary: &'a str,
    pub evidence: &'a Value,
    pub key: &'a str,
    pub correction: Option<&'a str>,
}

impl Engine {
    pub(super) fn change(&self, path: &Path, text: String, changes: &mut Vec<Value>) -> Result<()> {
        storage::reject_symlink_ancestors(path)?;
        if changes.iter().any(|change| {
            change["path"]
                .as_str()
                .is_some_and(|rel| self.root.join(rel) == path)
        }) {
            return Err(Error::new(
                "invalid_arguments",
                "A local transaction cannot change the same canonical path twice",
            ));
        }
        let pattern = tracking_pattern();
        if pattern.find_iter(&text).count() != 1 {
            return Err(Error::new(
                "invalid_arguments",
                "A vacancy update must preserve exactly one tracking block; escape quoted tracking markup",
            ));
        }
        changes.push(json!({"path":path.strip_prefix(&self.root).map_err(|_|Error::new("path_denied","Change outside workspace"))?,"before_sha256":if path.exists(){Some(domain::digest(&fs::read(path)?))}else{None},"text":text}));
        Ok(())
    }
    pub(super) fn set_tracking(
        &self,
        update: TrackingUpdate<'_>,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        let TrackingUpdate {
            record,
            tracking,
            summary,
            evidence,
            key,
            correction,
        } = update;
        let path = self.record_path(record, true)?;
        let mut text = fs::read_to_string(&path)?;
        let pattern = tracking_pattern();
        let captures = pattern.captures(&text);
        let old = if let Some(c) = &captures {
            serde_json::from_str(&c[1])?
        } else {
            pipeline::build(&self.root)?
                .records
                .into_iter()
                .find(|r| r["record"].as_str() == path.file_name().and_then(|n| n.to_str()))
                .ok_or_else(|| Error::new("missing_resource", "Missing canonical vacancy"))?
        };
        let object = tracking
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Tracking must be an object"))?;
        if object
            .keys()
            .any(|key| !domain::TRACKING_KEYS.contains(&key.as_str()))
        {
            return Err(Error::new("invalid_arguments", "Unknown tracking fields"));
        }
        let mut merged = if captures.is_some() {
            old.clone()
        } else {
            Value::Object(
                old.as_object()
                    .unwrap()
                    .iter()
                    .filter(|(k, _)| domain::TRACKING_KEYS.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
        };
        for (k, v) in object {
            merged[k] = v.clone();
        }
        let mut previous_booking = None;
        if object.contains_key("schedule_state")
            && tracking["schedule_state"] != "booked"
            && !object.contains_key("scheduled_start")
        {
            previous_booking = merged.as_object_mut().unwrap().remove("scheduled_start");
        }
        if ["booked", "completed", "awaiting-outcome", "unbooked"]
            .contains(&tracking["schedule_state"].as_str().unwrap_or_default())
        {
            merged.as_object_mut().unwrap().remove("proposed_time");
        }
        let stage = s(&merged, "stage")?;
        s(&merged, "next_action")?;
        if !domain::STAGES.contains(&stage) {
            return Err(Error::new(
                "invalid_arguments",
                "Tracking needs stage, checked and next_action",
            ));
        }
        if let Some(url) = merged
            .get("vacancy_url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            domain::normal_url(url)?;
        }
        let previous = s(&old, "stage")?;
        let ladder = &domain::STAGES[..8];
        let regression = (previous == "closed" && stage != "closed")
            || match (
                ladder.iter().position(|x| *x == previous),
                ladder.iter().position(|x| *x == stage),
            ) {
                (Some(a), Some(b)) => b < a,
                _ => false,
            };
        if regression && correction.is_none() {
            return Err(Error::new(
                "invalid_state",
                "Older evidence cannot regress a stage; provide an explicit sourced correction reason",
            ));
        }
        let checked = s(&merged, "checked")?;
        domain::iso_datetime(checked)
            .map_err(|_| Error::new("invalid_arguments", "tracking.checked: ISO date required"))?;
        if let Some(schedule) = merged.get("schedule_state").and_then(Value::as_str)
            && !domain::SCHEDULES.contains(&schedule)
        {
            return Err(Error::new("invalid_arguments", "Invalid schedule state"));
        }
        if let Some(start) = merged.get("scheduled_start").and_then(Value::as_str)
            && (merged["schedule_state"] != "booked"
                || domain::iso_datetime(start).map_or(true, |(_, zone)| zone.is_none()))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Booked time needs an explicit timezone and booked state",
            ));
        }
        for field in ["brief", "record_evidence"] {
            if let Some(rel) = merged.get(field).and_then(Value::as_str)
                && !self.text_path(&format!("applications/{rel}"))?.is_file()
            {
                return Err(Error::new("missing_resource", format!("Missing {field}")));
            }
        }
        let block = format!(
            "<!-- tracking\n{}\n-->",
            serde_json::to_string_pretty(&merged)?
        );
        if let Some(c) = captures {
            let m = c.get(0).unwrap();
            let range = m.start()..m.end();
            text.replace_range(range, &block);
        } else {
            let (first, rest) = text.split_once('\n').unwrap_or((&text, ""));
            text = format!("{first}\n\n{block}\n{rest}");
        }
        text.push_str(&format!(
            "\n## Recorded update — {}\n\n{summary}\n\nEvidence: {}\n",
            merged["checked"].as_str().unwrap_or_default(),
            string_array(evidence)?.join(", ")
        ));
        if let Some(previous) = previous_booking {
            text.push_str(&format!(
                "\nPrevious booked time: {}. Current schedule state: {}.\n",
                previous.as_str().unwrap_or_default(),
                merged["schedule_state"].as_str().unwrap_or_default()
            ));
        }
        if let Some(reason) = correction {
            text.push_str(&format!("\nCorrection reason: {reason}\n"));
        }
        text.push_str(&format!(
            "\n<!-- career-event:{} -->\n",
            &domain::digest(key.as_bytes())[..16]
        ));
        self.change(&path, text, changes)?;
        Ok(merged)
    }
    pub(super) fn record_event(
        &self,
        tool: &str,
        args: &Value,
        state: &Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        self.active_run(state, s(args, "run_id")?)?;
        self.evidence(&args["evidence"])?;
        if args.get("tracking").is_some() && args.get("record").is_none() {
            return Err(Error::new(
                "invalid_arguments",
                "A tracking update needs a canonical record",
            ));
        }
        let mut result = json!({"run_id":args["run_id"],"kind":if tool=="update_application"{json!("status_updated")}else{args["kind"].clone()},"summary":args["summary"],"evidence":args["evidence"]});
        if let Some(record) = args.get("record").and_then(Value::as_str) {
            self.record_path(record, true)?;
            result["record"] = json!(record);
            if let Some(tracking) = args.get("tracking") {
                result["tracking"] = self.set_tracking(
                    TrackingUpdate {
                        record,
                        tracking,
                        summary: s(args, "summary")?,
                        evidence: &args["evidence"],
                        key: s(args, "idempotency_key")?,
                        correction: args.get("correction_reason").and_then(Value::as_str),
                    },
                    changes,
                )?;
            }
        }
        Ok(result)
    }
    pub(super) fn create_application(
        &self,
        args: &Value,
        state: &Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        self.active_run(state, s(args, "run_id")?)?;
        self.evidence(&args["evidence"])?;
        let record = s(args, "record")?;
        let path = self.record_path(record, false)?;
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let valid = Regex::new(r"^[a-z0-9][a-z0-9-]+\.md$").unwrap();
        if path.exists()
            || !valid.is_match(filename)
            || [
                "readme.md",
                "shortlist.md",
                "template.md",
                "pipeline.md",
                "approved-answers.md",
                "platform-profiles.md",
            ]
            .contains(&filename)
        {
            return Err(Error::new(
                "invalid_arguments",
                "Use a new lowercase company-role-posting filename",
            ));
        }
        let duplicate = self.duplicate(
            state,
            s(args, "company")?,
            s(args, "vacancy_url")?,
            args.get("posting_id").and_then(Value::as_str),
            None,
        )?;
        if !duplicate["exact_matches"].as_array().unwrap().is_empty()
            || !duplicate["blocking_attempts"]
                .as_array()
                .unwrap()
                .is_empty()
        {
            return Err(Error::new(
                "duplicate_blocked",
                "Vacancy already recorded; update its existing record",
            ));
        }
        let tracking = json!({"stage":"saved","checked":self.today()?,"next_action":"Review fit and prepare application","company":args["company"],"vacancy_url":args["vacancy_url"],"source":args["source"],"posting_id":args.get("posting_id").cloned().unwrap_or(Value::Null)});
        let text = format!(
            "# {} — {}\n\n<!-- tracking\n{}\n-->\n\n## Vacancy and fit\n\nSource URL: {}\n\n{}\n\nEvidence: {}\n",
            s(args, "company")?,
            s(args, "role")?,
            serde_json::to_string_pretty(&tracking)?,
            s(args, "vacancy_url")?,
            s(args, "fit_reason")?,
            string_array(&args["evidence"])?.join(", ")
        );
        self.change(&path, text, changes)?;
        Ok(json!({"run_id":args["run_id"],"record":record,"tracking":tracking}))
    }
}
