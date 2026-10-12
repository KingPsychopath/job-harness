//! Backfill ordering, per-account coverage and whole-thread review projections.
use super::*;
fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    crate::workspace::populate(root.path()).unwrap();
    root
}
fn supplied() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../examples/daily-review/observations.json"
    ))
    .unwrap()
}
fn ok(engine: &Engine, tool: &str, args: Value) -> Value {
    let result = engine.call(tool, &args);
    assert_eq!(result["ok"], true, "{tool}: {result}");
    result["data"].clone()
}
fn record(root: &Path, snapshot: &Value, key: &str) {
    let engine = Engine::open(root).unwrap();
    let run = ok(
        &engine,
        "start_run",
        json!({"operation":"daily_update","scope":{},"user_request":"Synthetic review only","idempotency_key":format!("{key}:run")}),
    );
    let path = format!("context/{key}.json");
    let bytes = storage::encoded(snapshot);
    fs::write(root.join(&path), &bytes).unwrap();
    ok(
        &engine,
        "daily_update",
        json!({"action":"record_review","run_id":run["run_id"],"mail_observations_path":path,"expected_source_sha256":storage::digest(&bytes),"idempotency_key":format!("{key}:review")}),
    );
}
fn date(snapshot: &mut Value, value: &str) {
    snapshot["observed_at"] = json!(value);
    snapshot["coverage"]["observed_at"] = json!(value);
}
#[test]
fn older_backfill_recorded_later_cannot_hide_a_request_from_a_newer_partial_scan() {
    let root = workspace();
    let mut complete = supplied();
    for row in complete["items"].as_array_mut().unwrap() {
        row["thread_id"] = json!("same-thread");
        row["thread_message_ids"] = json!(["confirmation-1", "recruiter-1"]);
        row["latest_message_id"] = json!("recruiter-1");
        row["request_message_ids"] = json!(["recruiter-1"]);
    }
    date(&mut complete, "2026-01-02T10:00:00Z");
    let mut partial = complete.clone();
    partial["items"].as_array_mut().unwrap().truncate(1);
    partial["items"][0]["thread_complete"] = json!(false);
    partial["coverage"]["pagination_exhausted"] = json!(false);
    date(&mut partial, "2026-01-02T11:00:00Z");
    record(root.path(), &partial, "newer");
    record(root.path(), &complete, "backfill");
    let result = ok(
        &Engine::open(root.path()).unwrap(),
        "daily_check",
        json!({}),
    );
    assert_eq!(result["counts"]["needs_reply"], 1);
    assert_eq!(result["communications"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["observations"]["observed_at"],
        "2026-01-02T11:00:00Z"
    );
    assert_eq!(result["observations"]["all_sources_complete"], false);
    assert!(
        result["archive_plan"]["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn coverage_is_scoped_per_mailbox_and_orders_real_instants_across_timezones() {
    let root = workspace();
    let mut later = supplied();
    later["mailbox_id"] = json!("later-mailbox");
    later["coverage"]["pagination_exhausted"] = json!(false);
    date(&mut later, "2026-01-02T07:30:00-01:00"); // 08:30 UTC
    let mut earlier = supplied();
    earlier["mailbox_id"] = json!("earlier-mailbox");
    date(&mut earlier, "2026-01-02T10:00:00+02:00"); // 08:00 UTC
    record(root.path(), &later, "later");
    record(root.path(), &earlier, "older-backfill");
    let engine = Engine::open(root.path()).unwrap();
    let result = ok(&engine, "daily_check", json!({}));
    assert_eq!(result["observations"]["mailbox_id"], "later-mailbox");
    assert_eq!(result["observations"]["source_count"], 2);
    assert_eq!(result["observations"]["all_sources_complete"], false);
    assert_eq!(result["communications"].as_array().unwrap().len(), 4);
    let filtered = ok(
        &engine,
        "daily_check",
        json!({"mailbox_id":"earlier-mailbox"}),
    );
    assert_eq!(filtered["communications"].as_array().unwrap().len(), 2);
}
#[test]
fn thread_archives_stay_together_on_normal_pages_and_support_exact_thread_filtering() {
    let root = workspace();
    let mut snapshot = supplied();
    let template = snapshot["items"][0].clone();
    let mut rows = Vec::new();
    for (id, thread, members) in [
        ("a1", "thread-one", json!(["a1", "a3"])),
        ("a2", "thread-two", json!(["a2", "a4"])),
        ("a3", "thread-one", json!(["a1", "a3"])),
        ("a4", "thread-two", json!(["a2", "a4"])),
    ] {
        let mut row = template.clone();
        row["message_id"] = json!(id);
        row["thread_id"] = json!(thread);
        row["thread_message_ids"] = members.clone();
        row["latest_message_id"] = members[1].clone();
        row["supports_message_archive"] = json!(false);
        rows.push(row);
    }
    snapshot["items"] = json!(rows);
    record(root.path(), &snapshot, "threads");
    let engine = Engine::open(root.path()).unwrap();
    let first = ok(&engine, "daily_check", json!({"limit":2}));
    assert_eq!(
        first["communications"][0]["thread_id"],
        first["communications"][1]["thread_id"]
    );
    assert_eq!(
        first["archive_plan"]["actions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(first["page"]["next_offset"], 2);
    let second = ok(&engine, "daily_check", json!({"limit":2,"offset":2}));
    assert_eq!(
        second["archive_plan"]["actions"].as_array().unwrap().len(),
        1
    );
    let thread = ok(&engine, "daily_check", json!({"thread_id":"thread-one"}));
    assert_eq!(thread["communications"].as_array().unwrap().len(), 2);
    assert_eq!(thread["counts"]["archive_candidates"], 1);
}
