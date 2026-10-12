//! Digest-checked observations and immutable archive plans.
use super::*;

impl Engine {
    pub(super) fn daily_source(
        &self,
        path: &str,
        expected: Option<&str>,
    ) -> Result<(Value, String, Vec<u8>)> {
        let path = self.safe_path(path, &["applications", "context"], &["json"])?;
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(1_000_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1_000_000 {
            return Err(invalid(
                "Supply at most one MB of sanitized mail observations",
            ));
        }
        let actual = storage::digest(&bytes);
        if expected.is_some_and(|value| !sha(value) || value != actual) {
            return Err(Error::new(
                "artifact_changed",
                "Mail observation source changed; review the current exact bytes",
            ));
        }
        let mut snapshot: Value = serde_json::from_slice(&bytes)?;
        normalize_snapshot(&mut snapshot);
        self.validate_daily_snapshot(&snapshot)?;
        Ok((snapshot, actual, bytes))
    }
    pub(super) fn validate_daily_snapshot(&self, value: &Value) -> Result<()> {
        contracts::validate(
            &contracts::specs()["daily_check"]["inputSchema"]["properties"]["mail_observations"],
            value,
        )?;
        if value["schema_version"] != 1 {
            return Err(invalid("Use mail observation schema_version 1"));
        }
        for key in ["provider", "mailbox_id", "observed_at"] {
            if !identity(s(value, key)?) {
                return Err(invalid(
                    "Mail source identities must be nonempty bounded text without controls",
                ));
            }
        }
        let observed = chrono::DateTime::parse_from_rfc3339(s(value, "observed_at")?)
            .map_err(|_| invalid("Mail observed_at needs an explicit RFC3339 timezone"))?;
        let covered =
            chrono::DateTime::parse_from_rfc3339(s(&value["coverage"], "observed_at")?)
                .map_err(|_| invalid("Coverage needs an explicit RFC3339 observation time"))?;
        if observed != covered {
            return Err(invalid(
                "Coverage and snapshot observation times must agree",
            ));
        }
        crate::workflow::coverage(&value["coverage"])?;
        let rows = value["items"]
            .as_array()
            .filter(|rows| rows.len() <= 1000)
            .ok_or_else(|| invalid("Supply at most 1000 mail items"))?;
        let mut identities = BTreeSet::new();
        for row in rows {
            let id = s(row, "message_id")?;
            let thread = s(row, "thread_id")?;
            for key in [
                "message_id",
                "thread_id",
                "latest_message_id",
                "inbox_label",
            ] {
                if !identity(s(row, key)?) {
                    return Err(invalid(
                        "Mail IDs and inbox label must be nonempty bounded text without controls",
                    ));
                }
            }
            if !identities.insert(id) {
                return Err(invalid(
                    "Mail message IDs must be unique within a mailbox snapshot",
                ));
            }
            if !sha(s(row, "source_sha256")?) {
                return Err(invalid("Mail items need exact source text digests"));
            }
            let text = s(row, "text")?;
            if text.trim().is_empty() {
                return Err(invalid(
                    "Retain bounded sanitized source text for every communication",
                ));
            }
            if storage::digest(text.as_bytes()) != row["source_sha256"] {
                return Err(Error::new(
                    "artifact_changed",
                    "Supplied message text does not match its digest",
                ));
            }
            if ![
                "rejection",
                "application_confirmation",
                "reply_request",
                "information",
                "correspondence",
                "unknown",
            ]
            .contains(&s(row, "category")?)
            {
                return Err(invalid("Unknown communication category"));
            }
            let ids = string_array(&row["thread_message_ids"])?;
            let latest = s(row, "latest_message_id")?;
            if ids.iter().any(|id| !identity(id))
                || ids.is_empty()
                || ids.len() > 1000
                || !ids.iter().any(|x| x == id)
                || !ids.iter().any(|x| x == latest)
                || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            {
                return Err(invalid(
                    "Exact thread membership and latest ID are required",
                ));
            }
            for key in ["request_message_ids", "replied_to_message_ids", "labels"] {
                let values = string_array(&row[key])?;
                if values.iter().any(|id| !identity(id))
                    || values.len() > 1000
                    || values.iter().collect::<BTreeSet<_>>().len() != values.len()
                    || (key != "labels" && values.iter().any(|x| !ids.contains(x)))
                {
                    return Err(invalid(
                        "Request/reply IDs and label sets must be unique observed values",
                    ));
                }
            }
            for key in ["full_read", "thread_complete", "supports_message_archive"] {
                if !row[key].is_boolean() {
                    return Err(invalid(
                        "Mail completeness and stable-label adapter capabilities must be explicit",
                    ));
                }
            }
            if !row["needs_reply"].is_null() && !row["needs_reply"].is_boolean() {
                return Err(invalid("needs_reply is boolean or unknown"));
            }
            s(row, "inbox_label")?;
            for other in rows.iter().filter(|other| other["thread_id"] == thread) {
                for key in [
                    "thread_message_ids",
                    "latest_message_id",
                    "request_message_ids",
                    "replied_to_message_ids",
                    "supports_message_archive",
                    "inbox_label",
                ] {
                    if other[key] != row[key] {
                        return Err(invalid("Thread observations contradict one another"));
                    }
                }
            }
            if row["thread_complete"] == true
                && ids.iter().any(|id| {
                    !rows.iter().any(|other| {
                        other["thread_id"] == thread && other["message_id"] == id.as_str()
                    })
                })
            {
                return Err(invalid("Complete thread evidence must retain every member"));
            }
        }
        Ok(())
    }
    pub(super) fn daily_plan(
        &self,
        snapshot: &Value,
        path: &str,
        source_sha: &str,
    ) -> Result<Value> {
        let rows = snapshot["items"].as_array().unwrap();
        let mut actions = Vec::new();
        let mut seen = BTreeSet::new();
        let mut coverage = crate::workflow::coverage(&snapshot["coverage"])?;
        if chrono::DateTime::parse_from_rfc3339(s(snapshot, "observed_at")?)
            .unwrap()
            .with_timezone(&chrono::Utc)
            > chrono::Utc::now()
        {
            coverage["complete"] = json!(false);
            coverage["missing"]
                .as_array_mut()
                .unwrap()
                .push(json!("Observation timestamp is in the future"));
        }
        for row in rows {
            let members: Vec<_> = rows
                .iter()
                .filter(|other| other["thread_id"] == row["thread_id"])
                .collect();
            let requests = string_array(&row["request_message_ids"])?;
            let replied = string_array(&row["replied_to_message_ids"])?;
            if members.iter().any(|m| {
                m["full_read"] != true
                    || m["thread_complete"] != true
                    || m["needs_reply"] != false
                    || m["category"] == "unknown"
            }) || requests.iter().any(|id| !replied.contains(id))
                || coverage["complete"] != true
            {
                continue;
            }
            if !["rejection", "application_confirmation"]
                .contains(&row["category"].as_str().unwrap())
                && row["archive_candidate"] != true
            {
                continue;
            }
            let message_scope = row["supports_message_archive"] == true;
            if !message_scope
                && members.iter().any(|m| {
                    !["rejection", "application_confirmation"]
                        .contains(&m["category"].as_str().unwrap())
                        && m["archive_candidate"] != true
                })
            {
                continue;
            }
            let key = if message_scope {
                s(row, "message_id")?
            } else {
                s(row, "thread_id")?
            };
            if !seen.insert((message_scope, key.to_owned())) {
                continue;
            }
            let targets = if message_scope {
                vec![row]
            } else {
                members.clone()
            };
            if targets
                .iter()
                .all(|m| !m["labels"].as_array().unwrap().contains(&m["inbox_label"]))
            {
                continue;
            }
            let ids: Vec<_> = targets.iter().map(|m| m["message_id"].clone()).collect();
            let mut labels = serde_json::Map::new();
            let mut hashes = serde_json::Map::new();
            for m in &members {
                labels.insert(s(m, "message_id")?.into(), m["labels"].clone());
                hashes.insert(s(m, "message_id")?.into(), m["source_sha256"].clone());
            }
            let mut action = json!({"provider":snapshot["provider"],"mailbox_id":snapshot["mailbox_id"],"target_scope":if message_scope{"message"}else{"thread"},"thread_id":row["thread_id"],"thread_message_ids":row["thread_message_ids"],"latest_message_id":row["latest_message_id"],"request_message_ids":row["request_message_ids"],"replied_to_message_ids":row["replied_to_message_ids"],"message_ids":ids,"message_hashes":hashes,"before_labels":labels,"inbox_label":row["inbox_label"],"category":row["category"],"reason":"Fully observed resolved communication; exact human confirmation and a fresh host read are required"});
            action["action_id"] = json!(digest(&action));
            actions.push(action);
        }
        let mut plan = json!({"provider":snapshot["provider"],"mailbox_id":snapshot["mailbox_id"],"observed_at":snapshot["observed_at"],"coverage":coverage,"source":{"path":path,"sha256":source_sha},"frozen_source":{"path":format!("applications/evidence/communications/{source_sha}.json"),"sha256":source_sha},"actions":actions});
        plan["plan_id"] = json!(digest(&plan));
        plan["plan_sha256"] = json!(digest(&plan));
        if storage::encoded(&plan).len() > 1_000_000 {
            return Err(invalid(
                "Mail plan exceeds one MB; provide smaller observed thread groups",
            ));
        }
        Ok(plan)
    }
    pub(super) fn preserve_mail_readback(&self, prior: &Value, result: &Value) -> Result<Value> {
        let original = s(result, "readback_source_path")?;
        let expected = s(result, "readback_source_sha256")?;
        let path = self.safe_path(original, &["applications", "context"], &["json"])?;
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(1_000_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1_000_000 {
            return Err(invalid("Mail readback proof exceeds one MB"));
        }
        let actual = storage::digest(&bytes);
        if !sha(expected) || expected != actual {
            return Err(Error::new(
                "artifact_changed",
                "Mail readback proof changed; use its current exact bytes",
            ));
        }
        let proof: Value = serde_json::from_slice(&bytes)?;
        if proof["schema_version"] != 1 {
            return Err(invalid("Readback proof requires schema_version 1"));
        }
        for key in ["action_id", "provider", "mailbox_id", "thread_id"] {
            if proof[key] != prior["definition"][key] {
                return Err(invalid(
                    "Readback proof does not match this exact provider/mailbox/thread/action",
                ));
            }
        }
        let supplied_ids = string_array(&proof["observed_message_ids"])?;
        let result_ids = string_array(&result["observed_message_ids"])?;
        if supplied_ids.len() != result_ids.len()
            || supplied_ids.iter().collect::<BTreeSet<_>>()
                != result_ids.iter().collect::<BTreeSet<_>>()
        {
            return Err(invalid(
                "Readback source does not match the exact unique reported IDs",
            ));
        }
        let proof_labels = proof["observed_labels"]
            .as_object()
            .ok_or_else(|| invalid("Readback proof needs per-message labels"))?;
        let reported_labels = result["observed_labels"]
            .as_object()
            .ok_or_else(|| invalid("Reported readback needs per-message labels"))?;
        if proof_labels.len() != reported_labels.len()
            || proof_labels
                .keys()
                .any(|id| !reported_labels.contains_key(id))
        {
            return Err(invalid(
                "Readback source label keys differ from reported targets",
            ));
        }
        for id in proof_labels.keys() {
            let supplied = string_array(&proof_labels[id])?;
            let reported = string_array(&reported_labels[id])?;
            if supplied.len() != reported.len()
                || supplied.iter().collect::<BTreeSet<_>>()
                    != reported.iter().collect::<BTreeSet<_>>()
            {
                return Err(invalid(
                    "Readback source does not match reported label sets",
                ));
            }
        }
        let observed = chrono::DateTime::parse_from_rfc3339(s(&proof, "observed_at")?)
            .map_err(|_| invalid("Readback proof needs an RFC3339 observation timestamp"))?
            .with_timezone(&chrono::Utc);
        let began = chrono::DateTime::parse_from_rfc3339(s(prior, "started_at")?)
            .map_err(|_| Error::new("invalid_state", "Malformed archive begin timestamp"))?
            .with_timezone(&chrono::Utc);
        if observed < began || observed > chrono::Utc::now() {
            return Err(Error::new(
                "stale_plan",
                "Readback must be observed after the durable begin and never in the future",
            ));
        }
        let retained = format!("applications/evidence/communications/readbacks/{actual}.json");
        let target = self.safe_path(&retained, &["applications"], &["json"])?;
        if target.exists() && fs::read(&target)? != bytes {
            return Err(Error::new(
                "artifact_changed",
                "Retained readback proof differs from its digest",
            ));
        }
        if !target.exists() {
            storage::atomic_write(&target, &bytes)?;
        }
        Ok(
            json!({"path":retained,"sha256":actual,"original_path":original,"observed_at":proof["observed_at"]}),
        )
    }
    pub(super) fn restore_generation(&self) -> Result<String> {
        let path = self.safe_path(
            crate::bundle::MAIL_RESTORE_PATH,
            &["applications"],
            &["json"],
        )?;
        if !path.exists() {
            return Ok("original".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(Error::new(
                "invalid_state",
                "Mail restore marker is oversized",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        if value["schema_version"] != 1
            || value["generation"]
                .as_str()
                .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(Error::new(
                "invalid_state",
                "Mail restore marker is invalid",
            ));
        }
        Ok(s(&value, "generation")?.into())
    }
    pub(super) fn preserved_plan(&self, state: &Value, args: &Value) -> Result<Value> {
        let plan = plans(state)
            .into_iter()
            .find(|p| p["plan_id"] == args["plan_id"])
            .ok_or_else(|| {
                Error::new(
                    "missing_resource",
                    "Review and record this exact mail plan first",
                )
            })?;
        if args["expected_plan_sha256"] != plan["plan_sha256"] {
            return Err(Error::new(
                "stale_plan",
                "Mail plan hash changed; review the exact plan",
            ));
        }
        if args["action"] != "withdraw_archive" {
            self.daily_source(
                s(&plan["frozen_source"], "path")?,
                Some(s(&plan["frozen_source"], "sha256")?),
            )?;
        }
        Ok(plan)
    }
}
