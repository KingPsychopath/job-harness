//! Human selection, one-shot archive permits and durable outcome recovery.
use super::*;

impl Engine {
    pub(in crate::engine) fn daily_update(&self, args: &Value, state: &Value) -> Result<Value> {
        let run = self.active_run(state, s(args, "run_id")?)?;
        if !["daily_update", "tidy_inbox", "review_inbox"].contains(&s(&run, "operation")?) {
            return Err(Error::new(
                "invalid_state",
                "Daily updates require a review run; applications are outside this workflow",
            ));
        }
        let operation = s(args, "action")?;
        if operation == "record_review" {
            let path = s(args, "mail_observations_path")?;
            let (snapshot, hash, bytes) =
                self.daily_source(path, Some(s(args, "expected_source_sha256")?))?;
            let plan = self.daily_plan(&snapshot, path, &hash)?;
            let target = self.safe_path(
                s(&plan["frozen_source"], "path")?,
                &["applications"],
                &["json"],
            )?;
            if target.exists() && fs::read(&target)? != bytes {
                return Err(Error::new(
                    "artifact_changed",
                    "Preserved mail evidence differs from its digest",
                ));
            }
            if !target.exists() {
                storage::atomic_write(&target, &bytes)?;
            }
            return Ok(
                json!({"run_id":args["run_id"],"action":operation,"plan_id":plan["plan_id"],"plan_sha256":plan["plan_sha256"],"plan":plan,"archive_actions":[],"external_actions":false,"applications":false}),
            );
        }
        let plan = self.preserved_plan(state, args)?;
        let existing = mail_actions(state);
        let mut updates = Vec::new();
        let mut permits = Vec::new();
        if ["approve_archive", "begin_archive", "withdraw_archive"].contains(&operation) {
            let selected = string_array(&args["selected_action_ids"])?;
            if selected.is_empty()
                || selected.len() > 50
                || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
            {
                return Err(invalid("Select exact unique archive action IDs"));
            }
            let generation = self.restore_generation()?;
            let fresh = if operation == "begin_archive" {
                let (snapshot, hash, bytes) = self.daily_source(
                    s(args, "mail_observations_path")?,
                    Some(s(args, "expected_source_sha256")?),
                )?;
                let observed = chrono::DateTime::parse_from_rfc3339(s(&snapshot, "observed_at")?)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                let age = chrono::Utc::now()
                    .signed_duration_since(observed)
                    .num_seconds();
                if !(0..=300).contains(&age) {
                    return Err(Error::new(
                        "stale_plan",
                        "Begin requires a host observation from the last five minutes, never a future timestamp",
                    ));
                }
                let current =
                    self.daily_plan(&snapshot, s(args, "mail_observations_path")?, &hash)?;
                let target = self.safe_path(
                    s(&current["frozen_source"], "path")?,
                    &["applications"],
                    &["json"],
                )?;
                if target.exists() && fs::read(&target)? != bytes {
                    return Err(Error::new(
                        "artifact_changed",
                        "Preserved fresh mail source changed",
                    ));
                }
                if !target.exists() {
                    storage::atomic_write(&target, &bytes)?;
                }
                Some(current)
            } else {
                None
            };
            for id in selected {
                let definition = plan["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["action_id"] == id)
                    .ok_or_else(|| invalid("Action is not part of this exact plan"))?;
                if operation != "withdraw_archive"
                    && existing.values().any(|row| {
                        ["started", "outcome_unknown"]
                            .contains(&row["status"].as_str().unwrap_or(""))
                            && overlap(&row["definition"], definition)
                    })
                {
                    return Err(Error::new(
                        "invalid_state",
                        "An overlapping mailbox target has an unresolved begun action; reconcile it before any new action",
                    ));
                }
                let prior = existing.get(&id);
                if operation == "withdraw_archive" {
                    let prior=prior.filter(|row|row["status"]=="queued"&&row["plan_id"]==plan["plan_id"]).ok_or_else(||Error::new("invalid_state","Only this exact queued selection may be withdrawn; begun or unknown work requires reconciliation"))?;
                    if !identity(s(args, "approved_by")?)
                        || s(args, "approval_evidence")?.trim().is_empty()
                        || s(args, "reason")?.trim().is_empty()
                    {
                        return Err(invalid(
                            "Withdrawal needs explicit human attribution, confirmation evidence and reason",
                        ));
                    }
                    let mut row = prior.clone();
                    row["status"] = json!("withdrawn");
                    row["run_id"] = args["run_id"].clone();
                    row["withdrawn_by"] = args["approved_by"].clone();
                    row["withdrawal_evidence"] = args["approval_evidence"].clone();
                    row["withdrawal_reason"] = args["reason"].clone();
                    row["withdrawn_at"] = json!(domain::now());
                    updates.push(row);
                } else if operation == "approve_archive" {
                    if prior.is_some_and(|p| {
                        ["started", "archived", "outcome_unknown"]
                            .contains(&p["status"].as_str().unwrap_or(""))
                    }) {
                        return Err(Error::new(
                            "invalid_state",
                            "This archive action is already begun or resolved; inspect current mailbox evidence",
                        ));
                    }
                    if !identity(s(args, "approved_by")?)
                        || s(args, "approval_evidence")?.trim().is_empty()
                    {
                        return Err(invalid(
                            "Explicit human confirmation attribution and evidence are required",
                        ));
                    }
                    updates.push(json!({"action_id":id,"plan_id":plan["plan_id"],"plan_sha256":plan["plan_sha256"],"definition":definition,"status":"queued","run_id":args["run_id"],"approved_by":args["approved_by"],"approval_evidence":args["approval_evidence"],"confirmation":"host_attestation_not_authenticated_actor","approved_at":domain::now(),"restore_generation":generation}));
                } else {
                    let prior=prior.filter(|p|p["status"]=="queued"&&p["plan_id"]==plan["plan_id"]).ok_or_else(||Error::new("invalid_state","Exact human confirmation is required; begun/unknown actions never reissue a permit"))?;
                    let approved = chrono::DateTime::parse_from_rfc3339(s(prior, "approved_at")?)
                        .map_err(|_| {
                            Error::new("invalid_state", "Invalid archive approval timestamp")
                        })?
                        .with_timezone(&chrono::Utc);
                    let age = chrono::Utc::now()
                        .signed_duration_since(approved)
                        .num_seconds();
                    if !(0..=86400).contains(&age) {
                        return Err(Error::new(
                            "stale_plan",
                            "Archive confirmation expired; review and confirm this exact selection again",
                        ));
                    }
                    if prior["restore_generation"] != generation {
                        return Err(Error::new(
                            "stale_plan",
                            "Workspace was restored; review and confirm the archive selection again",
                        ));
                    }
                    if !fresh.as_ref().unwrap()["actions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a == definition)
                    {
                        return Err(Error::new(
                            "stale_plan",
                            "Fresh thread membership, latest message, content, labels or request state changed",
                        ));
                    }
                    let mut row = prior.clone();
                    row["status"] = json!("started");
                    row["run_id"] = args["run_id"].clone();
                    row["started_at"] = json!(domain::now());
                    row["fresh_source"] = fresh.as_ref().unwrap()["frozen_source"].clone();
                    updates.push(row);
                    permits.push(json!({"action_id":id,"definition":definition,"ready_for_single_archive":true}));
                }
            }
        } else if ["record_archive_outcome", "reconcile_archive"].contains(&operation) {
            let results = args["results"]
                .as_array()
                .filter(|r| !r.is_empty() && r.len() <= 50)
                .ok_or_else(|| invalid("Supply bounded per-action mailbox results"))?;
            let mut seen = BTreeSet::new();
            for result in results {
                let id = s(result, "action_id")?;
                if !seen.insert(id) {
                    return Err(invalid("Duplicate archive result"));
                }
                let prior = existing
                    .get(id)
                    .filter(|p| p["plan_id"] == plan["plan_id"])
                    .ok_or_else(|| invalid("Archive result lacks its approved action"))?;
                let outcome = s(result, "outcome")?;
                if !["archived", "failed", "outcome_unknown"].contains(&outcome) {
                    return Err(invalid("Unknown archive outcome"));
                }
                let proof = digest(result);
                if ["archived", "failed"].contains(&prior["status"].as_str().unwrap_or("")) {
                    if prior["result_sha256"] != proof {
                        return Err(Error::new(
                            "idempotency_conflict",
                            "Conflicting archive outcome replay",
                        ));
                    }
                    updates.push(prior.clone());
                    continue;
                }
                if !["started", "outcome_unknown"].contains(&prior["status"].as_str().unwrap_or(""))
                {
                    return Err(Error::new(
                        "invalid_state",
                        "Archive was not begun; a queued plan is not an external action",
                    ));
                }
                let readback = if outcome != "outcome_unknown" {
                    self.evidence(&result["evidence"])?;
                    if result["evidence"].as_array().is_none_or(Vec::is_empty) {
                        return Err(invalid("Verified mailbox readback requires evidence"));
                    }
                    let differences = validate_readback(&prior["definition"], result)?;
                    let source = self.preserve_mail_readback(prior, result)?;
                    Some((differences, source))
                } else {
                    None
                };
                let mut row = prior.clone();
                if let Some((differences, source)) = readback {
                    row["label_changes"] = json!(has_label_drift(&differences));
                    row["label_differences"] = differences;
                    row["readback"] = source;
                }
                row["status"] = json!(outcome);
                row["result_sha256"] = json!(proof);
                row["result"] = result.clone();
                row["observed_final_state_only"] = json!(true);
                updates.push(row);
            }
        } else {
            return Err(invalid("Unknown daily update action"));
        }
        if storage::encoded(&json!(permits)).len() > 40_000 {
            return Err(invalid(
                "Selected archive permit exceeds the bounded response; choose fewer messages or smaller thread groups",
            ));
        }
        Ok(
            json!({"run_id":args["run_id"],"action":operation,"plan_id":plan["plan_id"],"approved_action_ids":updates.iter().map(|a|a["action_id"].clone()).collect::<Vec<_>>(),"withdrawn_action_ids":if operation=="withdraw_archive"{updates.iter().map(|a|a["action_id"].clone()).collect::<Vec<_>>()}else{vec![]},"archive_actions":updates,"status":if operation=="withdraw_archive"{"withdrawn"}else if operation=="approve_archive"{"queued"}else if operation=="begin_archive"{"started"}else{"recorded"},"permits":permits,"ready_for_single_archive":operation=="begin_archive","external_actions":false,"applications":false}),
        )
    }
}
