use super::*;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for path in [
        "applications/evidence",
        "context/profile",
        "config",
        "output",
    ] {
        fs::create_dir_all(dir.path().join(path)).unwrap();
    }
    fs::write(
        dir.path().join("config/career-ledger.json"),
        r#"{"schema_version":1,"timezone":"UTC","primary_cv":"output/cv.pdf"}"#,
    )
    .unwrap();
    fs::write(dir.path().join("output/cv.pdf"), "%PDF-1.7\nSynthetic\n").unwrap();
    fs::write(
        dir.path().join("applications/approved-answers.md"),
        "# Fictional answers\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("context/profile/application-decisions.md"),
        "# Fictional decisions\n",
    )
    .unwrap();
    fs::write(dir.path().join("applications/fictional-role.md"), "# Fictional — Engineer\n\n<!-- tracking\n{\"company\":\"Fictional\",\"stage\":\"saved\",\"next_action\":\"Review\",\"checked\":\"2026-10-12\",\"vacancy_url\":\"https://jobs.example.test/role/123\",\"posting_id\":\"123\"}\n-->\n").unwrap();
    dir
}
fn call(engine: &Engine, name: &str, args: Value) -> Value {
    let value = engine.call(name, &args);
    assert_eq!(value["ok"], true, "{name}: {value}");
    contracts::validate(&contracts::specs()[name]["outputSchema"], &value).unwrap();
    value["data"].clone()
}
fn row(id: &str, thread: &str, category: &str, reply: bool) -> Value {
    let text = format!("Synthetic {category} communication for {id}. No candidate facts.");
    json!({"message_id":id,"thread_id":thread,"thread_message_ids":[id],"latest_message_id":id,"source_sha256":storage::digest(text.as_bytes()),"text":text,"category":category,"needs_reply":reply,"request_message_ids":if reply{vec![id]}else{vec![]},"replied_to_message_ids":[],"full_read":true,"thread_complete":true,"labels":["INBOX","READ"],"inbox_label":"INBOX","supports_message_archive":true,"subject":"Synthetic communication","company":"Fictional"})
}
fn snapshot(rows: Vec<Value>) -> Value {
    let now = domain::now();
    json!({"schema_version":1,"provider":"fictional-label-provider","mailbox_id":"fictional-mailbox","observed_at":now,"coverage":{"source":"fictional-label-provider","observed_at":now,"requested_period":"synthetic requested window","pages_read":1,"pagination_exhausted":true,"failed_reads":[],"exclusions":[],"checkpoint":null},"items":rows})
}
fn save(dir: &tempfile::TempDir, name: &str, value: &Value) -> String {
    let bytes = storage::encoded(value);
    fs::write(dir.path().join(name), &bytes).unwrap();
    storage::digest(&bytes)
}
fn start(engine: &Engine, key: &str) -> String {
    call(engine,"start_run",json!({"operation":"daily_update","scope":{},"user_request":"Review fictional mail only","idempotency_key":key}))["run_id"].as_str().unwrap().into()
}
fn record(engine: &Engine, run: &str, path: &str, hash: &str, key: &str) -> Value {
    call(
        engine,
        "daily_update",
        json!({"action":"record_review","run_id":run,"mail_observations_path":path,"expected_source_sha256":hash,"idempotency_key":key}),
    )
}
fn selection(plan: &Value, run: &str, action: &str, key: &str) -> Value {
    json!({"action":action,"run_id":run,"plan_id":plan["plan_id"],"expected_plan_sha256":plan["plan_sha256"],"selected_action_ids":[plan["archive_plan"]["actions"][0]["action_id"]],"idempotency_key":key})
}
fn without_action(mut args: Value) -> Value {
    args.as_object_mut().unwrap().remove("action");
    args
}
fn approve(engine: &Engine, plan: &Value, run: &str, key: &str) -> Value {
    let mut args = selection(plan, run, "approve_archive", key);
    args["approved_by"] = json!("fictional human via trusted host");
    args["approval_evidence"] = json!(
        "Current human explicitly confirmed the displayed exact plan hash and selected target."
    );
    call(engine, "daily_update", args)
}

#[test]
fn daily_check_inline_is_read_only_and_identity_is_canonical() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let mut item = row("m1", "t1", "rejection", false);
    item["record_path"] = json!("applications/fictional-role.md");
    item["posting_id"] = json!("different-posting");
    let data = call(
        &engine,
        "daily_check",
        json!({"mail_observations":snapshot(vec![item]),"as_of":"2026-10-12"}),
    );
    assert_eq!(data["observations"]["origin"], "supplied");
    assert_eq!(data["observations"]["source_state"], "supplied_inline");
    assert_eq!(
        data["communications"][0]["attribution"],
        "identity_conflict"
    );
    assert!(data["communications"][0].get("record_path").is_none());
    assert_eq!(data["counts"]["archive_candidates"], 1);
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
    assert!(
        !dir.path()
            .join("applications/evidence/communications")
            .exists()
    );
    let mut valid = row("m2", "t2", "correspondence", true);
    valid["record_path"] = json!("applications/fictional-role.md");
    valid["posting_id"] = json!("123");
    let data = call(
        &engine,
        "daily_check",
        json!({"mail_observations":snapshot(vec![valid]),"record":"applications/fictional-role.md"}),
    );
    assert_eq!(
        data["communications"][0]["attribution"],
        "proposed_exact_identity"
    );
    assert_eq!(data["counts"]["needs_reply"], 1);
    assert_eq!(data["counts"]["archive_candidates"], 0);
}

#[test]
fn daily_archive_one_shot_unknown_overlap_and_source_recovery() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    let source = snapshot(vec![row("m1", "t1", "rejection", false)]);
    let hash = save(&dir, "applications/evidence/mail.json", &source);
    let plan = record(
        &engine,
        &run,
        "applications/evidence/mail.json",
        &hash,
        "review",
    );
    assert!(plan.get("plan").is_none());
    assert_eq!(plan["archive_plan"]["actions"].as_array().unwrap().len(), 1);
    approve(&engine, &plan, &run, "approve");
    call(
        &engine,
        "finish_run",
        json!({"run_id":run,"status":"completed","summary":"Human queued only; no external action","evidence":[],"idempotency_key":"human-done"}),
    );
    let host = start(&engine, "host-run");
    let fresh_hash = save(
        &dir,
        "applications/evidence/fresh.json",
        &snapshot(vec![row("m1", "t1", "rejection", false)]),
    );
    let mut begin = selection(&plan, &host, "begin_archive", "begin");
    begin["mail_observations_path"] = json!("applications/evidence/fresh.json");
    begin["expected_source_sha256"] = json!(fresh_hash);
    assert_eq!(
        engine.call(
            "execute_batch",
            &json!({"steps":[{"id":"bad","tool":"begin_mail_archive","arguments":without_action(begin.clone())}]})
        )["error"]["code"],
        "batch_boundary"
    );
    let begun = call(&engine, "begin_mail_archive", without_action(begin.clone()));
    assert_eq!(begun["permits"].as_array().unwrap().len(), 1);
    let replay = call(&engine, "begin_mail_archive", without_action(begin));
    assert_eq!(replay["ready_for_single_archive"], false);
    assert_eq!(replay["permits"], json!([]));
    let complete=engine.call("finish_run",&json!({"run_id":host,"status":"completed","summary":"Must not clear unknown mail work","evidence":[],"idempotency_key":"premature"}));
    assert_eq!(complete["error"]["code"], "invalid_state");
    // The original bounded source may advance; it must not prevent unknown recording or reconciliation.
    fs::write(
        dir.path().join("applications/evidence/mail.json"),
        "changed source",
    )
    .unwrap();
    let action = &plan["archive_plan"]["actions"][0]["action_id"];
    let base =
        json!({"run_id":host,"plan_id":plan["plan_id"],"expected_plan_sha256":plan["plan_sha256"]});
    let mut unknown = base.clone();
    unknown["action"] = json!("record_archive_outcome");
    unknown["idempotency_key"] = json!("unknown");
    unknown["results"] = json!([{"action_id":action,"outcome":"outcome_unknown"}]);
    call(&engine, "daily_update", unknown);
    let mut changed = row("m1", "t1", "rejection", false);
    changed["labels"] = json!(["INBOX", "READ", "NEW_LABEL"]);
    let changed_hash = save(
        &dir,
        "applications/evidence/changed.json",
        &snapshot(vec![changed]),
    );
    let changed_plan = record(
        &engine,
        &host,
        "applications/evidence/changed.json",
        &changed_hash,
        "changed-review",
    );
    let mut second = selection(&changed_plan, &host, "approve_archive", "second");
    second["approved_by"] = json!("human");
    second["approval_evidence"] = json!("Explicit selection");
    assert_eq!(
        engine.call("daily_update", &second)["error"]["code"],
        "invalid_state"
    );
    let readback = "applications/evidence/readback.json";
    let proof = json!({"schema_version":1,"action_id":action,"provider":"fictional-label-provider","mailbox_id":"fictional-mailbox","thread_id":"t1","observed_at":domain::now(),"observed_message_ids":["m1"],"observed_labels":{"m1":["READ"]}});
    let proof_hash = save(&dir, readback, &proof);
    let mut outcome = base;
    outcome["action"] = json!("reconcile_archive");
    outcome["idempotency_key"] = json!("resolve");
    outcome["results"] = json!([{"action_id":action,"outcome":"archived","observed_message_ids":["m1"],"observed_labels":{"m1":["READ"]},"evidence":[readback],"readback_source_path":readback,"readback_source_sha256":proof_hash}]);
    let resolved = call(&engine, "daily_update", outcome.clone());
    assert_eq!(resolved["archive_actions"][0]["status"], "archived");
    assert_eq!(
        resolved["archive_actions"][0]["observed_final_state_only"],
        true
    );
    outcome["idempotency_key"] = json!("conflict");
    outcome["results"][0]["outcome"] = json!("failed");
    assert_eq!(
        engine.call("daily_update", &outcome)["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(
        call(&engine, "reconcile_tracker", json!({}))["pending_mail_action_count"],
        0
    );
}

#[test]
fn daily_partial_and_mixed_threads_never_propose_archival() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let mut partial = snapshot(vec![row("m1", "t1", "rejection", false)]);
    partial["coverage"]["failed_reads"] = json!(["a failed read"]);
    assert_eq!(
        call(&engine, "daily_check", json!({"mail_observations":partial}))["counts"]["archive_candidates"],
        0
    );
    let mut one = row("m1", "t1", "application_confirmation", false);
    let mut two = row("m2", "t1", "reply_request", true);
    for item in [&mut one, &mut two] {
        item["thread_message_ids"] = json!(["m1", "m2"]);
        item["latest_message_id"] = json!("m2");
        item["request_message_ids"] = json!(["m2"]);
    }
    let mixed = snapshot(vec![one, two]);
    assert_eq!(
        call(
            &engine,
            "daily_check",
            json!({"mail_observations":mixed.clone()})
        )["counts"]["archive_candidates"],
        0
    );
    let mut bad = mixed.clone();
    bad["items"][0]["supports_message_archive"] = json!(false);
    assert_eq!(
        engine.call("daily_check", &json!({"mail_observations":bad}))["ok"],
        false
    );
    let mut bad = mixed.clone();
    bad["items"][0]["text"] = json!("Corrupted text");
    assert_eq!(
        engine.call("daily_check", &json!({"mail_observations":bad}))["error"]["code"],
        "artifact_changed"
    );
    let mut bad = mixed;
    bad["provider"] = json!("");
    assert_eq!(
        engine.call("daily_check", &json!({"mail_observations":bad}))["ok"],
        false
    );
}

#[test]
fn daily_incremental_scans_keep_requests_and_corrupt_evidence_is_visible() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "r");
    let first = snapshot(vec![row(
        "request-1",
        "request-thread",
        "correspondence",
        true,
    )]);
    let hash = save(&dir, "applications/evidence/first.json", &first);
    let plan = record(
        &engine,
        &run,
        "applications/evidence/first.json",
        &hash,
        "first",
    );
    let second = snapshot(vec![row(
        "receipt-2",
        "receipt-thread",
        "application_confirmation",
        false,
    )]);
    let hash = save(&dir, "applications/evidence/second.json", &second);
    record(
        &engine,
        &run,
        "applications/evidence/second.json",
        &hash,
        "second",
    );
    let data = call(&engine, "daily_check", json!({"company":"Fictional"}));
    assert_eq!(data["page"]["total"], 2);
    assert_eq!(data["counts"]["needs_reply"], 1);
    assert!(
        data["attention"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["message_id"] == "request-1")
    );
    fs::write(
        dir.path().join(
            plan["archive_plan"]["frozen_source"]["path"]
                .as_str()
                .unwrap(),
        ),
        "broken retained original",
    )
    .unwrap();
    let data = call(&engine, "daily_check", json!({}));
    assert_eq!(data["observations"]["retained_evidence_complete"], false);
    assert_eq!(data["observations"]["retained_evidence_gap_count"], 1);
    assert_eq!(data["observations"]["coverage"]["complete"], false);
    assert!(
        data["attention"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "mail_evidence_gap")
    );
}

#[test]
fn daily_read_projection_is_bounded_and_old_routes_are_unchanged() {
    let dir = fixture();
    let mut old: Value =
        serde_json::from_slice(include_bytes!("../../../../../resources/workflows.json")).unwrap();
    old["workflows"]
        .as_object_mut()
        .unwrap()
        .remove("daily_check");
    old["workflows"]
        .as_object_mut()
        .unwrap()
        .remove("daily_update");
    let old_bytes = storage::encoded(&old);
    fs::write(dir.path().join("config/career-workflows.json"), &old_bytes).unwrap();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    assert_eq!(
        fs::read(dir.path().join("config/career-workflows.json")).unwrap(),
        old_bytes
    );
    let rows = (0..100)
        .map(|n| row(&format!("m{n:03}"), &format!("t{n:03}"), "rejection", false))
        .collect();
    let hash = save(&dir, "applications/evidence/many.json", &snapshot(rows));
    let plan = record(
        &engine,
        &run,
        "applications/evidence/many.json",
        &hash,
        "review",
    );
    assert!(storage::encoded(&plan).len() < 64_000);
    let data = call(&engine, "daily_check", json!({"offset":20,"limit":20}));
    assert_eq!(data["page"]["total"], 100);
    assert_eq!(data["communications"].as_array().unwrap().len(), 20);
    assert_eq!(
        data["archive_plan"]["actions"].as_array().unwrap().len(),
        20
    );
    assert!(storage::encoded(&data).len() < 64_000);
    for action in data["archive_plan"]["actions"].as_array().unwrap() {
        assert!(action.get("before_labels").is_none());
        assert!(
            data["communications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["message_id"] == action["message_ids"][0])
        );
    }
    let state: Value = serde_json::from_slice(
        &fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
    )
    .unwrap();
    let mut forged = state.clone();
    for event in forged["events"].as_array_mut().unwrap() {
        if event["tool"] == "daily_update" {
            event["result"]["plan"]["plan_id"] = json!("0".repeat(64));
            break;
        }
    }
    assert!(crate::state::parse(dir.path(), &forged).is_err());
}

#[test]
fn daily_fresh_same_path_reordering_is_valid_but_new_membership_is_stale() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    let source = snapshot(vec![row("m1", "t1", "rejection", false)]);
    let path = "applications/evidence/mail.json";
    let hash = save(&dir, path, &source);
    let plan = record(&engine, &run, path, &hash, "review");
    approve(&engine, &plan, &run, "approve");
    let mut one = row("m1", "t1", "rejection", false);
    let mut two = row("m2", "t1", "rejection", false);
    for item in [&mut one, &mut two] {
        item["thread_message_ids"] = json!(["m1", "m2"]);
        item["latest_message_id"] = json!("m2");
    }
    let changed = snapshot(vec![one, two]);
    let changed_hash = save(&dir, path, &changed);
    let mut begin = selection(&plan, &run, "begin_archive", "stale-begin");
    begin["mail_observations_path"] = json!(path);
    begin["expected_source_sha256"] = json!(changed_hash);
    assert_eq!(
        engine.call("begin_mail_archive", &without_action(begin.clone()))["error"]["code"],
        "stale_plan"
    );
    let mut same = source;
    same["items"][0]["labels"] = json!(["READ", "INBOX"]);
    let hash = save(&dir, path, &same);
    begin["idempotency_key"] = json!("begin");
    begin["expected_source_sha256"] = json!(hash);
    let data = call(&engine, "begin_mail_archive", without_action(begin));
    assert_eq!(data["ready_for_single_archive"], true);
}

#[test]
fn daily_partial_thread_preserves_request_and_interviews_use_recorded_dates() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    let first = snapshot(vec![row("request", "thread", "correspondence", true)]);
    let hash = save(&dir, "applications/evidence/first.json", &first);
    record(
        &engine,
        &run,
        "applications/evidence/first.json",
        &hash,
        "first",
    );
    let mut partial = row("receipt", "thread", "application_confirmation", false);
    partial["thread_message_ids"] = json!(["request", "receipt"]);
    partial["latest_message_id"] = json!("receipt");
    partial["thread_complete"] = json!(false);
    let data = call(
        &engine,
        "daily_check",
        json!({"mail_observations":snapshot(vec![partial])}),
    );
    assert_eq!(data["counts"]["needs_reply"], 1);
    assert_eq!(data["page"]["total"], 2);
    for (name, date) in [
        ("past", "2026-10-11T11:00:00Z"),
        ("future", "2026-10-13T11:00:00Z"),
        ("unscheduled", ""),
    ] {
        let scheduling = if date.is_empty() {
            String::new()
        } else {
            format!(",\"schedule_state\":\"booked\",\"scheduled_start\":\"{date}\"")
        };
        fs::write(dir.path().join(format!("applications/{name}.md")),format!("# Fictional interview {name}\n<!-- tracking\n{{\"stage\":\"screening\",\"checked\":\"2026-10-12\",\"next_action\":\"Review\"{scheduling}}}\n-->\n")).unwrap();
    }
    let data = call(&engine, "daily_check", json!({"as_of":"2026-10-12"}));
    assert_eq!(data["counts"]["past_interviews"], 1);
    assert_eq!(data["counts"]["upcoming_interviews"], 1);
    assert_eq!(data["counts"]["unscheduled_interviews"], 1);
}

fn begun_archive(rows: Vec<Value>) -> (tempfile::TempDir, Engine, String, Value) {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    let source = snapshot(rows);
    let path = "applications/evidence/mail.json";
    let hash = save(&dir, path, &source);
    let plan = record(&engine, &run, path, &hash, "review");
    approve(&engine, &plan, &run, "approve");
    let mut begin = without_action(selection(&plan, &run, "begin_archive", "begin"));
    begin["mail_observations_path"] = json!(path);
    begin["expected_source_sha256"] = json!(hash);
    call(&engine, "begin_mail_archive", begin);
    (dir, engine, run, plan)
}
fn readback_result(
    dir: &tempfile::TempDir,
    plan: &Value,
    outcome: &str,
    ids: Value,
    labels: Value,
) -> Value {
    let action = &plan["archive_plan"]["actions"][0];
    let path = "applications/evidence/readback.json";
    let proof = json!({"schema_version":1,"action_id":action["action_id"],"provider":action["provider"],"mailbox_id":action["mailbox_id"],"thread_id":action["thread_id"],"observed_at":domain::now(),"observed_message_ids":ids,"observed_labels":labels});
    let hash = save(dir, path, &proof);
    json!({"action_id":action["action_id"],"outcome":outcome,"observed_message_ids":ids,"observed_labels":labels,"evidence":[path],"readback_source_path":path,"readback_source_sha256":hash})
}
fn outcome_args(plan: &Value, run: &str, result: Value, key: &str) -> Value {
    json!({"action":"reconcile_archive","run_id":run,"plan_id":plan["plan_id"],"expected_plan_sha256":plan["plan_sha256"],"results":[result],"idempotency_key":key})
}

#[test]
fn daily_label_drift_resolves_observed_state_with_exact_retained_proof() {
    for (outcome, labels) in [
        ("archived", json!(["UNREAD", "AUTO_LABEL"])),
        ("failed", json!(["INBOX", "UNREAD", "AUTO_LABEL"])),
    ] {
        let (dir, engine, run, plan) = begun_archive(vec![row("m1", "t1", "rejection", false)]);
        let mut result = readback_result(&dir, &plan, outcome, json!(["m1"]), json!({"m1":labels}));
        let args = outcome_args(&plan, &run, result.clone(), "clean-drift");
        assert_eq!(
            engine.call("daily_update", &args)["error"]["code"],
            "adapter_mismatch"
        );
        result["label_changes"] = json!(true);
        let mut bad = result.clone();
        bad["readback_source_sha256"] = json!("0".repeat(64));
        assert_eq!(
            engine.call("daily_update", &outcome_args(&plan, &run, bad, "bad-hash"))["error"]["code"],
            "artifact_changed"
        );
        let data = call(
            &engine,
            "daily_update",
            outcome_args(&plan, &run, result.clone(), "drift-resolved"),
        );
        let action = &data["archive_actions"][0];
        assert_eq!(action["status"], outcome);
        assert_eq!(action["label_changes"], true);
        assert_eq!(action["observed_final_state_only"], true);
        assert_eq!(
            action["label_differences"]["m1"]["unexpected_changes"],
            true
        );
        assert!(
            action["label_differences"]["m1"]["removed"]
                .as_array()
                .unwrap()
                .contains(&json!("READ"))
        );
        let retained = dir
            .path()
            .join(action["readback"]["path"].as_str().unwrap());
        let bytes = fs::read(&retained).unwrap();
        assert_eq!(
            storage::digest(&bytes),
            result["readback_source_sha256"].as_str().unwrap()
        );
        assert_eq!(
            bytes,
            fs::read(dir.path().join("applications/evidence/readback.json")).unwrap()
        );
        fs::write(
            dir.path().join("applications/evidence/readback.json"),
            "changed mutable original",
        )
        .unwrap();
        assert_eq!(
            storage::digest(&fs::read(retained).unwrap()),
            result["readback_source_sha256"].as_str().unwrap()
        );
        assert_eq!(
            call(&engine, "reconcile_tracker", json!({}))["pending_mail_action_count"],
            0
        );
    }
}

#[test]
fn daily_partial_or_missing_readback_targets_remain_unknown() {
    let mut first = row("m1", "t1", "rejection", false);
    let mut second = row("m2", "t1", "application_confirmation", false);
    for row in [&mut first, &mut second] {
        row["supports_message_archive"] = json!(false);
        row["thread_message_ids"] = json!(["m1", "m2"]);
        row["latest_message_id"] = json!("m2");
    }
    let (dir, engine, run, plan) = begun_archive(vec![first, second]);
    let mut partial = readback_result(
        &dir,
        &plan,
        "archived",
        json!(["m1", "m2"]),
        json!({"m1":["READ"],"m2":["INBOX","READ"]}),
    );
    partial["label_changes"] = json!(true);
    assert_eq!(
        engine.call(
            "daily_update",
            &outcome_args(&plan, &run, partial, "partial")
        )["error"]["code"],
        "adapter_mismatch"
    );
    let mut missing = readback_result(
        &dir,
        &plan,
        "failed",
        json!(["m1"]),
        json!({"m1":["INBOX","READ"]}),
    );
    missing["label_changes"] = json!(true);
    assert_eq!(
        engine.call(
            "daily_update",
            &outcome_args(&plan, &run, missing, "missing")
        )["error"]["code"],
        "invalid_arguments"
    );
    let unknown = json!({"action_id":plan["archive_plan"]["actions"][0]["action_id"],"outcome":"outcome_unknown"});
    call(
        &engine,
        "daily_update",
        outcome_args(&plan, &run, unknown, "unknown"),
    );
    assert_eq!(
        call(&engine, "reconcile_tracker", json!({}))["pending_mail_action_count"],
        1
    );
}

#[test]
fn daily_withdraw_is_human_selected_queued_only_and_can_be_reapproved() {
    let dir = fixture();
    let engine = Engine::open(dir.path()).unwrap();
    let run = start(&engine, "run");
    let path = "applications/evidence/mail.json";
    let source = snapshot(vec![row("m1", "t1", "rejection", false)]);
    let hash = save(&dir, path, &source);
    let plan = record(&engine, &run, path, &hash, "review");
    approve(&engine, &plan, &run, "approve");
    let frozen = dir.path().join(
        plan["archive_plan"]["frozen_source"]["path"]
            .as_str()
            .unwrap(),
    );
    let original = fs::read(&frozen).unwrap();
    fs::remove_file(&frozen).unwrap();
    let mut withdrawal = selection(&plan, &run, "withdraw_archive", "withdraw");
    withdrawal["approved_by"] = json!("fictional human");
    withdrawal["approval_evidence"] =
        json!("Current human confirmed withdrawing these exact queued IDs and plan hash");
    withdrawal["reason"] = json!("Keep these messages in the inbox");
    let data = call(&engine, "daily_update", withdrawal.clone());
    assert_eq!(data["status"], "withdrawn");
    assert_eq!(data["archive_actions"][0]["status"], "withdrawn");
    assert_eq!(
        data["archive_actions"][0]["plan_sha256"],
        plan["plan_sha256"]
    );
    assert_eq!(data["ready_for_single_archive"], false);
    assert_eq!(data["permits"], json!([]));
    assert_eq!(
        call(&engine, "reconcile_tracker", json!({}))["pending_mail_action_count"],
        0
    );
    fs::write(&frozen, original).unwrap();
    approve(&engine, &plan, &run, "reapprove");
    let mut begin = without_action(selection(&plan, &run, "begin_archive", "begin"));
    begin["mail_observations_path"] = json!(path);
    begin["expected_source_sha256"] = json!(hash);
    call(&engine, "begin_mail_archive", begin);
    withdrawal["idempotency_key"] = json!("withdraw-started");
    assert_eq!(
        engine.call("daily_update", &withdrawal)["error"]["code"],
        "invalid_state"
    );
    let unknown = json!({"action_id":plan["archive_plan"]["actions"][0]["action_id"],"outcome":"outcome_unknown"});
    call(
        &engine,
        "daily_update",
        outcome_args(&plan, &run, unknown, "unknown"),
    );
    withdrawal["idempotency_key"] = json!("withdraw-unknown");
    assert_eq!(
        engine.call("daily_update", &withdrawal)["error"]["code"],
        "invalid_state"
    );
    assert_eq!(
        call(&engine, "reconcile_tracker", json!({}))["pending_mail_action_count"],
        1
    );
}
