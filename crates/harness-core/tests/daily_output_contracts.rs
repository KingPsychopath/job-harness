//! Public daily envelopes reject ambiguous producer output while preserving typed failures.
use job_harness_core::{Engine, contracts, workspace};
use serde_json::json;

#[test]
fn daily_output_contracts_reject_unknown_fields_and_wrong_types() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("fictional-workspace");
    workspace::initialize(&root).unwrap();
    let engine = Engine::open(&root).unwrap();
    let checked = engine.call("daily_check", &json!({}));
    assert_eq!(checked["ok"], true, "{checked}");
    let schema = &contracts::specs()["daily_check"]["outputSchema"];
    contracts::validate(schema, &checked).unwrap();
    for (field, value) in [
        ("undeclared_output", json!(true)),
        ("communications", json!({"message":"ambiguous container"})),
        ("counts", json!({"needs_reply":"one"})),
        ("archive_plan", json!("a plan must have typed fields")),
    ] {
        let mut bad = checked.clone();
        bad["data"][field] = value;
        assert!(contracts::validate(schema, &bad).is_err(), "{field}");
    }
    let mut false_claim = checked;
    false_claim["data"]["observations"]["live_checked"] = json!(true);
    assert!(contracts::validate(schema, &false_claim).is_err());
    for tool in ["daily_update", "begin_mail_archive"] {
        let failed = engine.call(tool, &json!({}));
        assert_eq!(failed["ok"], false);
        assert!(failed["data"].is_null());
        contracts::validate(&contracts::specs()[tool]["outputSchema"], &failed).unwrap();
    }
}
