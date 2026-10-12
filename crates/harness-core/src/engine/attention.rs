//! Local attention and packet previews. No inbox access or submission authority.
use super::*;

impl Engine {
    pub(super) fn review_queue(&self, args: &Value, state: &Value) -> Result<Value> {
        let built = pipeline::build(&self.root)?;
        let records: BTreeMap<String, Value> = built
            .records
            .iter()
            .map(|row| {
                (
                    format!(
                        "applications/{}",
                        row["record"].as_str().unwrap_or_default()
                    ),
                    row.clone(),
                )
            })
            .collect();
        let mut items = vec![];
        let mut counts = json!({"unknown_submissions":0,"reserved_attempts":0,"prepared_applications":0,"interviews":0,"supplied_mail":0,"pending_note_reviews":0});
        for attempt in state["attempts"].as_object().unwrap().values() {
            let status = SubmissionState::parse(&attempt["status"])?;
            if !status.unresolved() {
                continue;
            }
            let kind = if status == SubmissionState::Reserved {
                "reserved_attempt"
            } else {
                "unknown_submission"
            };
            let count = if status == SubmissionState::Reserved {
                "reserved_attempts"
            } else {
                "unknown_submissions"
            };
            counts[count] = json!(counts[count].as_u64().unwrap_or(0) + 1);
            let mut item = json!({"kind":kind,"attempt_id":attempt["id"],"run_id":attempt["run_id"],"record_path":attempt["record"],"company":attempt["company"],"status":attempt["status"],"next_action":if status==SubmissionState::Reserved{"Review the prepared form before the separate begin boundary"}else{"Inspect the external outcome before resolving or retrying"},"reason":"Persisted coordination state; elapsed time never clears this attempt."});
            if let Some(record) = records.get(s(attempt, "record")?) {
                for key in [
                    "title",
                    "stage",
                    "checked",
                    "schedule_state",
                    "scheduled_start",
                ] {
                    if let Some(value) = record.get(key) {
                        item[key] = value.clone();
                    }
                }
            }
            items.push(item);
        }
        for row in &built.records {
            let (kind, count) = match row["stage"].as_str().unwrap_or_default() {
                "prepared" => ("prepared_application", "prepared_applications"),
                "introduction" | "screening" | "technical" | "final" | "offer" => {
                    ("interview", "interviews")
                }
                _ => continue,
            };
            counts[count] = json!(counts[count].as_u64().unwrap_or(0) + 1);
            let mut item = domain::record(row);
            item["kind"] = json!(kind);
            for key in [
                "schedule_state",
                "scheduled_start",
                "proposed_time",
                "contact",
                "brief",
            ] {
                if let Some(value) = row.get(key) {
                    item[key] = value.clone();
                }
            }
            item["reason"] = json!(
                "Canonical tracking and its recorded next action; current provider state is not inferred."
            );
            items.push(item);
        }
        let mut mail_source = Value::Null;
        if let Some(path) = args.get("mail_observations_path").and_then(Value::as_str) {
            let source = self.safe_path(path, &["context", "applications"], &["json"])?;
            let bytes = fs::read(source)?;
            if bytes.len() > 1_000_000 {
                return Err(Error::new(
                    "invalid_arguments",
                    "Mail observations exceed one MB; supply a bounded observation file",
                ));
            }
            let value: Value = serde_json::from_slice(&bytes)?;
            let rows = value
                .get("items")
                .or_else(|| value.get("messages"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "Mail observation JSON requires items or messages",
                    )
                })?;
            if rows.len() > 1000 {
                return Err(Error::new(
                    "invalid_arguments",
                    "Supply at most 1000 bounded mail observations",
                ));
            }
            mail_source = json!({"path":path,"sha256":domain::digest(&bytes),"observed_at":value.get("observed_at").or_else(||value.get("checked_at")).cloned().unwrap_or(Value::Null)});
            for row in rows {
                if !row.is_object() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Every supplied mail observation must be an object",
                    ));
                }
                let needs_reply = row
                    .get("needs_reply")
                    .or_else(|| row.get("needs_attention"))
                    .and_then(Value::as_bool);
                let attribution = row.get("attribution").and_then(Value::as_str);
                let uncertain = needs_reply.is_none()
                    || row.get("needs_review") == Some(&Value::Bool(true))
                    || attribution.is_some_and(|value| value != "proposed_exact_identity");
                if needs_reply == Some(false) && !uncertain {
                    continue;
                }
                let mut item = json!({"kind":"supplied_mail","needs_reply":needs_reply,"needs_review":uncertain,"source_path":path,"source_sha256":mail_source["sha256"],"next_action":"Review the supplied observation and latest full mail before replying","reason":"Supplied mail observation, not a fresh inbox scan. Unknown request state and ambiguous attribution remain visible."});
                for key in [
                    "id",
                    "subject",
                    "company",
                    "thread_url",
                    "summary",
                    "observed_at",
                    "attribution",
                    "latest_message_id",
                ] {
                    if let Some(value) = row.get(key)
                        && value.is_string()
                    {
                        item[key] = value.clone();
                    }
                }
                if let Some(ids) = row.get("request_message_ids").filter(|value| {
                    value
                        .as_array()
                        .is_some_and(|ids| ids.iter().all(Value::is_string))
                }) {
                    item["request_message_ids"] = ids.clone();
                }
                items.push(item);
                counts["supplied_mail"] = json!(counts["supplied_mail"].as_u64().unwrap_or(0) + 1);
            }
        }
        let notes = crate::intake::list(&self.root, &json!({"limit":100}))?.data;
        let note_reviews_complete = notes["page"]["has_more"] != true;
        for note in notes["notes"].as_array().into_iter().flatten() {
            let id = s(note, "note_id")?;
            let review = crate::intake::review(&self.root, &json!({"note_id":id}))?.data;
            let accepted: BTreeSet<_> = review["accepted_reviews"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| row["proposal_id"].as_str())
                .collect();
            let proposals = review["proposals"]
                .as_array()
                .ok_or_else(|| Error::new("invalid_state", "Intake review lacks proposals"))?;
            let pending: Vec<_> = proposals
                .iter()
                .filter(|proposal| {
                    proposal["proposal_id"]
                        .as_str()
                        .is_some_and(|id| !accepted.contains(id))
                })
                .collect();
            if proposals.is_empty() {
                items.push(json!({"kind":"pending_note_review","note_id":id,"source_path":note["original_path"],"source_sha256":note["source_sha256"],"next_action":"Review the supplied original and propose any separately cited additions","reason":"This unarchived intake has no reviewed proposal; no facts or CV content are inferred."}));
                counts["pending_note_reviews"] =
                    json!(counts["pending_note_reviews"].as_u64().unwrap_or(0) + 1);
            }
            for proposal in pending {
                items.push(json!({"kind":"pending_note_review","note_id":id,"proposal_id":proposal["proposal_id"],"source_path":proposal["path"],"source_sha256":proposal["sha256"],"next_action":"Review citations and explicitly select proposal item IDs before accepting","reason":"This proposal has no explicit accepted review. Proposed text is not a confirmed career fact."}));
                counts["pending_note_reviews"] =
                    json!(counts["pending_note_reviews"].as_u64().unwrap_or(0) + 1);
            }
        }
        counts["pending_mail_actions"] = json!(pending_mail_actions(state).len());
        for row in pending_mail_actions(state) {
            items.push(json!({"kind":"mail_archive_action","action_id":row["action_id"],"plan_id":row["plan_id"],"status":row["status"],"next_action":"Inspect queued host work or reconcile observed mailbox state; never blindly retry"}));
        }
        items.sort_by(|a, b| {
            let rank = |value: &Value| match value["kind"].as_str().unwrap_or_default() {
                "unknown_submission" => 0,
                "supplied_mail" => 1,
                "interview" => 2,
                "reserved_attempt" => 3,
                "pending_note_review" => 4,
                _ => 5,
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| {
                    a["scheduled_start"]
                        .as_str()
                        .unwrap_or("9999")
                        .cmp(b["scheduled_start"].as_str().unwrap_or("9999"))
                })
                .then_with(|| {
                    a["record_path"]
                        .as_str()
                        .unwrap_or_default()
                        .cmp(b["record_path"].as_str().unwrap_or_default())
                })
        });
        let (items, page) = domain::page(&items, n(args, "offset", 0), n(args, "limit", 10));
        Ok(
            json!({"items":items,"counts":counts,"page":page,"note_reviews_complete":note_reviews_complete,"mail_source":mail_source,"authority":"recorded_local_state","live_mail_checked":false}),
        )
    }
    pub(super) fn preview_packet(&self, args: &Value, state: &Value) -> Result<Value> {
        let id = s(args, "packet_id")?;
        if id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::new("invalid_arguments", "Invalid packet ID"));
        }
        let relative = format!("applications/operations/packets/{id}.json");
        let path = self.safe_path(&relative, &["applications"], &["json"])?;
        let mut checks = vec![];
        let mut packet = Value::Null;
        if path.is_file() {
            let bytes = fs::read(path)?;
            let parsed: Value = serde_json::from_slice(&bytes)?;
            let actual = domain::digest(&domain::encoded(&parsed)?);
            checks.push(json!({"kind":"packet_metadata","path":relative,"expected_sha256":id,"actual_sha256":actual,"changed":actual!=id}));
            for field in ["cv", "answers", "source_record"] {
                let artifact = parsed.get(field).ok_or_else(|| {
                    Error::new(
                        "invalid_state",
                        "Packet is missing frozen artifact metadata",
                    )
                })?;
                let expected = s(artifact, "sha256")?;
                for (original, path) in [
                    (false, s(artifact, "path")?),
                    (true, s(artifact, "original_path")?),
                ] {
                    let target =
                        self.safe_path(path, ARTIFACT_ROOTS, &["md", "json", "pdf", "txt"])?;
                    let actual = if target.is_file() {
                        Some(domain::digest(&fs::read(target)?))
                    } else {
                        None
                    };
                    checks.push(json!({"kind":if original{"original_artifact"}else{"frozen_artifact"},"path":path,"expected_sha256":expected,"actual_sha256":actual,"changed":actual.as_deref()!=Some(expected)}));
                }
            }
            let sources = parsed["answer_source_sha256"].as_object().ok_or_else(|| {
                Error::new(
                    "invalid_state",
                    "Packet lacks canonical answer-source hashes",
                )
            })?;
            for (source, expected) in sources {
                let target = self.text_path(source)?;
                let actual = if target.is_file() {
                    Some(domain::digest(&fs::read(target)?))
                } else {
                    None
                };
                checks.push(json!({"kind":"answer_source","path":source,"expected_sha256":expected,"actual_sha256":actual,"changed":actual.as_ref().map(|x|json!(x))!=Some(expected.clone())}));
            }
            let settings = self.settings()?;
            let current: BTreeSet<_> = settings["answer_sources"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let frozen: BTreeSet<_> = sources.keys().map(String::as_str).collect();
            if current != frozen {
                checks.push(json!({"kind":"answer_configuration","path":"config/career-ledger.json","expected_sha256":domain::digest(&domain::encoded(&json!(frozen))?),"actual_sha256":domain::digest(&domain::encoded(&json!(current))?),"changed":true}));
            }
            packet = domain::packet(&parsed);
        } else {
            checks.push(json!({"kind":"packet_metadata","path":relative,"expected_sha256":id,"actual_sha256":null,"changed":true}));
        }
        let attempts: Vec<_> = state["attempts"]
            .as_object()
            .unwrap()
            .values()
            .filter(|attempt| attempt["packet_id"] == id)
            .map(domain::attempt)
            .collect();
        let (attempts, page) = domain::page(&attempts, 0, 50);
        let stale = checks.iter().any(|check| check["changed"] == true);
        Ok(
            json!({"packet_id":id,"packet":packet,"stale":stale,"checks":checks,"attempts":attempts,"page":page,"authority":"preview_only","instruction":"A preview never begins, retries or grants submission authority. Inspect changes and the existing attempt before acting."}),
        )
    }
}
