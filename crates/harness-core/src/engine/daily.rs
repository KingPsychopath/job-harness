//! Durable mail review coordination. The host owns mailbox access and human confirmation.
use super::*;
use std::io::Read;

type MailThreads =
    BTreeMap<(String, String, String), (chrono::DateTime<chrono::FixedOffset>, Vec<Value>)>;

const STATES: &[&str] = &[
    "queued",
    "started",
    "archived",
    "failed",
    "outcome_unknown",
    "withdrawn",
];
fn invalid(message: &str) -> Error {
    Error::new("invalid_arguments", message)
}
fn digest(value: &Value) -> String {
    storage::digest(&storage::encoded(value))
}
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 2000 && !value.chars().any(char::is_control)
}
fn overlap(a: &Value, b: &Value) -> bool {
    a["provider"] == b["provider"]
        && a["mailbox_id"] == b["mailbox_id"]
        && (a["thread_id"] == b["thread_id"]
            || a["message_ids"].as_array().into_iter().flatten().any(|id| {
                b["message_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|other| other == id)
            }))
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn pending_mail_actions(state: &Value) -> Vec<Value> {
    mail_actions(state)
        .into_values()
        .filter(|row| {
            matches!(
                row["status"].as_str(),
                Some("queued" | "started" | "outcome_unknown")
            )
        })
        .collect()
}
fn mail_actions(state: &Value) -> BTreeMap<String, Value> {
    let mut rows = BTreeMap::new();
    for event in state["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["tool"] == "daily_update" || e["tool"] == "begin_mail_archive")
    {
        for row in event["result"]["archive_actions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(id) = row["action_id"].as_str() {
                rows.insert(id.into(), row.clone());
            }
        }
    }
    rows
}
pub fn validate_daily_events(state: &Value) -> Result<()> {
    let malformed = || {
        Error::new(
            "invalid_state",
            "Malformed durable mail lifecycle; preserve and repair its exact source",
        )
    };
    let mut plans: BTreeMap<String, Value> = BTreeMap::new();
    let mut rows: BTreeMap<String, Value> = BTreeMap::new();
    for event in state["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["tool"] == "daily_update" || e["tool"] == "begin_mail_archive")
    {
        let result = &event["result"];
        let operation = s(result, "action")?;
        if ![
            "record_review",
            "approve_archive",
            "withdraw_archive",
            "begin_archive",
            "record_archive_outcome",
            "reconcile_archive",
        ]
        .contains(&operation)
        {
            return Err(malformed());
        }
        let run_id = s(result, "run_id")?;
        if state["runs"].get(run_id).is_none() {
            return Err(malformed());
        }
        if operation == "record_review" {
            let plan = result.get("plan").ok_or_else(malformed)?;
            let id = s(plan, "plan_id")?;
            let expected = s(plan, "plan_sha256")?;
            let mut base = plan.clone();
            base.as_object_mut()
                .ok_or_else(malformed)?
                .remove("plan_sha256");
            if !sha(id) || !sha(expected) || digest(&base) != expected {
                return Err(malformed());
            }
            base.as_object_mut().unwrap().remove("plan_id");
            if digest(&base) != id {
                return Err(malformed());
            }
            let source_sha = s(&plan["source"], "sha256")?;
            if !sha(source_sha)
                || plan["frozen_source"]["sha256"] != source_sha
                || plan["frozen_source"]["path"]
                    != format!("applications/evidence/communications/{source_sha}.json")
            {
                return Err(malformed());
            }
            let actions = plan["actions"]
                .as_array()
                .filter(|actions| actions.len() <= 1000)
                .ok_or_else(malformed)?;
            let mut action_ids = BTreeSet::new();
            for action in actions {
                let id = s(action, "action_id")?;
                let mut base = action.clone();
                base.as_object_mut()
                    .ok_or_else(malformed)?
                    .remove("action_id");
                if !sha(id) || digest(&base) != id || !action_ids.insert(id) {
                    return Err(malformed());
                }
                for key in [
                    "provider",
                    "mailbox_id",
                    "thread_id",
                    "latest_message_id",
                    "inbox_label",
                ] {
                    if !identity(s(action, key)?) {
                        return Err(malformed());
                    }
                }
                let targets = string_array(&action["message_ids"])?;
                let members = string_array(&action["thread_message_ids"])?;
                if targets.is_empty()
                    || members.is_empty()
                    || members.len() > 1000
                    || targets.iter().any(|id| !members.contains(id))
                    || !members
                        .iter()
                        .any(|id| action["latest_message_id"] == id.as_str())
                    || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
                    || members.iter().collect::<BTreeSet<_>>().len() != members.len()
                {
                    return Err(malformed());
                }
                match action["target_scope"].as_str() {
                    Some("message") if targets.len() == 1 => {}
                    Some("thread") if targets == members => {}
                    _ => return Err(malformed()),
                }
                for id in members {
                    if !identity(&id) || !sha(s(&action["message_hashes"], &id)?) {
                        return Err(malformed());
                    }
                    let labels = string_array(&action["before_labels"][&id])?;
                    if labels.iter().any(|label| !identity(label))
                        || labels.iter().collect::<BTreeSet<_>>().len() != labels.len()
                    {
                        return Err(malformed());
                    }
                }
            }
            if plans.get(id).is_some_and(|prior| prior != plan) {
                return Err(malformed());
            }
            plans.insert(id.into(), plan.clone());
        } else if result.get("plan").is_some() {
            return Err(malformed());
        }
        let updates = result["archive_actions"]
            .as_array()
            .filter(|rows| rows.len() <= 1000)
            .ok_or_else(malformed)?;
        for row in updates {
            let id = s(row, "action_id")?;
            let status = s(row, "status")?;
            let plan = plans.get(s(row, "plan_id")?).ok_or_else(malformed)?;
            if row["plan_sha256"] != plan["plan_sha256"]
                || !STATES.contains(&status)
                || !plan["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action["action_id"] == id && action == &row["definition"])
            {
                return Err(malformed());
            }
            for key in ["approved_by", "restore_generation", "run_id"] {
                if !identity(s(row, key)?) {
                    return Err(malformed());
                }
            }
            if s(row, "approval_evidence")?.trim().is_empty() {
                return Err(malformed());
            }
            chrono::DateTime::parse_from_rfc3339(s(row, "approved_at")?)
                .map_err(|_| malformed())?;
            let prior = rows.get(id);
            match operation {
                "approve_archive" => {
                    if status != "queued"
                        || prior.is_some_and(|p| {
                            ["started", "outcome_unknown", "archived"]
                                .contains(&p["status"].as_str().unwrap_or(""))
                        })
                    {
                        return Err(malformed());
                    }
                }
                "withdraw_archive" => {
                    let prior = prior.ok_or_else(malformed)?;
                    if status != "withdrawn"
                        || prior["status"] != "queued"
                        || prior["plan_id"] != row["plan_id"]
                        || prior["definition"] != row["definition"]
                        || prior["approval_evidence"] != row["approval_evidence"]
                    {
                        return Err(malformed());
                    }
                    for key in ["withdrawn_by", "withdrawal_evidence", "withdrawal_reason"] {
                        if s(row, key)?.trim().is_empty() {
                            return Err(malformed());
                        }
                    }
                    if !identity(s(row, "withdrawn_by")?) {
                        return Err(malformed());
                    }
                    chrono::DateTime::parse_from_rfc3339(s(row, "withdrawn_at")?)
                        .map_err(|_| malformed())?;
                }
                "begin_archive" => {
                    if status != "started"
                        || prior.is_none_or(|p| {
                            p["status"] != "queued"
                                || p["plan_id"] != row["plan_id"]
                                || p["definition"] != row["definition"]
                                || p["approval_evidence"] != row["approval_evidence"]
                                || p["restore_generation"] != row["restore_generation"]
                        })
                    {
                        return Err(malformed());
                    }
                    chrono::DateTime::parse_from_rfc3339(s(row, "started_at")?)
                        .map_err(|_| malformed())?;
                    if !sha(s(&row["fresh_source"], "sha256")?) {
                        return Err(malformed());
                    }
                }
                "record_archive_outcome" | "reconcile_archive" => {
                    let prior = prior.ok_or_else(malformed)?;
                    if ["archived", "failed"].contains(&prior["status"].as_str().unwrap_or("")) {
                        if prior != row {
                            return Err(malformed());
                        }
                    } else if !["started", "outcome_unknown"]
                        .contains(&prior["status"].as_str().unwrap_or(""))
                        || !["archived", "failed", "outcome_unknown"].contains(&status)
                    {
                        return Err(malformed());
                    }
                    if digest(&row["result"]) != s(row, "result_sha256")?
                        || row["result"]["outcome"] != status
                        || row["result"]["action_id"] != id
                    {
                        return Err(malformed());
                    }
                    if status != "outcome_unknown" {
                        let differences = validate_readback(&row["definition"], &row["result"])?;
                        if row["label_differences"] != differences
                            || row["label_changes"] != has_label_drift(&differences)
                            || !sha(s(&row["readback"], "sha256")?)
                            || row["readback"]["sha256"] != row["result"]["readback_source_sha256"]
                            || row["readback"]["path"]
                                != format!(
                                    "applications/evidence/communications/readbacks/{}.json",
                                    s(&row["readback"], "sha256")?
                                )
                        {
                            return Err(malformed());
                        }
                    }
                }
                _ => return Err(malformed()),
            }
            rows.insert(id.into(), row.clone());
        }
    }
    Ok(())
}
fn has_label_drift(differences: &Value) -> bool {
    differences
        .as_object()
        .is_some_and(|rows| rows.values().any(|row| row["unexpected_changes"] == true))
}
fn validate_readback(definition: &Value, result: &Value) -> Result<Value> {
    let ids = string_array(&result["observed_message_ids"])?;
    let targets = string_array(&definition["message_ids"])?;
    if ids.len() != targets.len()
        || ids.iter().collect::<BTreeSet<_>>() != targets.iter().collect::<BTreeSet<_>>()
    {
        return Err(invalid(
            "Readback must name the exact unique target set; partial or missing targets remain unknown",
        ));
    }
    let labels = result["observed_labels"]
        .as_object()
        .ok_or_else(|| invalid("Readback needs exact per-message label sets"))?;
    if labels.len() != targets.len() || labels.keys().any(|id| !targets.contains(id)) {
        return Err(invalid(
            "Readback label keys must match the exact target set; deleted or unobservable messages remain unknown",
        ));
    }
    if string_array(&result["evidence"])?
        .iter()
        .all(|item| item.trim().is_empty())
    {
        return Err(invalid("Readback needs retained host evidence"));
    }
    let inbox = s(definition, "inbox_label")?;
    let archived = result["outcome"] == "archived";
    let mut differences = serde_json::Map::new();
    for id in targets {
        let before: BTreeSet<_> = string_array(&definition["before_labels"][&id])?
            .into_iter()
            .collect();
        let expected: BTreeSet<_> = before
            .iter()
            .filter(|label| !archived || label.as_str() != inbox)
            .cloned()
            .collect();
        let values = string_array(&result["observed_labels"][&id])?;
        let actual: BTreeSet<_> = values.iter().cloned().collect();
        if actual.len() != values.len() || values.iter().any(|label| !identity(label)) {
            return Err(invalid(
                "Observed labels must be unique nonempty bounded provider label IDs",
            ));
        }
        if actual.contains(inbox) == archived {
            return Err(Error::new(
                "adapter_mismatch",
                "The reported outcome disagrees with observed inbox presence; partial archive results remain unknown",
            ));
        }
        let drift = actual != expected;
        if drift && result["label_changes"] != true {
            return Err(Error::new(
                "adapter_mismatch",
                "Unrelated labels changed; explicitly attest label_changes:true with the verified readback to reconcile observed final state",
            ));
        }
        differences.insert(id,json!({"before":before,"observed":actual,"expected":expected,"added":actual.difference(&before).collect::<Vec<_>>(),"removed":before.difference(&actual).collect::<Vec<_>>(),"unexpected_changes":drift}));
    }
    Ok(Value::Object(differences))
}
fn public_action(action: &Value) -> Value {
    let keys = [
        "action_id",
        "provider",
        "mailbox_id",
        "target_scope",
        "thread_id",
        "message_ids",
        "category",
        "reason",
    ];
    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(value) = action.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    Value::Object(result)
}
pub(super) fn public_archive_row(row: &Value) -> Value {
    let mut value = public_action(&row["definition"]);
    for key in [
        "action_id",
        "plan_id",
        "plan_sha256",
        "status",
        "run_id",
        "approved_by",
        "approved_at",
        "approval_evidence",
        "started_at",
        "result_sha256",
        "observed_final_state_only",
        "label_changes",
        "label_differences",
        "readback",
        "withdrawn_by",
        "withdrawal_evidence",
        "withdrawal_reason",
        "withdrawn_at",
    ] {
        if let Some(item) = row.get(key) {
            value[key] = item.clone();
        }
    }
    if let Some(evidence) = row["result"].get("evidence") {
        value["evidence"] = evidence.clone();
    }
    value
}
fn normalize_snapshot(value: &mut Value) {
    for row in value["items"].as_array_mut().into_iter().flatten() {
        for key in [
            "thread_message_ids",
            "request_message_ids",
            "replied_to_message_ids",
            "labels",
        ] {
            if let Some(values) = row[key].as_array_mut() {
                values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            }
        }
    }
    if let Some(rows) = value["items"].as_array_mut() {
        rows.sort_by(|a, b| a["message_id"].as_str().cmp(&b["message_id"].as_str()));
    }
}
fn plans(state: &Value) -> Vec<Value> {
    state["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["tool"] == "daily_update" || e["tool"] == "begin_mail_archive")
        .filter_map(|e| e["result"].get("plan").cloned())
        .collect()
}

mod archive;
mod queries;
mod source;

impl Engine {
    pub(super) fn compact_daily_update(&self, mut value: Value) -> Value {
        if let Some(plan) = value.get_mut("plan") {
            let actions = plan["actions"].as_array().unwrap();
            let total = actions.len();
            let mut shown = Vec::new();
            for action in actions.iter().take(20) {
                let mut item = public_action(action);
                if item["message_ids"].as_array().unwrap().len() > 50 {
                    item["message_ids"] = json!([]);
                    item["requires_paged_review"] = json!(true);
                }
                shown.push(item);
            }
            plan["actions"] = json!(shown);
            plan["actions_page"] = json!({"total":total,"has_more":total>20});
            value["archive_plan"] = value["plan"].clone();
            value.as_object_mut().unwrap().remove("plan");
        }
        if let Some(actions) = value["archive_actions"].as_array_mut() {
            for row in actions {
                *row = public_archive_row(row);
            }
        }
        value
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod query_tests;
