use super::*;
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["applications", "context/profile", "output", "config"] {
        fs::create_dir_all(dir.path().join(name)).unwrap();
    }
    fs::write(
        dir.path().join("config/career-ledger.json"),
        r#"{"schema_version":1,"timezone":"UTC","primary_cv":"output/cv.pdf"}"#,
    )
    .unwrap();
    fs::write(dir.path().join("applications/acme-engineer-123.md"),"# Acme — Senior Engineer\n\n- Stage: saved\n- Source URL: https://jobs.example.com/acme/123?utm_source=old\n").unwrap();
    fs::write(
        dir.path().join("applications/approved-answers.md"),
        "# Answers\n\n## Salary\nConfirmed salary fixture.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("context/profile/application-decisions.md"),
        "# Decisions\n\n## Experience\nSourced fixture.\n",
    )
    .unwrap();
    fs::write(dir.path().join("output/cv.pdf"), b"%PDF-1.7\nfixture\n").unwrap();
    fs::write(
        dir.path().join("applications/answers.json"),
        r#"{"experience":6}"#,
    )
    .unwrap();
    dir
}
fn call(h: &Engine, tool: &str, args: Value) -> Value {
    let value = h.call(tool, &args);
    assert_eq!(value["ok"], true, "{tool}: {value}");
    contracts::validate(&contracts::specs()[tool]["outputSchema"], &value).unwrap();
    value["data"].clone()
}
fn start(h: &Engine, max: usize) -> String {
    call(h,"start_run",json!({"operation":"apply","scope":{"sources":["employer"],"max_submissions":max,"dry_run":false},"user_request":"Synthetic test","idempotency_key":"run"}))["run_id"].as_str().unwrap().into()
}
fn prepare(h: &Engine, run: &str, key: &str) -> Value {
    call(
        h,
        "prepare_application",
        json!({"run_id":run,"record":"applications/acme-engineer-123.md","company":"Acme","vacancy_url":"https://jobs.example.com/acme/123","source":"employer","posting_id":"123","cv_path":"output/cv.pdf","answers_path":"applications/answers.json","idempotency_key":key}),
    )
}
#[test]
fn all_contracts_reject_extra_input_without_writes() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    for name in contracts::specs().keys() {
        let value = h.call(name, &json!({"unexpected":true}));
        assert_eq!(value["error"]["code"], "invalid_arguments", "{name}");
    }
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
}
#[test]
fn batches_chain_values_and_preserve_partial_success() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let steps = json!([{"id":"start","tool":"start_run","arguments":{"operation":"reconcile","scope":{},"user_request":"fixture","idempotency_key":"r"}},{"id":"inspect","tool":"get_run","arguments":{"run_id":{"$ref":"start.data.run_id"}}},{"id":"bad","tool":"read_record","arguments":{"path":"../secret.md"}},{"id":"later","tool":"find_application","arguments":{}}]);
    let result = h.call("execute_batch", &json!({"steps":steps}));
    assert_eq!(result["ok"], false);
    assert_eq!(result["data"]["completed"], 2);
    assert_eq!(result["data"]["skipped"], json!(["later"]));
    assert_eq!(
        h.load_state().unwrap()["runs"].as_object().unwrap().len(),
        1
    );
}
#[test]
fn batches_reject_submit_boundaries_before_any_write() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let result=h.call("execute_batch",&json!({"steps":[{"id":"start","tool":"start_run","arguments":{"operation":"reconcile","scope":{},"user_request":"fixture","idempotency_key":"r"}},{"id":"begin","tool":"begin_submission","arguments":{}}]}));
    assert_eq!(result["error"]["code"], "batch_boundary");
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
}
#[test]
fn compound_preparation_replays_and_begin_is_one_shot() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let prepared = prepare(&h, &run, "prepare");
    assert_eq!(prepared, prepare(&h, &run, "prepare"));
    let args = json!({"attempt_id":prepared["attempt_id"],"idempotency_key":"begin"});
    assert_eq!(
        call(&h, "begin_submission", args.clone())["ready_for_single_submit"],
        true
    );
    assert_eq!(
        call(&h, "begin_submission", args)["ready_for_single_submit"],
        false
    );
    let again = h.call(
        "begin_submission",
        &json!({"attempt_id":prepared["attempt_id"],"idempotency_key":"other"}),
    );
    assert_eq!(again["error"]["code"], "submit_replay_blocked");
}
#[test]
fn unknown_outcome_blocks_retry_and_run_completion_forever() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    call(
        &h,
        "begin_submission",
        json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
    );
    call(
        &h,
        "finish_submission",
        json!({"attempt_id":p["attempt_id"],"outcome":"outcome_unknown","summary":"Lost response","evidence":[],"idempotency_key":"unknown"}),
    );
    assert_eq!(
        call(
            &h,
            "check_duplicate",
            json!({"company":"Acme","vacancy_url":"https://jobs.example.com/acme/123"})
        )["blocked"],
        true
    );
    let completed=h.call("finish_run",&json!({"run_id":run,"status":"completed","summary":"done","evidence":[],"idempotency_key":"finish"}));
    assert_eq!(completed["ok"], false);
}
#[test]
fn changed_answers_do_not_start_reserved_submission() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    fs::write(
        dir.path().join("applications/approved-answers.md"),
        "New sourced answer",
    )
    .unwrap();
    let result = h.call(
        "begin_submission",
        &json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
    );
    assert_eq!(result["error"]["code"], "answers_changed");
    assert_eq!(
        h.load_state().unwrap()["attempts"][p["attempt_id"].as_str().unwrap()]["status"],
        "reserved"
    );
    assert_eq!(h.call("prepare_application",&json!({"run_id":run,"record":"applications/acme-engineer-123.md","company":"Acme","vacancy_url":"https://jobs.example.com/acme/123","source":"employer","posting_id":"123","cv_path":"output/cv.pdf","answers_path":"applications/answers.json","idempotency_key":"prepare"}))["error"]["code"],"artifact_changed");
}
#[test]
fn cancelled_attempts_still_count_toward_total_budget() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    for i in 0..3 {
        let p = prepare(&h, &run, &format!("prepare{i}"));
        call(
            &h,
            "cancel_reservation",
            json!({"attempt_id":p["attempt_id"],"summary":"Changed form","idempotency_key":format!("cancel{i}")}),
        );
    }
    let args = json!({"run_id":run,"record":"applications/acme-engineer-123.md","company":"Acme","vacancy_url":"https://jobs.example.com/acme/123","source":"employer","posting_id":"123","cv_path":"output/cv.pdf","answers_path":"applications/answers.json","idempotency_key":"prepare4"});
    assert_eq!(
        h.call("prepare_application", &args)["error"]["code"],
        "cap_reached"
    );
}
#[test]
fn missing_linked_record_does_not_resolve_attempt() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    call(
        &h,
        "begin_submission",
        json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
    );
    fs::remove_file(dir.path().join("applications/acme-engineer-123.md")).unwrap();
    let result=h.call("finish_submission",&json!({"attempt_id":p["attempt_id"],"outcome":"submitted","summary":"Submitted","evidence":["https://jobs.example.com/receipt"],"idempotency_key":"finish"}));
    assert_eq!(result["error"]["code"], "missing_resource");
    assert_eq!(
        h.load_state().unwrap()["attempts"][p["attempt_id"].as_str().unwrap()]["status"],
        "submitting"
    );
}
#[test]
fn later_interview_stage_survives_old_receipt() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    call(
        &h,
        "begin_submission",
        json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
    );
    call(
        &h,
        "update_application",
        json!({"run_id":run,"record":"applications/acme-engineer-123.md","tracking":{"stage":"technical","checked":"2026-10-12","next_action":"Prepare interview"},"summary":"Interview invitation","evidence":["https://jobs.example.com/interview"],"idempotency_key":"invite"}),
    );
    call(
        &h,
        "finish_submission",
        json!({"attempt_id":p["attempt_id"],"outcome":"submitted","summary":"Found original receipt","evidence":["https://jobs.example.com/receipt"],"idempotency_key":"finish"}),
    );
    assert_eq!(
        pipeline::build(dir.path()).unwrap().records[0]["stage"],
        "technical"
    );
}
#[test]
fn journal_conflicts_preserve_manual_edits_and_diagnostic_reads() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let path = dir.path().join("applications/acme-engineer-123.md");
    let before = domain::digest(&fs::read(&path).unwrap());
    let mut state = h.load_state().unwrap();
    state["pending"] = json!([{"path":"applications/acme-engineer-123.md","before_sha256":before,"text":"# Acme\n\n<!-- tracking\n{\"stage\":\"applied\",\"checked\":\"2026-10-12\",\"next_action\":\"Monitor\"}\n-->\n"}]);
    storage::atomic_write(
        &dir.path().join("applications/operations/state.json"),
        &domain::encoded(&state).unwrap(),
    )
    .unwrap();
    fs::write(&path, "# Acme — Engineer\n\nHuman update\n").unwrap();
    let result = h.call("reconcile_tracker", &json!({}));
    assert_eq!(result["error"]["code"], "pending_recovery");
    assert_eq!(result["next"][0]["tool"], "rebuild_pipeline");
    assert_eq!(
        h.call(
            "read_record",
            &json!({"path":"applications/acme-engineer-123.md"})
        )["ok"],
        true
    );
    assert_eq!(
        h.call("rebuild_pipeline", &json!({"idempotency_key":"recover"}))["error"]["code"],
        "recovery_conflict"
    );
    assert!(fs::read_to_string(path).unwrap().contains("Human update"));
}
#[test]
fn packet_tampering_is_typed() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    let packet = p["packet_id"].as_str().unwrap();
    fs::write(
        dir.path()
            .join(format!("applications/operations/packets/{packet}.json")),
        "{}",
    )
    .unwrap();
    assert_eq!(
        h.call("get_packet", &json!({"packet_id":packet}))["error"]["code"],
        "artifact_changed"
    );
}
#[test]
fn booking_completion_preserves_previous_booking_history() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    for (stage, start, key) in [
        ("booked", Some("2026-10-13T11:00:00+01:00"), "book"),
        ("completed", None, "complete"),
    ] {
        let mut tracking = json!({"stage":"screening","checked":"2026-10-12","next_action":"Await feedback","schedule_state":stage});
        if let Some(start) = start {
            tracking["scheduled_start"] = json!(start);
        }
        call(
            &h,
            "update_application",
            json!({"run_id":run,"record":"applications/acme-engineer-123.md","tracking":tracking,"summary":"Fixture scheduling","evidence":["https://example.com/calendar"],"idempotency_key":key}),
        );
    }
    let text = fs::read_to_string(dir.path().join("applications/acme-engineer-123.md")).unwrap();
    assert!(text.contains("Previous booked time: 2026-10-13T11:00:00+01:00"));
    assert!(
        pipeline::build(dir.path()).unwrap().records[0]
            .get("scheduled_start")
            .is_none()
    );
}
#[test]
fn idempotency_is_bound_to_the_exact_input() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let args = json!({"operation":"reconcile","scope":{},"user_request":"fixture","idempotency_key":"same"});
    let first = call(&h, "start_run", args.clone());
    assert_eq!(first, call(&h, "start_run", args.clone()));
    let mut changed = args;
    changed["user_request"] = json!("changed");
    assert_eq!(
        h.call("start_run", &changed)["error"]["code"],
        "idempotency_conflict"
    );
}
#[test]
fn exact_references_are_data_not_code() {
    let mut values = BTreeMap::new();
    values.insert("first".into(), json!({"data":{"rows":["safe"]}}));
    assert_eq!(
        resolve_refs(&json!({"$ref":"first.data.rows.0"}), &values).unwrap(),
        "safe"
    );
    assert!(resolve_refs(&json!({"$ref":"first","extra":true}), &values).is_err());
    assert!(resolve_refs(&json!({"$ref":"first.data.missing"}), &values).is_err());
}

#[test]
fn concurrent_reservations_have_one_canonical_winner() {
    use std::sync::{Arc, Barrier};
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let r1 = start(&h, 1);
    let r2=call(&h,"start_run",json!({"operation":"apply","scope":{"sources":["employer"],"max_submissions":1,"dry_run":false},"user_request":"Second synthetic worker","idempotency_key":"run2"}))["run_id"].as_str().unwrap().to_owned();
    let packet_args = |run: &str, key: &str| json!({"run_id":run,"record":"applications/acme-engineer-123.md","company":"Acme","vacancy_url":"https://jobs.example.com/acme/123","source":"employer","posting_id":"123","cv_path":"output/cv.pdf","answers_path":"applications/answers.json","idempotency_key":key});
    let p1 = call(&h, "prepare_packet", packet_args(&r1, "packet1"));
    let p2 = call(&h, "prepare_packet", packet_args(&r2, "packet2"));
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = vec![];
    for (run, packet) in [(r1, p1), (r2, p2)] {
        let root = dir.path().to_path_buf();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move||{let h=Engine::open(root).unwrap();barrier.wait();h.call("reserve_submission",&json!({"run_id":run,"packet_id":packet["packet_id"],"idempotency_key":format!("reserve:{run}")}))}));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r["ok"] == true).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r["error"]["code"] == "duplicate_blocked")
            .count(),
        1
    );
    assert_eq!(
        h.load_state().unwrap()["attempts"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn interrupted_journal_finishes_exact_local_change() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let path = dir.path().join("applications/acme-engineer-123.md");
    let before = domain::digest(&fs::read(&path).unwrap());
    let text = "# Acme — Engineer\n\n<!-- tracking\n{\"stage\":\"applied\",\"checked\":\"2026-10-12\",\"next_action\":\"Monitor response\"}\n-->\n";
    let mut state = h.load_state().unwrap();
    state["pending"] =
        json!([{"path":"applications/acme-engineer-123.md","before_sha256":before,"text":text}]);
    storage::atomic_write(
        &dir.path().join("applications/operations/state.json"),
        &domain::encoded(&state).unwrap(),
    )
    .unwrap();
    call(
        &h,
        "rebuild_pipeline",
        json!({"idempotency_key":"recovery"}),
    );
    assert_eq!(fs::read_to_string(path).unwrap(), text);
    assert!(h.load_state().unwrap().get("pending").is_none());
    assert_eq!(
        call(&h, "reconcile_tracker", json!({}))["pipeline_stale"],
        false
    );
}

#[test]
fn provider_observations_and_repeated_failure_recovery_are_bounded() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let args = json!({"run_id":run,"record_path":"applications/acme-engineer-123.md","provider":"fixture-board","target_url":"https://board.example.com/123","capabilities":["board.sync"]});
    let plan = call(&h, "plan_adapter", args.clone());
    let action = &plan["actions"][0];
    let mismatch=h.call("record_adapter_result",&json!({"run_id":run,"record_path":"applications/acme-engineer-123.md","provider":"fixture-board","target_url":"https://board.example.com/123","plan_id":plan["plan_id"],"outcome":"verified","observed_stage":"applied","observed_note":action["note"],"evidence":["https://board.example.com/123"],"idempotency_key":"bad"}));
    assert_eq!(mismatch["error"]["code"], "adapter_mismatch");
    call(
        &h,
        "record_adapter_result",
        json!({"run_id":run,"record_path":"applications/acme-engineer-123.md","provider":"fixture-board","target_url":"https://board.example.com/123","plan_id":plan["plan_id"],"outcome":"verified","observed_stage":action["desired_stage"],"observed_note":action["note"],"evidence":["https://board.example.com/123"],"idempotency_key":"sync"}),
    );
    for i in 0..2 {
        call(
            &h,
            "record_adapter_result",
            json!({"run_id":run,"record_path":"applications/acme-engineer-123.md","provider":"fixture-board","target_url":"https://board.example.com/123","plan_id":plan["plan_id"],"outcome":"failed","failure_code":"upload_control_missing","evidence":["https://board.example.com/123"],"idempotency_key":format!("failure{i}")}),
        );
    }
    let mut recovery = args;
    recovery["failure_code"] = json!("upload_control_missing");
    assert_eq!(call(&h, "plan_adapter", recovery)["stop_retrying"], true);
}

#[test]
fn all_capabilities_can_return_their_published_output_shape() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    call(
        &h,
        "bootstrap",
        json!({"operation":"apply","capabilities":["web.read"]}),
    );
    call(&h, "list_operations", json!({}));
    call(&h, "plan_workflow", json!({"operation":"reconcile"}));
    call(&h, "get_approved_answer", json!({"query":"salary"}));
    call(
        &h,
        "inspect_tools",
        json!({"names":["prepare_application"],"include_schemas":true}),
    );
    let run = start(&h, 1);
    call(
        &h,
        "record_event",
        json!({"run_id":run,"kind":"mail_reviewed","summary":"Read source mail","evidence":["https://mail.example.com/thread"],"idempotency_key":"mail"}),
    );
    call(
        &h,
        "create_application",
        json!({"run_id":run,"record":"applications/other-engineer-999.md","company":"Other","role":"Engineer","vacancy_url":"https://jobs.example.com/other/999","source":"employer","fit_reason":"Synthetic fixture fit","evidence":["https://jobs.example.com/other/999"],"idempotency_key":"create"}),
    );
    call(
        &h,
        "save_receipt",
        json!({"run_id":run,"path":"applications/answers.json","idempotency_key":"receipt"}),
    );
    call(&h, "get_run", json!({"run_id":run}));
    call(&h, "list_events", json!({"run_id":run,"limit":50}));
    call(&h, "find_application", json!({"limit":1}));
    call(
        &h,
        "finish_run",
        json!({"run_id":run,"status":"completed","summary":"Synthetic checks finished","evidence":[],"idempotency_key":"done"}),
    );
}

#[test]
fn conflict_resolution_preserves_losing_bytes_and_allows_external_resolution() {
    for keep in ["current", "journal"] {
        let dir = fixture();
        let h = Engine::open(dir.path()).unwrap();
        let run = start(&h, 1);
        let p = prepare(&h, &run, "prepare");
        call(
            &h,
            "begin_submission",
            json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
        );
        let path = dir.path().join("applications/acme-engineer-123.md");
        let before = domain::digest(&fs::read(&path).unwrap());
        let wanted = "# Acme — Engineer\n\n<!-- tracking\n{\"stage\":\"applied\",\"checked\":\"2026-10-12\",\"next_action\":\"Monitor\",\"company\":\"Acme\",\"vacancy_url\":\"https://jobs.example.com/acme/123\"}\n-->\n";
        let current = "# Acme — Engineer\n\n- Stage: technical\n- Source URL: https://jobs.example.com/acme/123\n\nManual interview note\n";
        let mut state = h.load_state().unwrap();
        state["pending"] = json!([{"path":"applications/acme-engineer-123.md","before_sha256":before,"text":wanted}]);
        storage::atomic_write(
            &dir.path().join("applications/operations/state.json"),
            &domain::encoded(&state).unwrap(),
        )
        .unwrap();
        fs::write(&path, current).unwrap();
        let pending = h.pending_transaction().unwrap();
        let sha = pending["pending_sha256"].as_str().unwrap();
        let evidence = vec!["https://jobs.example.com/reconciliation".to_owned()];
        let result = h.resolve_pending(keep, &evidence, "resolve", sha).unwrap();
        let losing = if keep == "current" { wanted } else { current };
        let copy = result["conflict_copies"][0]["path"].as_str().unwrap();
        assert_eq!(fs::read_to_string(dir.path().join(copy)).unwrap(), losing);
        assert_eq!(
            result,
            h.resolve_pending(keep, &evidence, "resolve", sha).unwrap()
        );
        assert!(h.load_state().unwrap().get("pending").is_none());
        call(
            &h,
            "finish_submission",
            json!({"attempt_id":p["attempt_id"],"outcome":"submitted","summary":"Verified external receipt","evidence":["https://jobs.example.com/receipt"],"idempotency_key":"finish"}),
        );
        assert_eq!(
            h.load_state().unwrap()["attempts"][p["attempt_id"].as_str().unwrap()]["status"],
            "submitted"
        );
    }
}

#[test]
fn duplicate_change_paths_are_rejected_before_journal_commit() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let path = h
        .record_path("applications/acme-engineer-123.md", true)
        .unwrap();
    let text = "# Acme\n\n<!-- tracking\n{\"stage\":\"saved\",\"checked\":\"2026-10-12\",\"next_action\":\"review\"}\n-->\n";
    let mut changes = vec![];
    h.change(&path, text.into(), &mut changes).unwrap();
    assert_eq!(
        h.change(&path, text.into(), &mut changes).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(changes.len(), 1);
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
}

#[test]
fn malformed_state_returns_typed_errors_and_preserves_unknown_fields() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let mut state = h.load_state().unwrap();
    state["future_extension"] = json!({"origin":"external-tool","number":1.0});
    state["runs"][&run]["future"] = json!(["preserve",{"x":true}]);
    storage::atomic_write(
        &dir.path().join("applications/operations/state.json"),
        &domain::encoded(&state).unwrap(),
    )
    .unwrap();
    call(
        &h,
        "record_event",
        json!({"run_id":run,"kind":"reconciled","summary":"Synthetic event","evidence":["https://example.com/source"],"idempotency_key":"event"}),
    );
    let changed = h.load_state().unwrap();
    assert_eq!(changed["future_extension"], state["future_extension"]);
    assert_eq!(
        changed["runs"][&run]["future"],
        state["runs"][&run]["future"]
    );
    let mut malformed = changed;
    malformed["runs"][&run] = json!([]);
    storage::atomic_write(
        &dir.path().join("applications/operations/state.json"),
        &domain::encoded(&malformed).unwrap(),
    )
    .unwrap();
    assert_eq!(
        h.call("get_run", &json!({"run_id":run}))["error"]["code"],
        "invalid_state"
    );
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        domain::encoded(&malformed).unwrap()
    );
}

#[test]
fn finish_and_cancel_can_resolve_after_the_run_is_partial() {
    for started in [false, true] {
        let dir = fixture();
        let h = Engine::open(dir.path()).unwrap();
        let run = start(&h, 1);
        let p = prepare(&h, &run, "prepare");
        if started {
            call(
                &h,
                "begin_submission",
                json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
            );
        }
        call(
            &h,
            "finish_run",
            json!({"run_id":run,"status":"partial","summary":"Session ended","evidence":[],"idempotency_key":"end"}),
        );
        if started {
            call(
                &h,
                "finish_submission",
                json!({"attempt_id":p["attempt_id"],"outcome":"not_submitted","summary":"Verified no submission","evidence":["https://jobs.example.com/form"],"idempotency_key":"resolve"}),
            );
        } else {
            call(
                &h,
                "cancel_reservation",
                json!({"attempt_id":p["attempt_id"],"summary":"Unstarted","idempotency_key":"cancel"}),
            );
        }
        assert_eq!(
            h.load_state().unwrap()["attempts"][p["attempt_id"].as_str().unwrap()]["status"],
            "cancelled"
        );
    }
}

#[test]
fn derived_search_preserves_source_kind_and_does_not_drive_submission_guards() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("applications/briefs")).unwrap();
    fs::write(
        dir.path().join("context/evidence.md"),
        "# Evidence\nSourced database transaction story.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("applications/briefs/acme.md"),
        "# Practice\nHypothetical database transaction answer.\n",
    )
    .unwrap();
    let h = Engine::open(dir.path()).unwrap();
    let result = call(
        &h,
        "search_workspace",
        json!({"query":"database transaction","limit":5}),
    );
    assert_eq!(result["authority"], "derived");
    assert_eq!(result["matches"].as_array().unwrap().len(), 2);
    assert!(
        result["matches"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "coaching")
    );
    let before = call(
        &h,
        "check_duplicate",
        json!({"company":"Acme","vacancy_url":"https://jobs.example.com/acme/123"}),
    );
    assert_eq!(before["blocked"], false);
    let tools = call(
        &h,
        "inspect_tools",
        json!({"names":["search_workspace"],"include_schemas":true}),
    );
    assert_eq!(tools["tools"][0]["read_only"], true);
    let batch = call(
        &h,
        "query_batch",
        json!({"steps":[{"id":"search","tool":"search_workspace","arguments":{"query":"database"}}]}),
    );
    assert_eq!(batch["completed"], 1);
}

#[cfg(unix)]
#[test]
fn mutation_refuses_internal_record_symlinks_before_committing_a_journal() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let path = dir.path().join("applications/acme-engineer-123.md");
    let target = dir.path().join("context/linked-record.md");
    fs::rename(&path, &target).unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let before = fs::read(dir.path().join("applications/operations/state.json")).unwrap();
    assert_eq!(
        h.call(
            "read_record",
            &json!({"path":"applications/acme-engineer-123.md"})
        )["ok"],
        true
    );
    let result=h.call("update_application",&json!({"run_id":run,"record":"applications/acme-engineer-123.md","tracking":{"stage":"prepared","checked":"2026-10-12","next_action":"review"},"summary":"fixture","evidence":["https://example.com/source"],"idempotency_key":"update"}));
    assert_eq!(result["error"]["code"], "path_denied");
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        before
    );
    assert!(path.is_symlink());
}

#[cfg(unix)]
#[test]
fn coordinator_symlinks_are_refused_without_replacing_them() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("applications/operations")).unwrap();
    let target = dir.path().join("context/state.json");
    fs::write(
        &target,
        r#"{"schema_version":1,"runs":{},"attempts":{},"events":[]}"#,
    )
    .unwrap();
    let path = dir.path().join("applications/operations/state.json");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert_eq!(
        Engine::open(dir.path()).err().unwrap().code,
        "unsupported_layout"
    );
    assert!(path.is_symlink());
}

#[test]
fn attention_queue_only_claims_recorded_or_supplied_state() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    call(
        &h,
        "begin_submission",
        json!({"attempt_id":p["attempt_id"],"idempotency_key":"begin"}),
    );
    let mail = dir.path().join("context/mail-observation.json");
    fs::write(&mail,r#"{"observed_at":"2026-10-12T12:00:00Z","items":[{"id":"fixture-mail","subject":"Pick a time","thread_url":"https://mail.example.com/thread","needs_reply":true},{"id":"receipt","needs_reply":false}]}"#).unwrap();
    let queue = call(
        &h,
        "review_queue",
        json!({"mail_observations_path":"context/mail-observation.json","limit":50}),
    );
    assert_eq!(queue["live_mail_checked"], false);
    assert_eq!(queue["authority"], "recorded_local_state");
    assert_eq!(queue["counts"]["unknown_submissions"], 1);
    assert_eq!(queue["counts"]["supplied_mail"], 1);
    assert_eq!(
        queue["mail_source"]["path"],
        "context/mail-observation.json"
    );
    assert!(
        queue["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "supplied_mail")
    );
}

#[test]
fn packet_preview_reports_hash_diffs_without_changing_an_attempt() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let p = prepare(&h, &run, "prepare");
    let before = fs::read(dir.path().join("applications/operations/state.json")).unwrap();
    let original = call(&h, "preview_packet", json!({"packet_id":p["packet_id"]}));
    assert_eq!(original["stale"], false);
    fs::write(
        dir.path().join("applications/answers.json"),
        r#"{"changed":true}"#,
    )
    .unwrap();
    let changed = call(&h, "preview_packet", json!({"packet_id":p["packet_id"]}));
    assert_eq!(changed["stale"], true);
    assert!(
        changed["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["kind"] == "original_artifact" && check["changed"] == true)
    );
    assert_eq!(changed["attempts"][0]["status"], "reserved");
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        before
    );
}

#[test]
fn keeping_current_supersedes_stale_creation_success() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let run = start(&h, 1);
    let args = json!({"run_id":run,"record":"applications/new-engineer-999.md","company":"New","role":"Engineer","vacancy_url":"https://jobs.example.com/new/999","source":"employer","fit_reason":"Synthetic fit","evidence":["https://jobs.example.com/new/999"],"idempotency_key":"create"});
    call(&h, "create_application", args.clone());
    let path = dir.path().join("applications/new-engineer-999.md");
    let wanted = fs::read_to_string(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let mut state = h.load_state().unwrap();
    state["pending"] =
        json!([{"path":"applications/new-engineer-999.md","before_sha256":null,"text":wanted}]);
    storage::commit_state(h.root(), &state).unwrap();
    let pending = h.pending_transaction().unwrap();
    let resolved = h
        .resolve_pending(
            "current",
            &["https://example.com/reconciliation".into()],
            "resolve",
            pending["pending_sha256"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(
        resolved["unupdated_paths"],
        json!(["applications/new-engineer-999.md"])
    );
    assert_eq!(resolved["superseded_actions"], json!(["create"]));
    assert_eq!(
        h.call("create_application", &args)["error"]["code"],
        "recovery_conflict"
    );
    assert!(!path.exists());
}

#[test]
fn derived_view_failure_cannot_pin_a_completed_journal() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let record = dir.path().join("applications/acme-engineer-123.md");
    let mut state = h.load_state().unwrap();
    let text = "# Acme — Engineer\n\n<!-- tracking\n{\"stage\":\"saved\",\"checked\":\"2026-10-12\",\"next_action\":\"Review\"}\n-->\n";
    state["pending"] = json!([{"path":"applications/acme-engineer-123.md","before_sha256":domain::digest(&fs::read(&record).unwrap()),"text":text}]);
    storage::commit_state(h.root(), &state).unwrap();
    fs::create_dir_all(dir.path().join("applications/pipeline.md")).unwrap();
    let result = h.call("rebuild_pipeline", &json!({"idempotency_key":"rebuild"}));
    assert_eq!(result["ok"], false);
    assert!(h.load_state().unwrap().get("pending").is_none());
    assert_eq!(fs::read_to_string(record).unwrap(), text);
    assert_eq!(
        call(&h, "reconcile_tracker", json!({}))["pipeline_stale"],
        true
    );
}

#[test]
fn intake_proposals_are_separate_and_acceptance_is_explicit() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let cv_before = fs::read(dir.path().join("output/cv.pdf")).unwrap();
    let answers_before = fs::read(dir.path().join("applications/approved-answers.md")).unwrap();
    let args = json!({"text":"# User notes\nWe discussed database transactions.\n","source_kind":"personal_notes","source_label":"Synthetic meeting","idempotency_key":"note"});
    let note = call(&h, "intake_note", args.clone());
    assert_eq!(note, call(&h, "intake_note", args));
    let proposal = call(
        &h,
        "propose_note",
        json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[{"id":"summary1","kind":"summary","text":"Database transactions were discussed.","citations":[{"path":note["original_path"],"sha256":note["source_sha256"],"start_line":2,"end_line":2}]}],"questions":["Confirm the missing meeting date."],"idempotency_key":"proposal"}),
    );
    let review = call(
        &h,
        "review_note",
        json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"],"include_original":true}),
    );
    assert_eq!(review["source_fresh"], true);
    assert_eq!(review["accepted_reviews"].as_array().unwrap().len(), 0);
    assert_eq!(review["metadata"]["occurred_at"], Value::Null);
    assert_eq!(review["proposal"]["status"], "proposed");
    let accepted = call(
        &h,
        "accept_note",
        json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"],"expected_proposal_sha256":proposal["proposal_sha256"],"selected_item_ids":["summary1"],"approved_by":"Synthetic user","approval_evidence":"Explicit fixture instruction","idempotency_key":"accept"}),
    );
    assert_eq!(accepted["facts_changed"], false);
    assert_eq!(accepted["cv_changed"], false);
    assert_eq!(accepted["next_steps_executed"], false);
    assert!(
        dir.path()
            .join(accepted["accepted_path"].as_str().unwrap())
            .is_file()
    );
    assert_eq!(
        fs::read(dir.path().join("output/cv.pdf")).unwrap(),
        cv_before
    );
    assert_eq!(
        fs::read(dir.path().join("applications/approved-answers.md")).unwrap(),
        answers_before
    );
    let notes = call(&h, "list_notes", json!({}));
    assert_eq!(notes["page"]["total"], 1);
    let archived = call(
        &h,
        "archive_note",
        json!({"note_id":note["note_id"],"expected_metadata_sha256":review["metadata_sha256"],"reason":"Fixture reviewed","evidence":"Explicit fixture archive instruction","idempotency_key":"archive"}),
    );
    assert_eq!(archived["retained"], true);
    assert_eq!(call(&h, "list_notes", json!({}))["page"]["total"], 0);
    assert_eq!(
        call(&h, "list_notes", json!({"include_archived":true}))["page"]["total"],
        1
    );
    assert!(
        dir.path()
            .join(note["original_path"].as_str().unwrap())
            .is_file()
    );
}

#[test]
fn intake_batch_chains_exact_sources_without_automatic_acceptance() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let result = call(
        &h,
        "execute_batch",
        json!({"steps":[{"id":"note","tool":"intake_note","arguments":{"text":"Synthetic original note\n","source_kind":"transcript","idempotency_key":"note"}},{"id":"proposal","tool":"propose_note","arguments":{"note_id":{"$ref":"note.data.note_id"},"expected_source_sha256":{"$ref":"note.data.source_sha256"},"items":[{"id":"next1","kind":"next_step","text":"Prepare a review","citations":[{"path":{"$ref":"note.data.original_path"},"sha256":{"$ref":"note.data.source_sha256"},"start_line":1,"end_line":1}]}],"idempotency_key":"proposal"}},{"id":"review","tool":"review_note","arguments":{"note_id":{"$ref":"note.data.note_id"},"proposal_id":{"$ref":"proposal.data.proposal_id"}}}]}),
    );
    assert_eq!(result["completed"], 3);
    assert_eq!(
        result["results"][2]["result"]["data"]["accepted_reviews"],
        json!([])
    );
    assert!(h.load_state().unwrap().get("pending").is_none());
    let readonly=h.call("query_batch",&json!({"steps":[{"id":"note","tool":"intake_note","arguments":{"text":"No write","source_kind":"personal_notes","idempotency_key":"other"}}]}));
    assert_eq!(readonly["error"]["code"], "invalid_arguments");
}

#[test]
fn intake_stale_source_is_reported_and_cannot_be_accepted() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"Original\n","source_kind":"generated_summary","idempotency_key":"note"}),
    );
    let proposal = call(
        &h,
        "propose_note",
        json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[{"id":"summary1","kind":"summary","text":"Original","citations":[{"path":note["original_path"],"sha256":note["source_sha256"],"start_line":1,"end_line":1}]}],"idempotency_key":"proposal"}),
    );
    fs::write(
        dir.path().join(note["original_path"].as_str().unwrap()),
        "Manual source change\n",
    )
    .unwrap();
    let review = call(
        &h,
        "review_note",
        json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"]}),
    );
    assert_eq!(review["source_fresh"], false);
    assert!(!review["issues"].as_array().unwrap().is_empty());
    let result=h.call("accept_note",&json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"],"expected_proposal_sha256":proposal["proposal_sha256"],"selected_item_ids":["summary1"],"approved_by":"Fixture","approval_evidence":"Fixture instruction","idempotency_key":"accept"}));
    assert_eq!(result["error"]["code"], "source_changed");
}

#[test]
fn context_pack_preserves_unknown_time_and_exact_passage_anchors() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"# Interview note\nDatabase transactions came up.\nA follow-up is proposed.\n","source_kind":"transcript","occurred_period":"around Q2 2024","source_group":"Example project","idempotency_key":"note"}),
    );
    let pack = call(
        &h,
        "context_pack",
        json!({"task":"Prepare a screening brief","query":"database","paths":[note["original_path"]],"period":{"start":"2024-04-01","end":"2024-06-30","basis":"occurred"},"limit":2}),
    );
    assert_eq!(pack["authority"], "sourced_context_only");
    assert_eq!(pack["complete"], false);
    let source = &pack["sources"][0];
    assert_eq!(source["sha256"], note["source_sha256"]);
    assert_eq!(source["start_line"], 1);
    assert_eq!(source["provenance"]["occurred_period"], "around Q2 2024");
    assert_eq!(source["provenance"]["occurred_at"], Value::Null);
    assert_eq!(
        source["provenance"]["date_confidence"],
        "supplied_approximate"
    );
    assert!(
        pack["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["code"] == "period_membership_unknown")
    );
    assert!(
        source["excerpt"]
            .as_str()
            .unwrap()
            .contains("Database transactions")
    );
    assert_eq!(pack["read_refs"][0]["tool"], "read_record");
}

#[test]
fn context_pack_period_filter_never_uses_recorded_time_as_occurrence() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"Known date original\n","source_kind":"personal_notes","occurred_at":"2024-05-01","idempotency_key":"note"}),
    );
    let pack = call(
        &h,
        "context_pack",
        json!({"task":"Review recent evidence","paths":[note["original_path"]],"period":{"start":"2025-01-01","basis":"occurred"}}),
    );
    assert_eq!(pack["selected_count"], 0);
    assert!(
        pack["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["code"] == "no_sources")
    );
    let unfiltered = call(
        &h,
        "context_pack",
        json!({"task":"Review exact evidence","paths":[note["original_path"]]}),
    );
    assert_eq!(
        unfiltered["sources"][0]["provenance"]["date_confidence"],
        "supplied_exact"
    );
}

#[test]
fn context_pack_remains_bounded_and_reports_current_source_drift() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"Original\n","source_kind":"personal_notes","idempotency_key":"note"}),
    );
    let target = dir.path().join(note["original_path"].as_str().unwrap());
    fs::write(&target, "Changed source\n".repeat(1000)).unwrap();
    let before = fs::read(dir.path().join("applications/operations/state.json")).unwrap();
    let pack = call(
        &h,
        "context_pack",
        json!({"task":"Read changed notes","paths":[note["original_path"]],"limit":1}),
    );
    assert_eq!(pack["sources"][0]["stale"], true);
    assert_eq!(pack["sources"][0]["partial"], true);
    assert!(
        pack["sources"][0]["excerpt"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= 1400
    );
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        before
    );
    let batch = call(
        &h,
        "query_batch",
        json!({"steps":[{"id":"context","tool":"context_pack","arguments":{"task":"Prepare interview","company":"Acme"}}]}),
    );
    assert_eq!(batch["completed"], 1);
}

#[test]
fn workflow_check_batches_readback_and_coverage_without_actions() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let checked = call(
        &h,
        "workflow_check",
        json!({"checks":[{"id":"coverage","kind":"coverage","observation":{"source":"fixture-mail","observed_at":"2026-10-12","requested_period":"October","pages_read":1,"pagination_exhausted":false,"failed_reads":["message-2"],"exclusions":[],"checkpoint":"next-page"}},{"id":"form","kind":"form","observation":{"vacancy_url":"https://jobs.example.com/acme/123","page_revision":"old","readback_revision":"new","observed_at":"2026-10-12","fields":[],"uploads":[],"parsed_claim_changes":[]}}]}),
    );
    assert_eq!(checked["canonical_changes"], false);
    assert_eq!(checked["external_actions"], false);
    assert_eq!(checked["checks"][0]["data"]["complete"], false);
    assert_eq!(checked["checks"][1]["data"]["ready_for_preparation"], false);
    assert_eq!(checked["checks"][1]["data"]["submit_permitted"], false);
    let discovery = call(
        &h,
        "inspect_tools",
        json!({"names":["workflow_check"],"include_schemas":true}),
    );
    assert_eq!(discovery["tools"][0]["read_only"], true);
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
}

#[test]
fn explicit_project_seeds_context_and_linked_intake_even_when_query_has_no_hits() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join("context/projects")).unwrap();
    let project = "context/projects/migration.md";
    fs::write(
        dir.path().join(project),
        "# Migration\nTransactions protect imports.\n",
    )
    .unwrap();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"A collaborator discussed import retries.\n","source_kind":"personal_notes","links":[{"kind":"project","path":project}],"idempotency_key":"linked-note"}),
    );
    let before = fs::read(dir.path().join("applications/operations/state.json")).unwrap();
    let args = json!({"task":"Prepare project story","project":project,"query":"harbourdragon-no-match","limit":6});
    let pack = call(&h, "context_pack", args.clone());
    assert!(
        pack["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["path"] == project)
    );
    assert!(
        pack["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["path"] == note["original_path"])
    );
    assert_eq!(
        pack["pack_sha256"],
        call(&h, "context_pack", args.clone())["pack_sha256"]
    );
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        before
    );
    fs::write(
        dir.path().join(project),
        "# Migration\nChanged exact evidence.\n",
    )
    .unwrap();
    assert_ne!(
        pack["pack_sha256"],
        call(&h, "context_pack", args)["pack_sha256"]
    );
}
#[test]
fn review_queue_retains_uncertain_mail_and_only_unreviewed_note_proposals() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let note = call(
        &h,
        "intake_note",
        json!({"text":"Original fixture\n","source_kind":"personal_notes","idempotency_key":"queue-note"}),
    );
    let proposal = call(
        &h,
        "propose_note",
        json!({"note_id":note["note_id"],"expected_source_sha256":note["source_sha256"],"items":[{"id":"summary","kind":"summary","text":"Fixture summary","citations":[{"path":note["original_path"],"sha256":note["source_sha256"],"start_line":1,"end_line":1}]}],"idempotency_key":"queue-proposal"}),
    );
    fs::write(dir.path().join("context/mail-observation.json"), r#"{"items":[{"id":"uncertain","needs_reply":null},{"id":"ambiguous","needs_reply":false,"attribution":"ambiguous"},{"id":"resolved","needs_reply":false,"attribution":"proposed_exact_identity","needs_review":false}]}"#).unwrap();
    let args = json!({"mail_observations_path":"context/mail-observation.json","limit":50});
    let before = fs::read(dir.path().join("applications/operations/state.json")).unwrap();
    let queue = call(&h, "review_queue", args.clone());
    assert_eq!(queue["counts"]["supplied_mail"], 2);
    assert_eq!(queue["counts"]["pending_note_reviews"], 1);
    assert_eq!(queue["note_reviews_complete"], true);
    assert!(
        queue["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["proposal_id"] == proposal["proposal_id"])
    );
    assert_eq!(
        fs::read(dir.path().join("applications/operations/state.json")).unwrap(),
        before
    );
    call(
        &h,
        "accept_note",
        json!({"note_id":note["note_id"],"proposal_id":proposal["proposal_id"],"expected_proposal_sha256":proposal["proposal_sha256"],"selected_item_ids":["summary"],"approved_by":"Fixture user","approval_evidence":"Explicit synthetic approval","idempotency_key":"queue-accept"}),
    );
    let reviewed = call(&h, "review_queue", args);
    assert_eq!(reviewed["counts"]["pending_note_reviews"], 0);
}
#[test]
fn workflow_child_schemas_are_discoverable_and_validate_before_execution() {
    let dir = fixture();
    let h = Engine::open(dir.path()).unwrap();
    let schema = &contracts::specs()["workflow_check"]["inputSchema"];
    let variants = schema["properties"]["checks"]["items"]["anyOf"]
        .as_array()
        .unwrap();
    assert_eq!(variants.len(), 4);
    let mail = variants
        .iter()
        .find(|row| row["properties"]["kind"]["enum"][0] == "mail")
        .unwrap();
    assert!(
        mail["properties"]["observation"]["properties"]
            .get("request_message_ids")
            .is_some()
    );
    let bad = h.call(
        "workflow_check",
        &json!({"checks":[{"id":"bad","kind":"coverage","observation":{"source":"example"}}]}),
    );
    assert_eq!(bad["error"]["code"], "invalid_arguments");
    assert!(
        !dir.path()
            .join("applications/operations/state.json")
            .exists()
    );
}

#[test]
fn reviewed_tracking_move_refuses_manual_edit_and_keeps_canonical_state() {
    let directory = fixture();
    let engine = Engine::open(directory.path()).unwrap();
    let run = start(&engine, 1);
    let record = "applications/acme-engineer-123.md";
    let original = call(&engine, "read_record", json!({"path":record}));
    let path = directory.path().join(record);
    fs::write(
        &path,
        format!(
            "{}\nA newer manual note must survive.\n",
            original["text"].as_str().unwrap()
        ),
    )
    .unwrap();
    let before_record = fs::read(&path).unwrap();
    let before_state =
        fs::read(directory.path().join("applications/operations/state.json")).unwrap();
    let arguments = json!({"run_id":run,"record":record,"tracking":{"stage":"screening","checked":"2026-10-12","next_action":"Prepare confirmed screening"},"summary":"Explicit synthetic reviewed move","evidence":["https://example.test/reviewed-instruction"],"expected_record_sha256":original["sha256"],"idempotency_key":"reviewed-move"});
    let stale = engine.call("update_application", &arguments);
    assert_eq!(stale["error"]["code"], "stale_record");
    assert_eq!(fs::read(&path).unwrap(), before_record);
    assert_eq!(
        fs::read(directory.path().join("applications/operations/state.json")).unwrap(),
        before_state
    );
    let fresh = call(&engine, "read_record", json!({"path":record}));
    let mut current = arguments.clone();
    current["expected_record_sha256"] = fresh["sha256"].clone();
    assert_eq!(
        call(&engine, "update_application", current.clone())["tracking"]["stage"],
        "screening"
    );
    let committed = fs::read(&path).unwrap();
    call(&engine, "update_application", current);
    assert_eq!(fs::read(&path).unwrap(), committed);
    let mut invalid = arguments;
    invalid["expected_record_sha256"] = json!("g".repeat(64));
    invalid["idempotency_key"] = json!("invalid-hash");
    assert_eq!(
        engine.call("update_application", &invalid)["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(fs::read(&path).unwrap(), committed);
}
