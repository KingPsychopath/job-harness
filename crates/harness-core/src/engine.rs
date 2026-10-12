//! Versioned capabilities and durable local coordination. No provider or model SDK.
use crate::{
    Error, Result, contracts,
    domain::{self, SubmissionLifecycle, SubmissionState},
    pipeline, storage, workspace,
};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

mod adapters;
mod attention;
mod batch;
mod configuration;
mod context;
mod daily;
pub use daily::{pending_mail_actions, validate_daily_events};
mod query;
mod recovery;
mod runs;
mod submission;
mod tracking;

const READ_ROOTS: &[&str] = &[
    "context",
    "content",
    "applications",
    "docs",
    "config",
    ".agents",
];
const ARTIFACT_ROOTS: &[&str] = &["applications", "output", "content"];

pub struct Engine {
    root: PathBuf,
}
impl Engine {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        contracts::validate_catalog()?;
        let root = fs::canonicalize(root)?;
        if !root.join("config/career-ledger.json").is_file() {
            return Err(Error::new(
                "missing_resource",
                "Ledger is not initialized: choose the correct --root or run init for a new ledger",
            ));
        }
        for rel in [
            "applications",
            "applications/operations",
            "applications/operations/state.json",
            "applications/operations/.lock",
            "applications/operations/packets",
            "applications/operations/receipts",
        ] {
            if root.join(rel).is_symlink() {
                return Err(Error::new(
                    "unsupported_layout",
                    "Coordinator paths cannot be symlinks. Back up and move the existing files into this workspace before opening it; never reset its state.",
                ));
            }
        }
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn call(&self, tool: &str, arguments: &Value) -> Value {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.call_result(tool, arguments)
        }))
        .unwrap_or_else(|_| {
            Err(Error::new(
                "internal_contract_error",
                "Unexpected local tool failure; preserve state and report the defect.",
            ))
        });
        let out = match result {
            Ok((data, next)) => {
                let failed = data.get("failed_step").is_some_and(|x| !x.is_null());
                json!({"schema_version":2,"ok":!failed,"data":data,"error":if failed{Some(json!({"code":"batch_failed","message":format!("Batch stopped at {}",data["failed_step"].as_str().unwrap_or_default()),"retryable":false,"recovery":recovery("batch_failed")}))}else{None},"next":next})
            }
            Err(error) => self.error_envelope(error),
        };
        if let Some(spec) = contracts::specs().get(tool)
            && let Err(validation) = contracts::validate(&spec["outputSchema"], &out)
        {
            #[cfg(test)]
            eprintln!("Synthetic output contract failure for {tool}: {validation}");
            #[cfg(test)]
            if let Some(branch) = spec["outputSchema"]["properties"]["data"]["anyOf"]
                .as_array()
                .and_then(|branches| branches.iter().find(|branch| branch["type"] == "object"))
                && let Err(detail) = contracts::validate(branch, &out["data"])
            {
                eprintln!("Synthetic data contract detail: {detail}");
            }
            #[cfg(not(test))]
            let _ = validation;
            return self.error_envelope(Error::new(
                "internal_contract_error",
                if contracts::is_read_only(tool)==Some(false) {"Generated output failed validation after a committed local action. Inspect its idempotency key and source records; do not repeat an external action."} else {"Generated result does not match its declared schema"},
            ));
        }
        out
    }
    fn error_envelope(&self, error: Error) -> Value {
        let code = error.code.as_str();
        let next = if code == "pending_recovery" {
            let pending = self
                .load_state()
                .ok()
                .and_then(|s| s.get("pending").cloned())
                .unwrap_or(Value::Null);
            json!([{"tool":"rebuild_pipeline","arguments":{"idempotency_key":format!("recover:{}",&domain::digest(&job_harness_domain::canonical::ascii_compact_sorted(&pending))[..16])},"reason":"Recover this exact pending local update; inspect source records if recovery conflicts."}])
        } else {
            json!([])
        };
        json!({"schema_version":2,"ok":false,"data":null,"error":{"code":code,"message":error.message,"retryable":false,"recovery":recovery(code)},"next":next})
    }
    fn call_result(&self, tool: &str, args: &Value) -> Result<(Value, Vec<Value>)> {
        let specs = contracts::specs();
        let spec = specs
            .get(tool)
            .ok_or_else(|| Error::new("unknown_tool", "Unknown harness tool"))?;
        contracts::validate(&spec["inputSchema"], args)?;
        if matches!(tool, "query_batch" | "execute_batch") {
            return self.batch(args, tool == "query_batch");
        }
        if tool == "inspect_tools" {
            return Ok((self.inspect_tools(args)?, vec![]));
        }
        if tool == "prepare_application" {
            return self.prepare_application(args);
        }
        let readonly = contracts::is_read_only(tool).ok_or_else(|| {
            Error::new(
                "internal_contract_error",
                "Unknown compiled capability authority",
            )
        })?;
        let _guard = storage::WorkspaceLock::acquire(&self.root, !readonly)?;
        let mut state = self.load_state()?;
        if readonly && tool != "read_record" && state.get("pending").is_some() {
            return Err(Error::new(
                "pending_recovery",
                "Pending transaction: call rebuild_pipeline to recover the local update",
            ));
        }
        if !readonly {
            self.recover(&mut state)?;
        }
        let mut changes = Vec::new();
        let raw = if readonly {
            self.invoke(tool, args, &mut state, &mut changes)?
        } else {
            self.mutate(tool, args, &mut state, &mut changes)?
        };
        let data = self.compact(tool, raw, args, &state)?;
        let next = match tool {
            "bootstrap" => self.bootstrap_next(args, &data)?,
            "start_run" => vec![
                json!({"tool":"query_batch","arguments":{"steps":[{"id":"answers","tool":"get_approved_answer","arguments":{"query":args["scope"].get("query").cloned().unwrap_or(json!("application"))}}]},"reason":"Retrieve only relevant answers/records; read complete sources before acting."}),
            ],
            "prepare_packet" => vec![
                json!({"tool":"reserve_submission","arguments":{"run_id":data["run_id"],"packet_id":data["packet_id"],"idempotency_key":format!("{}:reserve",s(args,"idempotency_key")?)},"reason":"Reserve after preparing the complete form."}),
            ],
            _ => vec![],
        };
        Ok((data, next))
    }
    fn safe_path(&self, rel: &str, roots: &[&str], extensions: &[&str]) -> Result<PathBuf> {
        {
            let extensions: Vec<String> = extensions.iter().map(|e| format!(".{e}")).collect();
            let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
            storage::safe_path(&self.root, rel, roots, &refs)
        }
    }
    fn text_path(&self, rel: &str) -> Result<PathBuf> {
        self.safe_path(rel, READ_ROOTS, &["md", "json"])
    }
    fn artifact_path(&self, rel: &str, extensions: &[&str]) -> Result<PathBuf> {
        if workspace::imported_cv_sha256(rel).is_some() && extensions.contains(&"pdf") {
            workspace::cv_path(&self.root, rel)
        } else {
            self.safe_path(rel, ARTIFACT_ROOTS, extensions)
        }
    }
    fn record_path(&self, rel: &str, exists: bool) -> Result<PathBuf> {
        let path = self.safe_path(rel, &["applications"], &["md"])?;
        storage::reject_symlink_ancestors(&path)?;
        if path.parent() != Some(self.root.join("applications").as_path()) {
            return Err(Error::new(
                "path_denied",
                "Vacancy records live directly under applications/",
            ));
        }
        if exists
            && !pipeline::build(&self.root)?
                .records
                .iter()
                .any(|r| r["record"].as_str() == path.file_name().and_then(|x| x.to_str()))
        {
            return Err(Error::new(
                "invalid_state",
                "Choose a canonical vacancy record, not a view, answer bank or report",
            ));
        }
        Ok(path)
    }
    fn journal_path(&self, relative: &str) -> Result<PathBuf> {
        crate::state::journal_path(&self.root, relative)
    }
    fn load_state(&self) -> Result<Value> {
        crate::state::load(&self.root)
    }
    fn recover(&self, state: &mut Value) -> Result<()> {
        if let Some(pending) = state.get("pending").cloned() {
            let changes = pending
                .as_array()
                .ok_or_else(|| Error::new("invalid_state", "Invalid pending transaction"))?;
            for change in changes {
                let path = self.journal_path(s(change, "path")?)?;
                let current = if path.exists() {
                    json!(domain::digest(&fs::read(&path)?))
                } else {
                    Value::Null
                };
                let text = s(change, "text")?;
                if current == json!(domain::digest(text.as_bytes())) {
                    continue;
                }
                if current != change["before_sha256"] {
                    return Err(Error::new(
                        "recovery_conflict",
                        "Interrupted update conflicts with a manual edit; preserve pending transaction",
                    ));
                }
                storage::atomic_write(&path, text.as_bytes())?;
            }
            state.as_object_mut().unwrap().remove("pending");
            storage::atomic_write(
                &self.root.join("applications/operations/state.json"),
                &domain::encoded(state)?,
            )?;
            let _ = self.write_pipeline();
            let _ = storage::rebuild_index(&self.root);
        }
        Ok(())
    }
    fn commit(&self, state: &mut Value, changes: &[Value]) -> Result<()> {
        if !changes.is_empty() {
            state["pending"] = json!(changes);
            if let Some(event) = state["events"]
                .as_array_mut()
                .and_then(|events| events.last_mut())
            {
                event["journal_paths"] = json!(
                    changes
                        .iter()
                        .map(|change| change["path"].clone())
                        .collect::<Vec<_>>()
                );
            }
        }
        storage::atomic_write(
            &self.root.join("applications/operations/state.json"),
            &domain::encoded(state)?,
        )?;
        if !changes.is_empty() {
            self.recover(state)?;
        }
        Ok(())
    }
    fn write_pipeline(&self) -> Result<()> {
        pipeline::write(&self.root)?;
        Ok(())
    }
    fn mutate(
        &self,
        tool: &str,
        args: &Value,
        state: &mut Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        let key = s(args, "idempotency_key")?;
        let request_hash = domain::digest(&domain::encoded(&json!({"name":tool,"args":args}))?);
        if let Some(prior) = state["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["idempotency_key"] == key)
            .cloned()
        {
            if prior.get("superseded_by").is_some() {
                return Err(Error::new(
                    "recovery_conflict",
                    "This action was superseded by an explicit conflict resolution. Inspect its resolution and unupdated records; do not replay stale success.",
                ));
            }
            if prior["request_sha256"] != request_hash {
                return Err(Error::new(
                    "idempotency_conflict",
                    "Idempotency key reused with different input; inspect the prior action",
                ));
            }
            if tool == "begin_mail_archive"
                || (tool == "daily_update" && args["action"] == "begin_archive")
            {
                let mut result = prior["result"].clone();
                result["replayed"] = json!(true);
                result["permits"] = json!([]);
                result["ready_for_single_archive"] = json!(false);
                return Ok(result);
            }
            if tool == "begin_submission" {
                let mut result = prior["result"].clone();
                result["replayed"] = json!(true);
                result["status"] = state["attempts"][s(args, "attempt_id")?]["status"].clone();
                result["ready_for_single_submit"] = json!(false);
                return Ok(result);
            }
            if tool == "rebuild_pipeline" {
                return self.invoke(tool, args, state, changes);
            }
            if tool == "prepare_packet" {
                self.validate_answer_sources(&prior["result"]["packet"], "artifact_changed")?;
                for (field, path) in [("cv", "cv_path"), ("answers", "answers_path")] {
                    let bytes = fs::read(
                        self.artifact_path(s(args, path)?, &["pdf", "md", "txt", "json"])?,
                    )?;
                    if json!(domain::digest(&bytes)) != prior["result"]["packet"][field]["sha256"] {
                        return Err(Error::new(
                            "artifact_changed",
                            "Packet source bytes changed; use a new preparation key after reconciling the existing attempt",
                        ));
                    }
                }
            }
            return Ok(prior["result"].clone());
        }
        let result = self.invoke(tool, args, state, changes)?;
        state["events"].as_array_mut().unwrap().push(json!({"idempotency_key":key,"request_sha256":request_hash,"tool":tool,"at":domain::now(),"run_id":args.get("run_id").cloned().unwrap_or(Value::Null),"result":result}));
        self.commit(state, changes)?;
        Ok(result)
    }
    fn invoke(
        &self,
        tool: &str,
        args: &Value,
        state: &mut Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        match tool {
            "intake_note" | "propose_note" | "review_note" | "accept_note" | "archive_note"
            | "list_notes" => {
                let plan = match tool {
                    "intake_note" => crate::intake::record(&self.root, args)?,
                    "propose_note" => crate::intake::propose(&self.root, args)?,
                    "review_note" => crate::intake::review(&self.root, args)?,
                    "accept_note" => crate::intake::accept(&self.root, args)?,
                    "archive_note" => crate::intake::archive(&self.root, args)?,
                    _ => crate::intake::list(&self.root, args)?,
                };
                if contracts::is_read_only(tool) == Some(true) && !plan.changes.is_empty() {
                    return Err(Error::new(
                        "internal_contract_error",
                        "A read-only intake capability emitted changes",
                    ));
                }
                let mut paths: BTreeSet<String> = changes
                    .iter()
                    .filter_map(|change| change["path"].as_str().map(str::to_owned))
                    .collect();
                for change in &plan.changes {
                    let path = s(change, "path")?;
                    self.journal_path(path)?;
                    if !paths.insert(path.to_owned()) {
                        return Err(Error::new(
                            "invalid_arguments",
                            "Intake plan contains duplicate canonical paths",
                        ));
                    }
                }
                changes.extend(plan.changes);
                Ok(plan.data)
            }
            "list_operations" => self.operations(),
            "plan_workflow" => self.plan(
                s(args, "operation")?,
                args.get("scope").cloned().unwrap_or(json!({})),
            ),
            "read_record" => self.read_record(s(args, "path")?),
            "find_application" => self.find_application(args),
            "search_workspace" => {
                crate::index::search(&self.root, s(args, "query")?, n(args, "limit", 20))
            }
            "get_approved_answer" => self.answers(s(args, "query")?),
            "review_queue" => self.review_queue(args, state),
            "daily_check" => self.daily_check(args, state),
            "daily_update" => self.daily_update(args, state),
            "begin_mail_archive" => {
                let mut begin = args.clone();
                begin["action"] = json!("begin_archive");
                self.daily_update(&begin, state)
            }
            "context_pack" => self.context_pack(args),
            "workflow_check" => crate::workflow::check(&self.root, args),
            "preview_packet" => self.preview_packet(args, state),
            "check_duplicate" => self.duplicate(
                state,
                s(args, "company")?,
                s(args, "vacancy_url")?,
                args.get("posting_id").and_then(Value::as_str),
                None,
            ),
            "start_run" => self.start_run(args, state),
            "get_run" => self.get_run(args, state),
            "list_events" => self.events(args, state),
            "finish_run" => self.finish_run(args, state),
            "record_event" | "update_application" => self.record_event(tool, args, state, changes),
            "create_application" => self.create_application(args, state, changes),
            "save_receipt" => {
                self.active_run(state, s(args, "run_id")?)?;
                Ok(
                    json!({"run_id":args["run_id"],"artifact":self.copy_artifact(s(args,"path")?,"receipts")?}),
                )
            }
            "prepare_packet" => self.prepare_packet(args, state),
            "get_packet" => Ok(
                json!({"packet_id":args["packet_id"],"packet":self.packet(s(args,"packet_id")?)?}),
            ),
            "reserve_submission" => self.reserve(args, state),
            "begin_submission" => self.begin(args, state),
            "finish_submission" => self.finish_submission(args, state, changes),
            "cancel_reservation" => self.cancel(args, state),
            "reconcile_tracker" => self.reconcile(state),
            "rebuild_pipeline" => {
                self.write_pipeline()?;
                let _ = storage::rebuild_index(&self.root);
                Ok(
                    json!({"records":pipeline::build(&self.root)?.records.len(),"pipeline":"applications/pipeline.md"}),
                )
            }
            "bootstrap" => self.bootstrap(args, state),
            "plan_adapter" => self.plan_adapter(args, state),
            "record_adapter_result" => self.adapter_result(args, state, changes),
            _ => Err(Error::new("unknown_tool", "Unknown harness capability")),
        }
    }
    fn evidence(&self, evidence: &Value) -> Result<()> {
        for item in evidence
            .as_array()
            .ok_or_else(|| Error::new("invalid_arguments", "Evidence must be an array"))?
        {
            let item = item.as_str().ok_or_else(|| {
                Error::new("invalid_arguments", "Evidence must contain paths or URLs")
            })?;
            if item.starts_with("http://") || item.starts_with("https://") {
                domain::normal_url(item)?;
            } else {
                let mut roots = READ_ROOTS.to_vec();
                roots.push("output");
                let path =
                    self.safe_path(item, &roots, &["md", "json", "pdf", "jpg", "png", "txt"])?;
                if !path.is_file() {
                    return Err(Error::new(
                        "missing_resource",
                        format!("Missing evidence: {item}"),
                    ));
                }
            }
        }
        Ok(())
    }
    fn revision(&self) -> String {
        Command::new("git")
            .arg("--no-optional-locks")
            .args(["-c", "core.fsmonitor=false"])
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|x| x.trim().to_owned())
            .unwrap_or("unversioned".into())
    }
    fn dirty(&self) -> Value {
        Command::new("git")
            .arg("--no-optional-locks")
            .args(["-c", "core.fsmonitor=false"])
            .arg("-C")
            .arg(&self.root)
            .args(["status", "--porcelain"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| json!(!o.stdout.is_empty()))
            .unwrap_or(Value::Null)
    }
    fn compact(&self, tool: &str, raw: Value, args: &Value, state: &Value) -> Result<Value> {
        let offset = n(args, "offset", 0);
        let limit = n(args, "limit", 10);
        match tool {
            "bootstrap" | "daily_check" => Ok(raw),
            "daily_update" | "begin_mail_archive" => Ok(self.compact_daily_update(raw)),
            "list_operations" => Ok(
                json!({"operations":raw["workflows"].as_object().unwrap().iter().map(|(k,v)|json!({"operation":k,"skill":v["skill"],"result":v["result"]})).collect::<Vec<_>>(),"shared_reads":raw["shared_reads"]}),
            ),
            "find_application" | "get_approved_answer" => {
                let rows: Vec<_> = raw["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| {
                        if tool == "find_application" {
                            domain::record(r)
                        } else {
                            r.clone()
                        }
                    })
                    .collect();
                let (rows, page) = domain::page(&rows, offset, limit);
                Ok(json!({"matches":rows,"page":page}))
            }
            "check_duplicate" => {
                let mut result = json!({"blocked":raw["blocked"],"counts":{},"truncated":false});
                for key in [
                    "exact_matches",
                    "company_matches",
                    "blocking_records",
                    "blocking_attempts",
                ] {
                    let rows = raw[key].as_array().unwrap();
                    result["counts"][key] = json!(rows.len());
                    if rows.len() > 50 {
                        result["truncated"] = json!(true);
                    }
                    result[key] = json!(
                        rows.iter()
                            .take(50)
                            .map(|r| if key == "blocking_attempts" {
                                domain::attempt(r)
                            } else {
                                domain::record(r)
                            })
                            .collect::<Vec<_>>()
                    );
                }
                Ok(result)
            }
            "start_run" => Ok(json!({"run_id":raw["run_id"],"run":domain::run(&raw["run"])})),
            "get_run" => {
                let rows: Vec<_> = raw["attempts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(domain::attempt)
                    .collect();
                let (rows, page) = domain::page(&rows, offset, limit);
                Ok(
                    json!({"run":domain::run(&raw["run"]),"attempts":rows,"page":page,"archive_actions":raw["archive_actions"]}),
                )
            }
            "list_events" => {
                let events:Vec<_>=raw["events"].as_array().unwrap().iter().map(|e|{let mut event=json!({"idempotency_key":e["idempotency_key"],"tool":e["tool"],"at":e["at"]});if let Some(v)=e.get("run_id"){event["run_id"]=v.clone();}
if let Some(v)=e["result"].get("summary"){event["summary"]=v.clone();}
if let Some(v)=e["result"].get("record"){event["record_path"]=v.clone();}event}).collect();
                let total = raw["total_matches"].as_u64().unwrap_or(0) as usize;
                Ok(
                    json!({"events":events,"page":{"total":total,"offset":offset,"limit":limit,"next_offset":if offset+limit<total{Some(offset+limit)}else{None}}}),
                )
            }
            "reconcile_tracker" => {
                let rows: Vec<_> = raw["unresolved_attempts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(domain::attempt)
                    .collect();
                let (rows, page) = domain::page(&rows, offset, limit);
                Ok(
                    json!({"records":raw["records"],"legacy_records":raw["legacy_records"],"needs_review_count":raw["needs_review"].as_array().unwrap().len(),"pipeline_stale":raw["pipeline_stale"],"unfinished_run_count":raw["unfinished_runs"].as_array().unwrap().len(),"unresolved_attempt_count":raw["unresolved_attempts"].as_array().unwrap().len(),"unfinished_runs":raw["unfinished_runs"].as_array().unwrap().iter().take(50).map(domain::run).collect::<Vec<_>>(),"unresolved_attempts":rows,"page":page,"pending_mail_action_count":pending_mail_actions(state).len(),"pending_mail_actions":pending_mail_actions(state).into_iter().take(50).map(|row|daily::public_archive_row(&row)).collect::<Vec<_>>()}),
                )
            }
            "reserve_submission" => {
                let a = &state["attempts"][raw["attempt"]["id"].as_str().unwrap_or_default()];
                Ok(
                    json!({"run_id":raw["run_id"],"attempt_id":a["id"],"packet_id":a["packet_id"],"status":a["status"],"record_path":a["record"]}),
                )
            }
            _ => {
                let mut result = raw;
                if let Some(object) = result.as_object_mut() {
                    for key in ["note", "instruction", "plan"] {
                        object.remove(key);
                    }
                    if let Some(record) = object.remove("record") {
                        object.insert("record_path".into(), record);
                    }
                    if let Some(packet) = object.get_mut("packet") {
                        *packet = domain::packet(packet);
                    }
                }
                Ok(result)
            }
        }
    }
}
fn s<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("Missing or invalid {key}")))
}
fn n(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|x| x as usize)
        .unwrap_or(default)
}
fn words(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}
fn string_array(value: &Value) -> Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(|| Error::new("invalid_arguments", "Expected an array"))?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::new("invalid_arguments", "Expected text values"))
        })
        .collect()
}
fn tracking_pattern() -> Regex {
    Regex::new(r"(?s)<!-- tracking\n(.*?)\n-->").unwrap()
}
fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}
fn envelope_error(value: &Value) -> Error {
    Error::new(
        value["error"]["code"].as_str().unwrap_or("invalid_state"),
        value["error"]["message"]
            .as_str()
            .unwrap_or("Local tool failed"),
    )
}
pub fn resolve_refs(value: &Value, values: &BTreeMap<String, Value>) -> Result<Value> {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref") {
                if map.len() != 1 || !reference.is_string() {
                    return Err(Error::new("invalid_reference", "Malformed reference"));
                }
                let parts: Vec<_> = reference.as_str().unwrap().split('.').collect();
                if parts.len() < 2 {
                    return Err(Error::new(
                        "invalid_reference",
                        "Reference must identify a prior successful result field",
                    ));
                }
                let mut item = values.get(parts[0]).ok_or_else(|| {
                    Error::new(
                        "invalid_reference",
                        "Reference must identify a prior successful result field",
                    )
                })?;
                for part in parts.iter().skip(1) {
                    item = match item {
                        Value::Array(rows) => part.parse::<usize>().ok().and_then(|i| rows.get(i)),
                        Value::Object(map) => map.get(*part),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        Error::new(
                            "invalid_reference",
                            "Reference must identify a prior successful result field",
                        )
                    })?;
                }
                return Ok(item.clone());
            }
            Ok(Value::Object(
                map.iter()
                    .map(|(k, v)| Ok((k.clone(), resolve_refs(v, values)?)))
                    .collect::<Result<_>>()?,
            ))
        }
        Value::Array(rows) => Ok(Value::Array(
            rows.iter()
                .map(|v| resolve_refs(v, values))
                .collect::<Result<_>>()?,
        )),
        _ => Ok(value.clone()),
    }
}
fn recovery(code: &str) -> &'static str {
    match code {
        "invalid_arguments" => "Correct arguments using the tool input schema.",
        "unknown_tool" => "Discover tools/list or CLI schemas; use a listed tool.",
        "path_denied" => "Use a workspace-relative path returned by a record tool.",
        "stale_record" => {
            "Read the current canonical record, review the intended change and use its new source hash; no record update was committed."
        }
        "missing_resource" => {
            "Configure or create the missing ledger file; do not substitute another identity."
        }
        "pending_recovery" => "Call rebuild_pipeline to finish the interrupted local update.",
        "recovery_conflict" => {
            "Inspect pending and current records; preserve both and reconcile deliberately."
        }
        "duplicate_blocked" => "Read exact/employer records and live outcome; do not resubmit.",
        "answers_changed" | "artifact_changed" => {
            "Cancel only an unstarted reservation and use a new preparation key; reconcile a begun submission before any retry."
        }
        "submit_replay_blocked" => {
            "Inspect external outcome and resolve the existing attempt; do not submit again."
        }
        "cap_reached" => "Finish this bounded run; do not silently increase its cap.",
        "idempotency_conflict" => {
            "Inspect the saved action. Changed input needs a new key only for a genuinely new action."
        }
        "invalid_reference" => {
            "Use an exact reference to a prior successful step, e.g. start.data.run_id."
        }
        "batch_boundary" => {
            "Use a separate begin_submission call immediately before one external submit."
        }
        "adapter_mismatch" => {
            "Inspect live provider state and resolve the mismatch; no verified sync was recorded."
        }
        "batch_failed" => {
            "Inspect the failed step. Earlier steps remain committed; retry identical calls with the same keys only after resolving the failure."
        }
        "io_error" => "Inspect local storage and recover pending writes before retrying.",
        "internal_contract_error" => {
            "Stop and report this tool contract defect; no extra external action is permitted."
        }
        _ => "Inspect the referenced record/run and reconcile its current state before continuing.",
    }
}

#[cfg(test)]
mod tests;
