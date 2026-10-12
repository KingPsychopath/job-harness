//! Read-only daily projections and retained recruiting communications.
use super::*;

struct MailScan {
    observed: chrono::DateTime<chrono::FixedOffset>,
    index: usize,
    snapshot: Value,
    path: String,
    plan: Value,
    origin: &'static str,
}

fn observed(value: &Value) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339(value.as_str().unwrap()).unwrap()
}

fn thread_key(row: &Value) -> (String, String, String) {
    (
        row["provider"].as_str().unwrap().into(),
        row["mailbox_id"].as_str().unwrap().into(),
        row["thread_id"].as_str().unwrap().into(),
    )
}

fn thread_page(rows: &[Value], offset: usize, limit: usize) -> (Vec<Value>, Value) {
    let start = offset.min(rows.len());
    let mut end = start;
    let mut partial = false;
    while end < rows.len() {
        let key = thread_key(&rows[end]);
        let mut group = end + 1;
        while group < rows.len() && thread_key(&rows[group]) == key {
            group += 1;
        }
        if group - start > limit {
            if end == start {
                end = (start + limit).min(rows.len());
                partial = true;
            }
            break;
        }
        end = group;
    }
    (
        rows[start..end].to_vec(),
        json!({"offset":offset,"limit":limit,"total":rows.len(),"next_offset":if end<rows.len(){Some(end)}else{None},"thread_boundary":!partial,"resume_thread_review":partial}),
    )
}

impl Engine {
    fn project_mail_row(&self, snapshot: &Value, row: &Value, path: &str) -> Value {
        let mut item = row.clone();
        for key in ["provider", "mailbox_id", "observed_at"] {
            item[key] = snapshot[key].clone();
        }
        item["source_path"] = json!(path);
        let supplied = row["record_path"].as_str();
        let attribution = crate::workflow::mail(
            &self.root,
            &json!({"provider":snapshot["provider"],"thread_id":row["thread_id"],"message_ids":row["thread_message_ids"],"request_message_ids":row["request_message_ids"],"latest_message_id":row["latest_message_id"],"vacancy_url":row["vacancy_url"],"posting_id":row["posting_id"],"company":row["company"],"candidate_records":supplied.into_iter().collect::<Vec<_>>(),"requires_reply":row["needs_reply"],"replied_to_message_ids":row["replied_to_message_ids"],"observed_at":snapshot["observed_at"]}),
        );
        match attribution {
            Ok(check) => {
                item["attribution"] = check["attribution"].clone();
                if check["attribution"] != "proposed_exact_identity" {
                    item["supplied_record_path"] = row["record_path"].clone();
                    item.as_object_mut().unwrap().remove("record_path");
                }
            }
            Err(error) => {
                item["attribution"] = json!("invalid_supplied_identity");
                item["attribution_gap"] = json!(error.to_string());
                item["supplied_record_path"] = row["record_path"].clone();
                item.as_object_mut().unwrap().remove("record_path");
            }
        }
        item.as_object_mut().unwrap().remove("text");
        for key in ["subject", "summary", "sender", "attribution_gap"] {
            if let Some(text) = item[key].as_str() {
                let max = if key == "subject" { 512 } else { 1000 };
                if text.chars().count() > max {
                    item[key] = json!(text.chars().take(max).collect::<String>());
                    item["metadata_truncated"] = json!(true);
                }
            }
        }
        for key in [
            "thread_message_ids",
            "request_message_ids",
            "replied_to_message_ids",
            "labels",
        ] {
            if item[key].as_array().is_some_and(|ids| ids.len() > 100) {
                item[key] = json!(
                    item[key]
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(100)
                        .collect::<Vec<_>>()
                );
                item["metadata_truncated"] = json!(true);
            }
        }
        item
    }
    pub(in crate::engine) fn daily_check(&self, args: &Value, state: &Value) -> Result<Value> {
        if args.get("mail_observations").is_some() && args.get("mail_observations_path").is_some() {
            return Err(invalid("Supply inline observations or a path, never both"));
        }
        let as_of = if let Some(date) = args["as_of"].as_str() {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| invalid("as_of must be YYYY-MM-DD"))?
        } else {
            chrono::NaiveDate::parse_from_str(&self.today()?, "%Y-%m-%d").unwrap()
        };
        let recorded = plans(state);
        let mut latest = None;
        let mut scans = Vec::new();
        let mut observations = json!({"origin":"none","source_state":"none","live_checked":false});
        // A bounded later scan replaces only threads it actually observed. Omission never resolves a request.
        let mut threads: MailThreads = BTreeMap::new();
        let mut gaps = Vec::new();
        for (index, plan) in recorded.iter().enumerate() {
            let path = s(&plan["frozen_source"], "path")?;
            match self.daily_source(path,Some(s(&plan["frozen_source"],"sha256")?)){
                Ok((snapshot,_,_))=>scans.push(MailScan {observed:observed(&snapshot["observed_at"]),index,snapshot,path:path.into(),plan:plan.clone(),origin:"saved"}),
                Err(error)=>gaps.push(json!({"path":path,"sha256":plan["frozen_source"]["sha256"],"error":error.to_string()}))
            }
        }
        if args.get("mail_observations").is_some() || args.get("mail_observations_path").is_some() {
            let (snapshot, hash, path) = if let Some(path) = args["mail_observations_path"].as_str()
            {
                let (snapshot, hash, _) =
                    self.daily_source(path, args["expected_source_sha256"].as_str())?;
                (snapshot, hash, path.to_owned())
            } else {
                let mut snapshot = args["mail_observations"].clone();
                let bytes = storage::encoded(&snapshot);
                if bytes.len() > 1_000_000 {
                    return Err(invalid("Inline mail observations exceed one MB"));
                }
                normalize_snapshot(&mut snapshot);
                self.validate_daily_snapshot(&snapshot)?;
                let hash = storage::digest(&bytes);
                if args["expected_source_sha256"]
                    .as_str()
                    .is_some_and(|expected| expected != hash)
                {
                    return Err(Error::new(
                        "artifact_changed",
                        "Inline observation digest differs",
                    ));
                }
                (snapshot, hash, "inline_observations".into())
            };
            let plan = self.daily_plan(&snapshot, &path, &hash)?;
            scans.push(MailScan {
                observed: observed(&snapshot["observed_at"]),
                index: recorded.len(),
                snapshot,
                path,
                plan,
                origin: "supplied",
            });
        }
        // Backfills must enter before newer partial scans, independent of recording order.
        scans.sort_by(|a, b| a.observed.cmp(&b.observed).then(a.index.cmp(&b.index)));
        let mut source_summaries = BTreeMap::new();
        for scan in &scans {
            self.merge_daily_threads(&mut threads, &scan.snapshot, &scan.path)?;
            let plan = &scan.plan;
            if ["provider", "mailbox_id"]
                .iter()
                .any(|key| args[*key].as_str().is_some_and(|value| plan[*key] != value))
            {
                continue;
            }
            let source_state = if plan["source"]["path"] == "inline_observations" {
                "supplied_inline"
            } else if self
                .daily_source(
                    s(&plan["source"], "path")?,
                    Some(s(&plan["source"], "sha256")?),
                )
                .is_ok()
            {
                "unchanged"
            } else {
                "changed_or_missing"
            };
            let summary = json!({"provider":plan["provider"],"mailbox_id":plan["mailbox_id"],"observed_at":plan["observed_at"],"coverage":plan["coverage"],"origin":scan.origin,"path":plan["source"]["path"],"sha256":plan["source"]["sha256"],"source_state":source_state,"live_checked":false});
            source_summaries.insert(
                (
                    s(plan, "provider")?.to_owned(),
                    s(plan, "mailbox_id")?.to_owned(),
                ),
                summary.clone(),
            );
            if args["thread_id"].as_str().is_some_and(|id| {
                !scan.snapshot["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["thread_id"] == id)
            }) {
                continue;
            }
            for key in [
                "provider",
                "mailbox_id",
                "observed_at",
                "coverage",
                "origin",
                "path",
                "sha256",
                "source_state",
            ] {
                observations[key] = summary[key].clone();
            }
            latest = Some(plan.clone());
        }
        observations["all_sources_complete"] = json!(
            !source_summaries.is_empty()
                && gaps.is_empty()
                && source_summaries
                    .values()
                    .all(|source| source["coverage"]["complete"] == true)
        );
        observations["source_count"] = json!(source_summaries.len());
        observations["sources_truncated"] = json!(source_summaries.len() > 50);
        observations["sources"] =
            json!(source_summaries.into_values().take(50).collect::<Vec<_>>());
        observations["retained_evidence_complete"] = json!(gaps.is_empty());
        observations["retained_evidence_gaps"] = json!(gaps.iter().take(100).collect::<Vec<_>>());
        observations["retained_evidence_gap_count"] = json!(gaps.len());
        if !gaps.is_empty() {
            observations["coverage"]["complete"] = json!(false);
        }
        let mut rows: Vec<_> = threads
            .values()
            .flat_map(|(time, rows)| {
                rows.iter()
                    .cloned()
                    .map(|mut row| {
                        row["thread_observed_at"] = json!(time.to_rfc3339());
                        row
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|row| {
                ["provider", "mailbox_id", "thread_id"]
                    .iter()
                    .all(|key| args[*key].as_str().is_none_or(|filter| row[*key] == filter))
                    && args["record"]
                        .as_str()
                        .is_none_or(|r| row["record_path"] == r)
                    && args["company"].as_str().is_none_or(|c| {
                        row["company"]
                            .as_str()
                            .is_some_and(|v| domain::company_key(v) == domain::company_key(c))
                    })
            })
            .collect();
        rows.sort_by(|a, b| {
            observed(&a["thread_observed_at"])
                .cmp(&observed(&b["thread_observed_at"]))
                .reverse()
                .then(thread_key(a).cmp(&thread_key(b)))
                .then(a["message_id"].as_str().cmp(&b["message_id"].as_str()))
        });
        if let Some(plan) = latest.as_mut() {
            let original = plan["actions"].as_array().unwrap();
            let eligible: Vec<_> = original
                .iter()
                .filter(|action| {
                    let key = thread_key(action);
                    threads.get(&key).is_some_and(|(time, members)| {
                        *time <= observed(&plan["observed_at"])
                            && members.iter().all(|member| {
                                member["needs_reply"] == false
                                    && member["category"] != "unknown"
                                    && member["partial_newer_observation"] != true
                            })
                    }) && action["message_ids"].as_array().unwrap().iter().all(|id| {
                        rows.iter()
                            .any(|row| row["message_id"] == *id && thread_key(row) == key)
                    })
                })
                .cloned()
                .collect();
            plan["actions"] = json!(eligible);
        }
        let mut counts = json!({"rejections":0,"application_confirmations":0,"reply_requests":0,"information":0,"correspondence":0,"unknown":0,"archive_candidates":latest.as_ref().map_or(0,|p|p["actions"].as_array().unwrap().len()),"queued":0,"started":0,"archived":0,"failed":0,"withdrawn":0,"outcome_unknown":0,"interviews":0,"upcoming_interviews":0,"past_interviews":0,"unscheduled_interviews":0,"needs_reply":0});
        for row in &rows {
            if row["needs_reply"] == true {
                counts["needs_reply"] = json!(counts["needs_reply"].as_u64().unwrap() + 1);
            }
            let key = match row["category"].as_str().unwrap() {
                "rejection" => "rejections",
                "application_confirmation" => "application_confirmations",
                "reply_request" => "reply_requests",
                other => other,
            };
            counts[key] = json!(counts[key].as_u64().unwrap_or(0) + 1);
        }
        let action_states = mail_actions(state);
        let actions: Vec<_> = action_states.values().cloned().collect();
        for row in &actions {
            let key = s(row, "status")?;
            counts[key] = json!(counts[key].as_u64().unwrap_or(0) + 1);
        }
        let mut attention = self.review_queue(&json!({"limit":usize::MAX}), state)?;
        let mut items = attention["items"].as_array().cloned().unwrap_or_default();
        for item in &mut items {
            if item["kind"] == "interview" {
                let kind = match item["scheduled_start"]
                    .as_str()
                    .and_then(|date| date.get(..10))
                    .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                {
                    Some(date) if date < as_of => "past",
                    Some(_) => "upcoming",
                    None => "unscheduled",
                };
                item["temporal_state"] = json!(kind);
                counts["interviews"] = json!(counts["interviews"].as_u64().unwrap() + 1);
                let key = format!("{kind}_interviews");
                counts[&key] = json!(counts[&key].as_u64().unwrap() + 1);
            }
        }
        for row in &rows {
            if row["needs_reply"] != false || row["category"] == "unknown" {
                items.push(json!({"kind":"supplied_mail","message_id":row["message_id"],"thread_id":row["thread_id"],"company":row["company"],"thread_url":row["thread_url"],"needs_reply":row["needs_reply"],"source_path":row["source_path"],"source_sha256":row["source_sha256"],"next_action":"Read the latest complete communication before replying"}));
            }
        }
        for gap in &gaps {
            items.push(json!({"kind":"mail_evidence_gap","source_path":gap["path"],"reason":"Retained mail evidence is unreadable or changed; missing scans never imply resolved requests"}));
        }
        let offset = n(args, "offset", 0);
        let limit = n(args, "limit", 20);
        let (items, attention_page) = domain::page(&items, offset, limit);
        counts["review_items"] = attention_page["total"].clone();
        attention["items"] = json!(items);
        attention["page"] = attention_page;
        let (rows, page) = thread_page(&rows, offset, limit);
        let (action_page, archive_actions_page) = domain::page(&actions, offset, limit);
        let action_page: Vec<_> = action_page.iter().map(public_archive_row).collect();
        if let Some(plan) = latest.as_mut() {
            let total = plan["actions"].as_array().unwrap().len();
            let action_rows: Vec<_> = plan["actions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|action| {
                    action["message_ids"].as_array().unwrap().iter().all(|id| {
                        rows.iter().any(|row| {
                            row["message_id"] == *id
                                && row["provider"] == action["provider"]
                                && row["mailbox_id"] == action["mailbox_id"]
                        })
                    })
                })
                .map(|definition| {
                    let mut action = public_action(definition);
                    action["archive_state"] = action_states.get(definition["action_id"].as_str().unwrap())
                        .map(|row| json!({"action_id":row["action_id"],"plan_id":row["plan_id"],"plan_sha256":row["plan_sha256"],"status":row["status"]}))
                        .unwrap_or(Value::Null);
                    action
                })
                .collect();
            plan["actions"] = json!(action_rows);
            plan["actions_page"] = json!({"total":total,"displayed":plan["actions"].as_array().unwrap().len(),"selection_scope":"only_targets_fully_displayed_in_communications_page","next_offset":page["next_offset"]});
        }
        Ok(
            json!({"observations":observations,"communications":rows,"counts":counts,"archive_plan":latest,"archive_actions":action_page,"archive_actions_page":archive_actions_page,"attention":attention,"as_of":as_of.to_string(),"page":page,"external_actions":false,"applications":false,"canonical_changes":false}),
        )
    }
    fn merge_daily_threads(
        &self,
        threads: &mut MailThreads,
        snapshot: &Value,
        path: &str,
    ) -> Result<()> {
        let observed = chrono::DateTime::parse_from_rfc3339(s(snapshot, "observed_at")?).unwrap();
        let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for row in snapshot["items"].as_array().unwrap() {
            grouped
                .entry(s(row, "thread_id")?.into())
                .or_default()
                .push(self.project_mail_row(snapshot, row, path));
        }
        for (thread, mut rows) in grouped {
            let key = (
                s(snapshot, "provider")?.into(),
                s(snapshot, "mailbox_id")?.into(),
                thread,
            );
            if threads
                .get(&key)
                .is_none_or(|(prior, _)| observed >= *prior)
            {
                if rows
                    .iter()
                    .any(|row| row["thread_complete"] != true || row["full_read"] != true)
                    && let Some((_, previous)) = threads.get(&key)
                {
                    for old in previous {
                        if let Some(index) = rows
                            .iter()
                            .position(|row| row["message_id"] == old["message_id"])
                        {
                            if old["needs_reply"] != false && rows[index]["needs_reply"] == false {
                                let mut retained = old.clone();
                                retained["partial_newer_observation"] = json!(true);
                                rows[index] = retained;
                            }
                        } else {
                            let mut retained = old.clone();
                            retained["partial_newer_observation"] = json!(true);
                            rows.push(retained);
                        }
                    }
                }
                threads.insert(key, (observed, rows));
            }
        }
        Ok(())
    }
}
