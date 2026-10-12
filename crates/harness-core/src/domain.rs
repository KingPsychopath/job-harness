//! Host-local clock and wire projections; business rules live in harness-domain.
use crate::Result;
pub use job_harness_domain::identity::{casefold, company_key, normal_url};
pub use job_harness_domain::time::iso_datetime;
pub use job_harness_domain::tracking::{SCHEDULES, STAGES, TRACKING_KEYS};
pub use job_harness_domain::{SubmissionLifecycle, SubmissionStatus as SubmissionState};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    Ok(crate::storage::encoded(value))
}
pub fn page(rows: &[Value], offset: usize, limit: usize) -> (Vec<Value>, Value) {
    let end = offset.saturating_add(limit);
    let selected = rows.iter().skip(offset).take(limit).cloned().collect();
    (
        selected,
        json!({"total":rows.len(),"offset":offset,"limit":limit,"next_offset":if end<rows.len(){Some(end)}else{None}}),
    )
}
pub fn record(row: &Value) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "record_path".into(),
        row.get("path").cloned().unwrap_or_else(|| {
            json!(format!(
                "applications/{}",
                row["record"].as_str().unwrap_or_default()
            ))
        }),
    );
    for key in [
        "title",
        "stage",
        "checked",
        "next_action",
        "company",
        "posting_id",
        "vacancy_url",
        "source",
        "status_detail",
        "tracking_source",
    ] {
        if let Some(v) = row.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    Value::Object(out)
}
pub fn attempt(row: &Value) -> Value {
    let mut out = json!({"attempt_id":row["id"],"record_path":row["record"]});
    for key in [
        "run_id",
        "packet_id",
        "company",
        "vacancy_url",
        "status",
        "summary",
        "reserved_at",
        "began_at",
        "resolved_at",
    ] {
        if let Some(v) = row.get(key) {
            out[key] = v.clone();
        }
    }
    out
}
pub fn run(row: &Value) -> Value {
    let mut out = json!({"run_id":row["id"]});
    for key in [
        "operation",
        "scope",
        "status",
        "started_at",
        "finished_at",
        "summary",
    ] {
        if let Some(v) = row.get(key) {
            out[key] = v.clone();
        }
    }
    if row["operation"] == "apply"
        && row["scope"].get("max_submissions").is_some()
        && out["scope"].get("max_attempts").is_none()
    {
        out["scope"]["max_attempts"] =
            json!(row["scope"]["max_submissions"].as_u64().unwrap_or(0) * 3);
    }
    out
}
pub fn packet(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(v) = out.as_object_mut().and_then(|m| m.remove("record")) {
        out["record_path"] = v;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_offset_is_monotonic() {
        let rows = vec![json!(1), json!(2), json!(3)];
        let (r, p) = page(&rows, 0, 2);
        assert_eq!(r.len(), 2);
        assert_eq!(p["next_offset"], 2);
        assert_eq!(page(&rows, 2, 2).1["next_offset"], Value::Null);
    }
}
