//! Freeze, reserve, begin once and resolve evidence. Unknown attempts never expire.
use super::*;

impl Engine {
    pub(super) fn copy_artifact(&self, source: &str, folder: &str) -> Result<Value> {
        let path = self.artifact_path(source, &["pdf", "md", "txt", "json", "jpg", "png"])?;
        let bytes = fs::read(&path)?;
        if bytes.len() > 30_000_000 {
            return Err(Error::new("invalid_arguments", "Artifact exceeds 30 MB"));
        }
        let sha = domain::digest(&bytes);
        if workspace::imported_cv_sha256(source).is_some_and(|expected| expected != sha) {
            return Err(Error::new(
                "artifact_changed",
                "The saved CV changed while preparing application materials. Review and import the intended CV again.",
            ));
        }
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let rel = format!("applications/operations/{folder}/{sha}.{ext}");
        let target = self.safe_path(
            &rel,
            &["applications"],
            &["pdf", "md", "txt", "json", "jpg", "png"],
        )?;
        if target.exists() && fs::read(&target)? != bytes {
            return Err(Error::new(
                "artifact_changed",
                "Immutable artifact hash mismatch",
            ));
        }
        if !target.exists() {
            storage::atomic_write(&target, &bytes)?;
        }
        Ok(json!({"path":rel,"sha256":sha,"original_path":source}))
    }
    pub(super) fn prepare_packet(&self, args: &Value, state: &Value) -> Result<Value> {
        let run = self.active_run(state, s(args, "run_id")?)?;
        if run["operation"] != "apply" {
            return Err(Error::new(
                "invalid_state",
                "Submission packets require an apply run",
            ));
        }
        let record = s(args, "record")?;
        self.record_path(record, true)?;
        self.check_scope(&run, args)?;
        let company = s(args, "company")?;
        let vacancy_url = s(args, "vacancy_url")?;
        let duplicate = self.duplicate(
            state,
            company,
            vacancy_url,
            args.get("posting_id").and_then(Value::as_str),
            None,
        )?;
        let bound=duplicate["exact_matches"].as_array().unwrap().iter().find(|r|r["record"].as_str()==Path::new(record).file_name().and_then(|x|x.to_str())).ok_or_else(||Error::new("invalid_state","Packet vacancy identity does not match its canonical record; record the identity first"))?;
        let employer = bound
            .get("company")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                Regex::new(r"\s+[—–-]\s+")
                    .unwrap()
                    .split(bound["title"].as_str().unwrap_or_default())
                    .next()
                    .unwrap_or_default()
            });
        if domain::company_key(company) != domain::company_key(employer) {
            return Err(Error::new(
                "invalid_state",
                "Packet company does not match its canonical record",
            ));
        }
        if duplicate["blocked"] == true {
            return Err(Error::new(
                "duplicate_blocked",
                "Duplicate or uncertain prior submission; reconcile it first",
            ));
        }
        if Path::new(s(args, "cv_path")?)
            .extension()
            .and_then(|x| x.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("pdf"))
            || !["md", "txt", "json"].contains(
                &Path::new(s(args, "answers_path")?)
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or_default(),
            )
        {
            return Err(Error::new(
                "invalid_arguments",
                "Packet needs a PDF and an exact answer/message file",
            ));
        }
        let mut source_hashes = serde_json::Map::new();
        for source in self.settings()?["answer_sources"].as_array().unwrap() {
            let source = source.as_str().unwrap();
            source_hashes.insert(
                source.into(),
                json!(domain::digest(&fs::read(self.text_path(source)?)?)),
            );
        }
        let packet = json!({"record":record,"company":company,"vacancy_url":domain::normal_url(vacancy_url)?,"source":args["source"],"posting_id":args.get("posting_id").cloned().unwrap_or(Value::Null),"cv":self.copy_artifact(s(args,"cv_path")?,"packets")?,"answers":self.copy_artifact(s(args,"answers_path")?,"packets")?,"source_record":self.copy_artifact(record,"packets")?,"answer_source_sha256":source_hashes,"ledger_revision":self.revision(),"ledger_dirty":self.dirty()});
        let id = domain::digest(&domain::encoded(&packet)?);
        storage::atomic_write(
            &self
                .root
                .join(format!("applications/operations/packets/{id}.json")),
            &domain::encoded(&packet)?,
        )?;
        Ok(json!({"run_id":args["run_id"],"packet_id":id,"packet":packet}))
    }
    pub(super) fn check_scope(&self, run: &Value, packet: &Value) -> Result<()> {
        if let Some(record) = run["scope"].get("record")
            && record != &packet["record"]
        {
            return Err(Error::new(
                "invalid_state",
                "Packet record outside this run",
            ));
        }
        if let Some(company) = run["scope"].get("company").and_then(Value::as_str)
            && domain::company_key(company) != domain::company_key(s(packet, "company")?)
        {
            return Err(Error::new(
                "invalid_state",
                "Packet company outside this run",
            ));
        }
        if !run["scope"]["sources"]
            .as_array()
            .is_some_and(|a| a.contains(&packet["source"]))
        {
            return Err(Error::new(
                "invalid_state",
                "Packet source outside this run",
            ));
        }
        Ok(())
    }
    pub(super) fn packet(&self, id: &str) -> Result<Value> {
        if id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::new("invalid_arguments", "Invalid packet ID"));
        }
        let path = self.safe_path(
            &format!("applications/operations/packets/{id}.json"),
            &["applications"],
            &["json"],
        )?;
        let packet: Value = serde_json::from_slice(&fs::read(path)?)?;
        if domain::digest(&domain::encoded(&packet)?) != id {
            return Err(Error::new(
                "artifact_changed",
                "Packet was changed after preparation",
            ));
        }
        for field in ["cv", "answers", "source_record"] {
            let item = &packet[field];
            let path = self.safe_path(
                s(item, "path")?,
                ARTIFACT_ROOTS,
                &["pdf", "md", "txt", "json"],
            )?;
            if json!(domain::digest(&fs::read(path)?)) != item["sha256"] {
                return Err(Error::new(
                    "artifact_changed",
                    "Submitted artifact was changed after preparation",
                ));
            }
        }
        Ok(packet)
    }
    pub(super) fn validate_answer_sources(&self, packet: &Value, code: &str) -> Result<()> {
        let settings = self.settings()?;
        let sources = settings["answer_sources"].as_array().unwrap();
        let hashes = packet["answer_source_sha256"]
            .as_object()
            .ok_or_else(|| Error::new("artifact_changed", "Packet lacks answer source hashes"))?;
        if sources.len() != hashes.len()
            || sources
                .iter()
                .any(|s| !hashes.contains_key(s.as_str().unwrap_or_default()))
        {
            return Err(Error::new(
                code,
                "Canonical answers changed; prepare a new packet after cancelling only an unstarted reservation",
            ));
        }
        for source in sources {
            let path = source.as_str().unwrap();
            if json!(domain::digest(&fs::read(self.text_path(path)?)?)) != hashes[path] {
                return Err(Error::new(
                    code,
                    "Canonical answers changed; prepare a new packet after cancelling only an unstarted reservation",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn reserve(&self, args: &Value, state: &mut Value) -> Result<Value> {
        let id = s(args, "run_id")?;
        let run = self.active_run(state, id)?;
        let packet = self.packet(s(args, "packet_id")?)?;
        if run["operation"] != "apply"
            || run["scope"]
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            return Err(Error::new(
                "invalid_state",
                "Dry runs cannot reserve a real submission",
            ));
        }
        self.check_scope(&run, &packet)?;
        let attempts: Vec<_> = state["attempts"]
            .as_object()
            .unwrap()
            .values()
            .filter(|a| a["run_id"] == id)
            .collect();
        let cap = run["scope"]["max_submissions"].as_u64().unwrap_or(0);
        if attempts.len() as u64
            >= run["scope"]
                .get("max_attempts")
                .and_then(Value::as_u64)
                .unwrap_or(cap * 3)
        {
            return Err(Error::new(
                "cap_reached",
                "Total attempt cap reached, including negative/cancelled attempts",
            ));
        }
        if attempts
            .iter()
            .filter(|a| a["status"] != "cancelled")
            .count() as u64
            >= cap
        {
            return Err(Error::new("cap_reached", "Submission cap reached"));
        }
        if self.duplicate(
            state,
            s(&packet, "company")?,
            s(&packet, "vacancy_url")?,
            packet.get("posting_id").and_then(Value::as_str),
            None,
        )?["blocked"]
            == true
        {
            return Err(Error::new(
                "duplicate_blocked",
                "Submission already reserved, submitted or uncertain",
            ));
        }
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let mut attempt = packet;
        attempt["id"] = json!(attempt_id);
        attempt["run_id"] = json!(id);
        attempt["packet_id"] = args["packet_id"].clone();
        attempt["status"] = json!(SubmissionState::Reserved);
        attempt["reserved_at"] = json!(domain::now());
        state["attempts"][&attempt_id] = attempt.clone();
        Ok(json!({"run_id":id,"attempt":attempt}))
    }
    pub(super) fn begin(&self, args: &Value, state: &mut Value) -> Result<Value> {
        let id = s(args, "attempt_id")?;
        let attempt = state["attempts"].get(id).cloned().ok_or_else(|| {
            Error::new(
                "submit_replay_blocked",
                "Only a reserved attempt can begin; never repeat a submitting/unknown attempt",
            )
        })?;
        if SubmissionState::parse(&attempt["status"])? != SubmissionState::Reserved {
            return Err(Error::new(
                "submit_replay_blocked",
                "Only a reserved attempt can begin; never repeat a submitting/unknown attempt",
            ));
        }
        self.active_run(state, s(&attempt, "run_id")?)?;
        let packet = self.packet(s(&attempt, "packet_id")?)?;
        self.validate_answer_sources(&packet, "answers_changed")?;
        if self.duplicate(
            state,
            s(&packet, "company")?,
            s(&packet, "vacancy_url")?,
            packet.get("posting_id").and_then(Value::as_str),
            Some(id),
        )?["blocked"]
            == true
        {
            return Err(Error::new(
                "duplicate_blocked",
                "A later ledger update now blocks submission",
            ));
        }
        state["attempts"][id]["status"] = json!(SubmissionState::Submitting);
        state["attempts"][id]["began_at"] = json!(domain::now());
        Ok(
            json!({"run_id":attempt["run_id"],"attempt_id":id,"status":"submitting","ready_for_single_submit":true,"packet":packet}),
        )
    }
    pub(super) fn finish_submission(
        &self,
        args: &Value,
        state: &mut Value,
        changes: &mut Vec<Value>,
    ) -> Result<Value> {
        let id = s(args, "attempt_id")?;
        let attempt = state["attempts"].get(id).cloned().ok_or_else(|| {
            Error::new(
                "invalid_state",
                "Only submitting/unknown attempts can be resolved",
            )
        })?;
        if !SubmissionState::parse(&attempt["status"])?.may_resolve() {
            return Err(Error::new(
                "invalid_state",
                "Only submitting/unknown attempts can be resolved",
            ));
        }
        self.evidence(&args["evidence"])?;
        let outcome = s(args, "outcome")?;
        if ["submitted", "not_submitted"].contains(&outcome)
            && args["evidence"].as_array().unwrap().is_empty()
        {
            return Err(Error::new(
                "invalid_arguments",
                "Confirmed or negative submission resolution needs evidence, not elapsed time",
            ));
        }
        let record = s(&attempt, "record")?;
        let current=pipeline::build(&self.root)?.records.into_iter().find(|r|r["record"].as_str()==Path::new(record).file_name().and_then(|x|x.to_str())).ok_or_else(||Error::new("missing_resource",format!("Missing canonical record {record}; restore or reconcile this exact linked record before resolving the attempt")))?;
        let receipt = args
            .get("receipt_path")
            .and_then(Value::as_str)
            .map(|p| self.copy_artifact(p, "receipts"))
            .transpose()?;
        let stage = match outcome {
            "submitted" => "applied",
            "outcome_unknown" => "needs-review",
            "not_submitted" => "prepared",
            _ => {
                return Err(Error::new(
                    "invalid_arguments",
                    "Unknown submission outcome",
                ));
            }
        };
        let mut tracking = json!({"stage":stage,"checked":self.today()?,"status_detail":args["summary"],"next_action":match outcome{"submitted"=>"Monitor for employer response","outcome_unknown"=>"Resolve submission outcome before retrying",_=>"Prepare a new verified attempt"},"company":attempt["company"],"vacancy_url":attempt["vacancy_url"],"posting_id":attempt["posting_id"],"source":attempt["source"],"record_evidence":format!("operations/packets/{}.json",s(&attempt,"packet_id")?)});
        let ladder = &domain::STAGES[..8];
        let stronger = current["stage"] == "closed"
            || ladder
                .iter()
                .position(|s| current["stage"] == *s)
                .is_some_and(|index| index > ladder.iter().position(|s| *s == stage).unwrap_or(1));
        if stronger {
            for field in ["stage", "next_action", "status_detail"] {
                if let Some(v) = current.get(field) {
                    tracking[field] = v.clone();
                }
            }
        }
        let mut links = vec![json!(format!(
            "applications/operations/packets/{}.json",
            s(&attempt, "packet_id")?
        ))];
        links.extend(args["evidence"].as_array().unwrap().clone());
        if let Some(r) = &receipt {
            links.push(r["path"].clone());
        }
        self.set_tracking(
            tracking::TrackingUpdate {
                record,
                tracking: &tracking,
                summary: s(args, "summary")?,
                evidence: &json!(links),
                key: s(args, "idempotency_key")?,
                correction: None,
                expected_record_sha256: None,
            },
            changes,
        )?;
        let updated = &mut state["attempts"][id];
        updated["status"] = json!(if outcome == "not_submitted" {
            "cancelled"
        } else {
            outcome
        });
        updated["resolved_at"] = json!(domain::now());
        updated["summary"] = args["summary"].clone();
        updated["evidence"] = args["evidence"].clone();
        updated["receipt"] = receipt.unwrap_or(Value::Null);
        Ok(
            json!({"run_id":attempt["run_id"],"attempt_id":id,"status":updated["status"],"record":record,"evidence":links}),
        )
    }
    pub(super) fn cancel(&self, args: &Value, state: &mut Value) -> Result<Value> {
        let id = s(args, "attempt_id")?;
        let attempt = state["attempts"].get_mut(id).ok_or_else(|| {
            Error::new(
                "invalid_state",
                "Only an unstarted reservation can be cancelled without outcome evidence",
            )
        })?;
        if SubmissionState::parse(&attempt["status"])? != SubmissionState::Reserved {
            return Err(Error::new(
                "invalid_state",
                "Only an unstarted reservation can be cancelled without outcome evidence",
            ));
        }
        attempt["status"] = json!(SubmissionState::Cancelled);
        attempt["summary"] = args["summary"].clone();
        attempt["resolved_at"] = json!(domain::now());
        Ok(
            json!({"run_id":attempt["run_id"],"attempt_id":id,"status":"cancelled","summary":args["summary"]}),
        )
    }
    pub(super) fn prepare_application(&self, args: &Value) -> Result<(Value, Vec<Value>)> {
        let key = s(args, "idempotency_key")?;
        let mut packet_args = args.clone();
        packet_args["idempotency_key"] = json!(format!("{key}:packet"));
        let p = self.call("prepare_packet", &packet_args);
        if p["ok"] != true {
            return Err(envelope_error(&p));
        }
        let a=self.call("reserve_submission",&json!({"run_id":args["run_id"],"packet_id":p["data"]["packet_id"],"idempotency_key":format!("{key}:reserve")}));
        if a["ok"] != true {
            return Err(envelope_error(&a));
        }
        let _guard = storage::WorkspaceLock::acquire(&self.root, false)?;
        let state = self.load_state()?;
        if state.get("pending").is_some() {
            return Err(Error::new(
                "pending_recovery",
                "Pending transaction: call rebuild_pipeline",
            ));
        }
        let attempt = &state["attempts"][a["data"]["attempt_id"].as_str().unwrap_or_default()];
        let active = state["runs"][s(args, "run_id")?]["status"] == "running";
        let status = &attempt["status"];
        let data = json!({"run_id":args["run_id"],"packet_id":p["data"]["packet_id"],"attempt_id":attempt["id"],"status":status,"packet":p["data"]["packet"]});
        let next = if status == "reserved" {
            json!({"tool":"begin_submission","arguments":{"attempt_id":attempt["id"],"idempotency_key":format!("{key}:begin")},"reason":"Only at the external submit boundary; verify current authority/form first."})
        } else if status == "cancelled" && active {
            let mut replacement = args.clone();
            replacement["idempotency_key"] = json!(format!(
                "replacement:{}",
                &domain::digest(&domain::encoded(
                    &json!({"key":key,"attempt_id":attempt["id"]})
                )?)[..32]
            ));
            json!({"tool":"prepare_application","arguments":replacement,"reason":"Only if the cancellation cause is resolved and this role is still in scope; otherwise stop. A new preparation still checks budget and duplicates."})
        } else {
            json!({"tool":"get_run","arguments":{"run_id":args["run_id"]},"reason":"This existing attempt has progressed; inspect it without repeating Submit."})
        };
        Ok((data, vec![next]))
    }
}
