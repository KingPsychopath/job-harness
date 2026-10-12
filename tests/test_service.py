"""Public API, batching, adapters and onboarding behavior; isolated fixtures only."""

import json
import subprocess
import sys
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import test_ledger as fixtures

from job_harness.contracts import SPECS
from job_harness.ledger import Ledger, validate
from job_harness.service import Harness
from job_harness.workspace import initialize, source_file

try:
    from jsonschema import Draft202012Validator
except ImportError:
    Draft202012Validator = None


class HarnessTests(unittest.TestCase):
    def setUp(self):
        fixtures.CareerTests.setUp(self)
        (self.root / "config/career-ledger.json").write_text(
            '{"schema_version":1,"timezone":"UTC"}'
        )

    tearDown = fixtures.CareerTests.tearDown
    call = fixtures.CareerTests.call
    run_apply = fixtures.CareerTests.run_apply
    packet = fixtures.CareerTests.packet
    reserve = fixtures.CareerTests.reserve

    def api(self, tool, **args):
        result = Harness(self.root).call(tool, args)
        validate(SPECS[tool]["outputSchema"], result, "output")
        if Draft202012Validator:
            Draft202012Validator(SPECS[tool]["outputSchema"]).validate(result)
        return result

    def test_all_schemas_are_json_schema_and_errors_are_typed(self):
        for tool, spec in SPECS.items():
            if Draft202012Validator:
                Draft202012Validator.check_schema(spec["inputSchema"])
                Draft202012Validator.check_schema(spec["outputSchema"])
            result = Harness(self.root).call(tool, {"unexpected_field": True})
            self.assertFalse(result["ok"], tool)
            self.assertEqual(result["error"]["code"], "invalid_arguments", tool)
            validate(spec["outputSchema"], result)
        self.assertEqual(Harness(self.root).call("not_a_tool", {})["error"]["code"], "unknown_tool")

    def test_deferred_discovery_and_focused_catalog(self):
        from job_harness.mcp import handle

        h = Harness(self.root)
        focused, _ = handle(h, {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}, True, "focused")
        full, _ = handle(h, {"jsonrpc": "2.0", "id": 2, "method": "tools/list"}, True, "full")
        self.assertLess(len(json.dumps(focused)), len(json.dumps(full)))
        self.assertNotIn("find_application", [t["name"] for t in focused["result"]["tools"]])
        selected = self.api("inspect_tools", names=["find_application"], include_schemas=True)
        self.assertEqual(
            selected["data"]["tools"][0]["inputSchema"], SPECS["find_application"]["inputSchema"]
        )
        self.assertNotIn("inputSchema", self.api("inspect_tools")["data"]["tools"][0])
        invalid, _ = handle(
            h,
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": []}},
            True,
            "focused",
        )
        self.assertTrue(invalid["result"]["isError"])

    def test_bootstrap_and_compact_paging(self):
        result = self.api("bootstrap", operation="apply", capabilities=["web.read"])
        self.assertTrue(result["ok"])
        self.assertIn("browser.submit", result["data"]["missing_capabilities"])
        self.assertEqual(result["data"]["health"]["records"], 1)
        self.assertEqual(len(self.api("find_application", stage="saved")["data"]["matches"]), 1)
        self.assertEqual(self.api("find_application", stage="closed")["data"]["page"]["total"], 0)
        for i in range(11):
            (self.root / f"applications/acme-engineer-{i + 200}.md").write_text(
                f"# Acme — Role {i}\n\n- Stage: saved\n"
            )
        a = self.api("find_application", query="Acme", limit=5)
        b = self.api(
            "find_application", query="Acme", offset=a["data"]["page"]["next_offset"], limit=5
        )
        self.assertEqual(a["data"]["page"]["total"], 12)
        self.assertEqual(len(a["data"]["matches"]), 5)
        self.assertNotEqual(a["data"]["matches"], b["data"]["matches"])
        self.assertTrue(
            all(x["record_path"].startswith("applications/") for x in a["data"]["matches"])
        )

    def test_batch_reference_partial_failure_and_restart(self):
        steps = [
            {
                "id": "start",
                "tool": "start_run",
                "arguments": {
                    "operation": "reconcile",
                    "scope": {},
                    "user_request": "fixture",
                    "idempotency_key": "batch:run",
                },
            },
            {
                "id": "inspect",
                "tool": "get_run",
                "arguments": {"run_id": {"$ref": "start.data.run_id"}},
            },
            {"id": "bad", "tool": "read_record", "arguments": {"path": "../private.md"}},
            {"id": "skipped", "tool": "find_application", "arguments": {"query": "Acme"}},
        ]
        r = self.api("execute_batch", steps=steps)
        self.assertFalse(r["ok"])
        self.assertEqual(r["data"]["completed"], 2)
        self.assertEqual(r["data"]["failed_step"], "bad")
        self.assertEqual(r["data"]["skipped"], ["skipped"])
        first = r["data"]["results"][0]["result"]["data"]["run_id"]
        again = Harness(self.root).call("execute_batch", {"steps": steps[:2]})
        self.assertTrue(again["ok"])
        self.assertEqual(again["data"]["results"][0]["result"]["data"]["run_id"], first)
        self.assertEqual(len(Ledger(self.root).load_state()["runs"]), 1)
        invalid = self.api("query_batch", steps=[steps[0]])
        self.assertFalse(invalid["ok"])
        boundary = self.api(
            "execute_batch",
            steps=[steps[0], {"id": "begin", "tool": "begin_submission", "arguments": {}}],
        )
        self.assertEqual(boundary["error"]["code"], "batch_boundary")
        ref = self.api(
            "query_batch",
            steps=[
                {
                    "id": "future",
                    "tool": "get_run",
                    "arguments": {"run_id": {"$ref": "later.data.run_id"}},
                }
            ],
        )
        self.assertEqual(ref["data"]["results"][0]["result"]["error"]["code"], "invalid_reference")

    def test_compound_prepare_and_one_shot_boundary(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url="https://jobs.example.com/acme/123",
            source="employer",
            posting_id="123",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="combined",
        )
        r = self.api("prepare_application", **args)
        self.assertTrue(r["ok"], r)
        repeated = self.api("prepare_application", **args)
        self.assertEqual(r, repeated)
        self.assertEqual(len(self.ledger.load_state()["attempts"]), 1)
        began = self.api(
            "begin_submission", attempt_id=r["data"]["attempt_id"], idempotency_key="begin:compound"
        )
        self.assertTrue(began["data"]["ready_for_single_submit"])
        replay = self.api(
            "begin_submission", attempt_id=r["data"]["attempt_id"], idempotency_key="begin:compound"
        )
        self.assertFalse(replay["data"]["ready_for_single_submit"])
        unknown = self.api(
            "finish_submission",
            attempt_id=r["data"]["attempt_id"],
            outcome="outcome_unknown",
            summary="fixture interruption",
            evidence=[],
            idempotency_key="unknown",
        )
        self.assertTrue(unknown["ok"])
        self.assertEqual(
            self.api("prepare_application", **{**args, "idempotency_key": "second"})["error"][
                "code"
            ],
            "duplicate_blocked",
        )
        self.assertNotIn("events", self.api("get_run", run_id=run)["data"])
        self.assertEqual(
            self.api("get_packet", packet_id=r["data"]["packet_id"])["data"]["packet"]["cv"][
                "sha256"
            ],
            r["data"]["packet"]["cv"]["sha256"],
        )

    def test_structured_vacancy_identity_handles_parentheses(self):
        url = "https://jobs.example.com/acme/job(uk),123"
        p = self.root / "applications/acme-engineer-123.md"
        p.write_text(
            "# Acme — Engineer\n\n<!-- tracking\n"
            + json.dumps(
                {
                    "stage": "saved",
                    "checked": "2026-10-11",
                    "next_action": "Review",
                    "company": "Acme",
                    "vacancy_url": url,
                }
            )
            + "\n-->\n"
        )
        r = self.api("check_duplicate", company="Acme", vacancy_url=url)
        self.assertEqual(len(r["data"]["exact_matches"]), 1)
        prepared = self.api(
            "prepare_packet",
            run_id=self.run_apply(),
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url=url,
            source="employer",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="parens",
        )
        self.assertTrue(prepared["ok"], prepared)

    def test_adapter_live_observation_and_recovery_stop(self):
        run = self.run_apply()
        self.call(
            "record_event",
            run_id=run,
            kind="reconciled",
            summary="[]",
            record="applications/acme-engineer-123.md",
            evidence=["https://example.com/evidence"],
            idempotency_key="non-object-summary",
        )
        args = {
            "run_id": run,
            "record_path": "applications/acme-engineer-123.md",
            "provider": "fixture",
            "target_url": "https://board.example.com/role/123",
            "capabilities": ["board.sync"],
        }
        plan = self.api("plan_adapter", **args)["data"]
        action = plan["actions"][0]
        completion = {k: args[k] for k in ["run_id", "record_path", "provider", "target_url"]}
        completion.update(
            plan_id=plan["plan_id"],
            outcome="verified",
            evidence=["https://board.example.com/role/123"],
            idempotency_key="sync:123",
        )
        wrong = self.api(
            "record_adapter_result",
            **completion,
            observed_stage="applied",
            observed_note=action["note"],
        )
        self.assertEqual(wrong["error"]["code"], "adapter_mismatch")

        observed = {"observed_stage": "saved", "observed_note": action["note"]}
        good = self.api("record_adapter_result", **completion, **observed)
        self.assertTrue(good["ok"], good)
        self.assertTrue(self.api("record_adapter_result", **completion, **observed)["ok"])
        for i in range(2):
            fail = self.api(
                "record_adapter_result",
                **{**completion, "outcome": "failed", "idempotency_key": f"failure:{i}"},
                failure_code="upload_failed",
            )
            self.assertTrue(fail["ok"])
        recovery = self.api("plan_adapter", **args, failure_code="upload_failed")["data"]
        self.assertTrue(recovery["stop_retrying"])
        self.assertEqual(recovery["recent_failure_count"], 2)
        state = self.ledger.load_state()
        state["pending"] = [{"path": "applications/acme-engineer-123.md"}]
        self.ledger.state_path.write_text(json.dumps(state))
        blocked = self.api("plan_adapter", **args)
        self.assertEqual(blocked["error"]["code"], "pending_recovery")

    def test_history_paging_does_not_truncate_at_old_default(self):
        run = self.run_apply()
        for i in range(105):
            self.call(
                "record_event",
                run_id=run,
                kind="reconciled",
                summary=f"fixture {i}",
                evidence=["https://example.com/evidence"],
                idempotency_key=f"event:{i}",
            )
        first = self.api("list_events", run_id=run)
        self.assertEqual(len(first["data"]["events"]), 10)
        self.assertEqual(first["data"]["page"]["next_offset"], 10)
        a = self.api("list_events", run_id=run, offset=100, limit=10)
        self.assertEqual(a["data"]["page"]["total"], 106)
        self.assertEqual(len(a["data"]["events"]), 6)
        self.assertIsNone(a["data"]["page"]["next_offset"])
        self.assertNotIn("result", a["data"]["events"][0])

    def test_adapter_runner_changes_only_differences_and_verifies(self):
        from job_harness.adapters import drive_board

        h = Harness(self.root)
        run = self.run_apply()
        args = {
            "run_id": run,
            "record_path": "applications/acme-engineer-123.md",
            "provider": "fixture",
            "target_url": "https://board.example.com/123",
            "capabilities": ["board.sync"],
        }

        class Driver:
            stage = "applied"

            def __init__(self):
                self.notes = []
                self.reads = 0
                self.writes = 0

            def read(self, url):
                self.reads += 1
                return {
                    "stage": self.stage,
                    "notes": self.notes[:],
                    "evidence": [url + "/observation/" + str(self.reads)],
                }

            def set_stage(self, url, stage):
                self.stage = stage
                self.writes += 1

            def add_note(self, url, note):
                self.notes.append(note)
                self.writes += 1

        missing = drive_board(h, Driver(), {**args, "capabilities": []})
        self.assertEqual(missing["error"]["code"], "adapter_not_executable")
        d = Driver()
        self.assertTrue(drive_board(h, d, args)["ok"])
        self.assertEqual((d.reads, d.writes), (2, 2))
        self.assertTrue(drive_board(h, d, args)["ok"])
        self.assertEqual((d.reads, d.writes), (3, 2))

        class BrokenDriver(Driver):
            def add_note(self, url, note):
                self.writes += 1

        failed = drive_board(h, BrokenDriver(), {**args, "provider": "broken-fixture"})
        self.assertEqual(failed["error"]["code"], "adapter_mismatch")

    def test_progressed_prepare_replay_returns_recovery_not_another_begin(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url="https://jobs.example.com/acme/123",
            source="employer",
            posting_id="123",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="compound",
        )
        a = self.api("prepare_application", **args)["data"]
        self.api("begin_submission", attempt_id=a["attempt_id"], idempotency_key="elsewhere:begin")
        replay = self.api("prepare_application", **args)
        self.assertEqual(replay["data"]["status"], "submitting")
        self.assertEqual(replay["next"][0]["tool"], "get_run")

    def test_total_attempt_budget_counts_cancelled_attempts(self):
        run = self.call(
            "start_run",
            operation="apply",
            scope={
                "sources": ["employer"],
                "max_submissions": 1,
                "max_attempts": 1,
                "dry_run": False,
            },
            user_request="fixture attempt cap",
            idempotency_key="limited:run",
        )["run_id"]
        packet = self.packet(run)
        a = self.reserve(run, packet)
        self.call(
            "cancel_reservation",
            attempt_id=a,
            summary="fixture never submitted",
            idempotency_key="cancel",
        )
        self.assertEqual(
            self.api("reserve_submission", run_id=run, packet_id=packet, idempotency_key="fresh")[
                "error"
            ]["code"],
            "cap_reached",
        )

    def test_generic_resources_are_package_local_and_symlinks_are_refused(self):
        guide = source_file("docs/career-operations.md")
        self.assertIn("job_harness/resources/guides", str(guide))
        with self.assertRaisesRegex(ValueError, "Unknown generic resource"):
            source_file(".scratch/private.json")
        temporary = self.root / "fake-toolkit"
        folder = temporary / "job_harness/resources/guides"
        folder.mkdir(parents=True)
        (folder / "career-operations.md").symlink_to(guide)
        with (
            patch("job_harness.workspace.ROOT", temporary.resolve()),
            self.assertRaisesRegex(ValueError, "symlinked"),
        ):
            source_file("docs/career-operations.md")

    def test_long_and_empty_sources_remain_complete(self):
        p = self.root / "context/long.md"
        for text in ["x" * 25000, ""]:
            p.write_text(text)
            result = self.api("read_record", path="context/long.md")
            self.assertTrue(result["ok"], result)
            self.assertEqual(result["data"]["text"], text)

    def test_shared_harness_serializes_reader_during_commit(self):
        h = Harness(self.root)
        run = self.run_apply()
        packet = self.packet(run)
        committing = threading.Event()
        release = threading.Event()
        reading = threading.Event()
        done = threading.Event()
        real_commit = h.ledger.commit
        results = []

        def paused_commit():
            committing.set()
            if not release.wait(3):
                raise RuntimeError("fixture release timed out")
            real_commit()

        def reserve():
            results.append(
                h.call(
                    "reserve_submission",
                    {"run_id": run, "packet_id": packet, "idempotency_key": "thread:reserve"},
                )
            )

        def read():
            reading.set()
            results.append(h.call("get_run", {"run_id": run}))
            done.set()

        with patch.object(h.ledger, "commit", paused_commit):
            writer = threading.Thread(target=reserve)
            writer.start()
            self.assertTrue(committing.wait(3))
            reader = threading.Thread(target=read)
            reader.start()
            try:
                self.assertTrue(reading.wait(3))
                self.assertFalse(done.wait(0.05))
            finally:
                release.set()
            writer.join(3)
            reader.join(3)
        self.assertTrue(all(r["ok"] for r in results), results)
        self.assertEqual(len(Ledger(self.root).load_state()["attempts"]), 1)

    def test_repair_keys_follow_record_content(self):
        h = Harness(self.root)
        first = h.call("bootstrap", {})["next"][0]
        self.assertTrue(h.call(first["tool"], first["arguments"])["ok"])
        p = self.root / "applications/acme-engineer-123.md"
        p.write_text(p.read_text().replace("Senior Engineer", "Another Engineer"))
        second = h.call("bootstrap", {})["next"][0]
        self.assertNotEqual(first["arguments"], second["arguments"])
        self.assertTrue(h.call(second["tool"], second["arguments"])["ok"])
        self.assertFalse(h.call("bootstrap", {})["data"]["health"]["pipeline_stale"])

    def test_adapter_batch_replay_after_run_finished(self):
        run = self.run_apply()
        args = {
            "run_id": run,
            "record_path": "applications/acme-engineer-123.md",
            "provider": "fixture",
            "target_url": "https://board.example.com/123",
            "capabilities": ["board.sync"],
        }
        plan = self.api("plan_adapter", **args)["data"]
        completion = {k: args[k] for k in ["run_id", "record_path", "provider", "target_url"]}
        completion.update(
            plan_id=plan["plan_id"],
            outcome="verified",
            observed_stage="saved",
            observed_note=plan["actions"][0]["note"],
            evidence=[args["target_url"]],
            idempotency_key="batch:adapter",
        )
        steps = [
            {"id": "sync", "tool": "record_adapter_result", "arguments": completion},
            {
                "id": "finish",
                "tool": "finish_run",
                "arguments": {
                    "run_id": run,
                    "status": "completed",
                    "summary": "fixture",
                    "evidence": [],
                    "idempotency_key": "batch:finish",
                },
            },
        ]
        self.assertTrue(self.api("execute_batch", steps=steps)["ok"])
        self.assertTrue(self.api("execute_batch", steps=steps)["ok"])

    def test_changed_packet_bytes_require_deliberate_new_preparation(self):
        run = self.run_apply()
        args = {
            "run_id": run,
            "record": "applications/acme-engineer-123.md",
            "company": "Acme",
            "vacancy_url": "https://jobs.example.com/acme/123",
            "source": "employer",
            "cv_path": "output/cv.pdf",
            "answers_path": "applications/answers.json",
            "idempotency_key": "source:prepare",
        }
        self.assertTrue(self.api("prepare_application", **args)["ok"])
        (self.root / "applications/answers.json").write_text('{"experience":7}')
        result = self.api("prepare_application", **args)
        self.assertEqual(result["error"]["code"], "artifact_changed")
        self.assertEqual(len(self.ledger.load_state()["attempts"]), 1)

    def test_unexpected_error_and_oversized_evidence_do_not_escape(self):
        h = Harness(self.root)
        with patch.object(h.ledger, "dispatch", side_effect=StopIteration):
            self.assertEqual(
                h.call("read_record", {"path": "AGENTS.md"})["error"]["code"],
                "internal_contract_error",
            )
        run = self.run_apply()
        result = self.api(
            "finish_run",
            run_id=run,
            status="completed",
            summary="fixture",
            evidence=["https://example.com"] * 1001,
            idempotency_key="too-large",
        )
        self.assertEqual(result["error"]["code"], "invalid_arguments")
        self.assertEqual(self.ledger.load_state()["runs"][run]["status"], "running")
        with patch("job_harness.service.sys.executable", ""):
            self.assertIsNone(self.api("bootstrap")["data"]["cli_fallback"])

    def test_quoted_tracking_markup_is_rejected_before_journaling(self):
        run = self.run_apply()
        p = self.root / "applications/acme-engineer-123.md"
        before = p.read_bytes()
        result = self.api(
            "update_application",
            run_id=run,
            record="applications/acme-engineer-123.md",
            tracking={"stage": "saved", "checked": "2026-10-11", "next_action": "Review"},
            summary="Quoted <!-- tracking\n{}\n-->",
            evidence=["https://example.com"],
            idempotency_key="injected",
        )
        self.assertEqual(result["error"]["code"], "invalid_arguments")
        self.assertEqual(p.read_bytes(), before)
        self.assertTrue(self.api("bootstrap")["ok"])

    def test_mcp_survives_deeply_nested_json(self):
        messages = [
            "[" * 2000 + "]" * 2000,
            json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {"name": "bootstrap", "arguments": {}},
                }
            ),
        ]
        result = subprocess.run(
            [sys.executable, "-m", "job_harness", "--root", str(self.root), "mcp"],
            cwd=Path(__file__).resolve().parents[1],
            input="\n".join(messages) + "\n",
            capture_output=True,
            text=True,
            check=True,
        )
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertIn(rows[0]["error"]["code"], [-32700, -32600])
        self.assertTrue(rows[2]["result"]["structuredContent"]["ok"])
        self.assertNotIn("Traceback", result.stderr)

    def test_repeated_repair_and_generator_change_replay_refreshes_views(self):
        h = Harness(self.root)
        path = self.root / "applications/acme-engineer-123.md"
        original = path.read_text()
        repairs = []
        for text in [
            original,
            original.replace("Senior Engineer", "Another Engineer"),
            original,
            original.replace("Senior Engineer", "Another Engineer"),
        ]:
            path.write_text(text)
            repair = h.call("bootstrap", {})["next"][0]
            repairs.append(repair)
            self.assertTrue(h.call(repair["tool"], repair["arguments"])["ok"])
            self.assertFalse(h.call("bootstrap", {})["data"]["health"]["pipeline_stale"])
        self.assertEqual(repairs[0], repairs[2])
        (self.root / "applications/pipeline.md").write_text("outdated generator output")
        self.assertTrue(h.call(repairs[-1]["tool"], repairs[-1]["arguments"])["ok"])
        self.assertFalse(h.call("bootstrap", {})["data"]["health"]["pipeline_stale"])

    def test_resolve_attempt_preserves_later_stage_and_begin_replay_status(self):
        run = self.run_apply()
        attempt = self.reserve(run, self.packet(run))
        began = self.api("begin_submission", attempt_id=attempt, idempotency_key="late:begin")
        self.assertTrue(began["ok"])
        self.api(
            "update_application",
            run_id=run,
            record="applications/acme-engineer-123.md",
            tracking={
                "stage": "closed",
                "checked": "2026-10-11",
                "next_action": "No further action",
                "status_detail": "Later employer rejection",
            },
            summary="Confirmed later response",
            evidence=["https://example.com/rejection"],
            idempotency_key="late:closed",
        )
        for outcome in ["outcome_unknown", "not_submitted"]:
            result = self.api(
                "finish_submission",
                attempt_id=attempt,
                outcome=outcome,
                summary="Resolved older attempt",
                evidence=["https://example.com/proof"],
                idempotency_key="late:" + outcome,
            )
            self.assertTrue(result["ok"], result)
            row = self.api("find_application", stage="closed")["data"]["matches"][0]
            self.assertEqual(row["status_detail"], "Later employer rejection")
            self.assertEqual(row["next_action"], "No further action")
        replay = self.api("begin_submission", attempt_id=attempt, idempotency_key="late:begin")
        self.assertEqual(replay["data"]["status"], "cancelled")
        self.assertFalse(replay["data"]["ready_for_single_submit"])

    def test_missing_record_resolution_is_typed_and_does_not_resolve(self):
        run = self.run_apply()
        attempt = self.reserve(run, self.packet(run))
        self.api("begin_submission", attempt_id=attempt, idempotency_key="missing:begin")
        (self.root / "applications/acme-engineer-123.md").write_text("Record title lost")
        result = self.api(
            "finish_submission",
            attempt_id=attempt,
            outcome="not_submitted",
            summary="Negative proof",
            evidence=["https://example.com/proof"],
            idempotency_key="missing:finish",
        )
        self.assertEqual(result["error"]["code"], "missing_resource")
        self.assertIn("acme-engineer-123.md", result["error"]["message"])
        self.assertEqual(self.ledger.load_state()["attempts"][attempt]["status"], "submitting")

    def test_maximum_finish_evidence_with_receipt_matches_schema(self):
        run = self.run_apply()
        attempt = self.reserve(run, self.packet(run))
        self.api("begin_submission", attempt_id=attempt, idempotency_key="max:begin")
        result = self.api(
            "finish_submission",
            attempt_id=attempt,
            outcome="submitted",
            summary="Verified",
            evidence=["https://example.com/proof"] * 1000,
            receipt_path="output/cv.pdf",
            idempotency_key="max:finish",
        )
        self.assertTrue(result["ok"], result)
        self.assertEqual(len(result["data"]["evidence"]), 1002)

    def test_invalid_url_updates_and_manual_tracking_are_actionable(self):
        run = self.run_apply()
        path = self.root / "applications/acme-engineer-123.md"
        before = path.read_bytes()
        result = self.api(
            "update_application",
            run_id=run,
            record="applications/acme-engineer-123.md",
            tracking={
                "stage": "saved",
                "checked": "2026-10-11",
                "next_action": "Review",
                "vacancy_url": "not a URL",
            },
            summary="fixture",
            evidence=["https://example.com/proof"],
            idempotency_key="url:invalid",
        )
        self.assertEqual(result["error"]["code"], "invalid_arguments")
        self.assertEqual(path.read_bytes(), before)
        for extra in [{"vacancy_url": "not a URL"}, {"company": None}, {"posting_id": 123}]:
            tracking = {"stage": "saved", "checked": "2026-10-11", "next_action": "Review", **extra}
            path.write_text(
                "# Acme — Senior Engineer\n\n<!-- tracking\n" + json.dumps(tracking) + "\n-->\n"
            )
            result = self.api(
                "check_duplicate", company="Acme", vacancy_url="https://jobs.example.com/acme/123"
            )
            self.assertEqual(result["error"]["code"], "invalid_state")
            self.assertIn(path.name, result["error"]["message"])

    def test_separate_harness_reads_wait_for_pending_commit(self):
        writer = Harness(self.root)
        reader = Harness(self.root)
        run = self.run_apply()
        pending, release, done = threading.Event(), threading.Event(), threading.Event()
        results = []
        original = writer.ledger.recover

        def paused_recover():
            if writer.ledger.state.get("pending"):
                pending.set()
                if not release.wait(3):
                    raise RuntimeError("fixture timeout")
            original()

        def write():
            results.append(
                writer.call(
                    "update_application",
                    {
                        "run_id": run,
                        "record": "applications/acme-engineer-123.md",
                        "tracking": {
                            "stage": "prepared",
                            "checked": "2026-10-11",
                            "next_action": "Review",
                        },
                        "summary": "fixture",
                        "evidence": ["https://example.com/proof"],
                        "idempotency_key": "concurrent:write",
                    },
                )
            )

        def read():
            results.append(reader.call("find_application", {"stage": "prepared"}))
            done.set()

        with patch.object(writer.ledger, "recover", paused_recover):
            w = threading.Thread(target=write)
            w.start()
            self.assertTrue(pending.wait(3))
            r = threading.Thread(target=read)
            r.start()
            try:
                self.assertFalse(done.wait(0.05))
            finally:
                release.set()
            w.join(3)
            r.join(3)
        self.assertTrue(all(item["ok"] for item in results), results)
        self.assertEqual(results[1]["data"]["matches"][0]["stage"], "prepared")

    def test_canonical_answer_change_requires_new_preparation_key(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url="https://jobs.example.com/acme/123",
            source="employer",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="canonical:prepare",
        )
        self.assertTrue(self.api("prepare_application", **args)["ok"])
        (self.root / "applications/approved-answers.md").write_text("# Changed canonical answer")
        self.assertEqual(
            self.api("prepare_application", **args)["error"]["code"], "artifact_changed"
        )

    def test_deferred_tool_error_guides_to_schema_and_batch(self):
        from job_harness.mcp import handle

        response, _ = handle(
            Harness(self.root),
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": "find_application", "arguments": {}},
            },
            True,
            "focused",
        )
        result = response["result"]["structuredContent"]
        self.assertEqual(result["error"]["code"], "deferred_tool")
        self.assertEqual(result["next"][0]["arguments"]["names"], ["find_application"])

    def test_cli_and_mcp_survive_surrogate_strings(self):
        messages = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize"},
            {"jsonrpc": "2.0", "id": "\ud800", "method": "ping"},
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "bootstrap", "arguments": {}},
            },
        ]
        base = [sys.executable, "-m", "job_harness", "--root", str(self.root)]
        process = subprocess.run(
            base + ["mcp"],
            input="\n".join(json.dumps(m) for m in messages) + "\n",
            capture_output=True,
            text=True,
            check=True,
        )
        rows = [json.loads(line) for line in process.stdout.splitlines()]
        self.assertEqual(rows[1]["id"], "\ud800")
        self.assertTrue(rows[2]["result"]["structuredContent"]["ok"])
        process = subprocess.run(
            base + ["call", "bootstrap"],
            input=json.dumps({"capabilities": ["\ud800"]}),
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(json.loads(process.stdout)["data"]["available_capabilities"], ["\ud800"])

    def test_uninitialized_root_is_rejected_without_creating_state(self):
        wrong = self.root / "typo-ledger"
        result = Harness(wrong).call(
            "start_run",
            {
                "operation": "reconcile",
                "scope": {},
                "user_request": "fixture",
                "idempotency_key": "typo",
            },
        )
        self.assertEqual(result["error"]["code"], "missing_resource")
        self.assertIn("init", result["error"]["message"])
        self.assertFalse(wrong.exists())

    def test_structured_records_override_legacy_filename_filters(self):
        run = self.run_apply()
        for company, slug in [
            ("1910 Genetics", "1910-genetics"),
            ("Search Lab", "search-lab"),
            ("Email Labs", "email-labs"),
        ]:
            path = "applications/" + slug + "-engineer.md"
            url = "https://jobs.example.com/" + slug
            result = self.api(
                "create_application",
                run_id=run,
                record=path,
                company=company,
                role="Engineer",
                vacancy_url=url,
                source="employer",
                fit_reason="Fixture",
                evidence=[url],
                idempotency_key=slug + ":create",
            )
            self.assertTrue(result["ok"], result)
            self.assertTrue(
                self.api(
                    "update_application",
                    run_id=run,
                    record=path,
                    tracking={
                        "stage": "applied",
                        "checked": "2026-10-11",
                        "next_action": "Monitor",
                    },
                    summary="Verified application",
                    evidence=[url],
                    idempotency_key=slug + ":applied",
                )["ok"]
            )
            self.assertTrue(
                self.api("check_duplicate", company=company, vacancy_url=url)["data"]["blocked"]
            )
        path = self.root / "applications/1910-genetics-engineer.md"
        path.write_text(path.read_text().replace("# 1910 Genetics", "## 1910 Genetics"))
        result = self.api(
            "check_duplicate",
            company="1910 Genetics",
            vacancy_url="https://jobs.example.com/1910-genetics",
        )
        self.assertEqual(result["error"]["code"], "invalid_state")
        self.assertIn(path.name, result["error"]["message"])
        self.assertIn("title", result["error"]["message"])

    def test_conflicting_pending_update_remains_readable_with_ready_repair_hint(self):
        from job_harness.ledger import atomic_write, digest, encoded

        path = self.root / "applications/acme-engineer-123.md"
        before = path.read_bytes()
        state = self.ledger.load_state()
        state["pending"] = [
            {
                "path": "applications/acme-engineer-123.md",
                "before_sha256": digest(before),
                "text": before.decode() + "\nPlanned change\n",
            }
        ]
        atomic_write(self.ledger.state_path, encoded(state))
        path.write_text(before.decode() + "\nManual conflicting change\n")
        h = Harness(self.root)
        result = h.call("bootstrap", {})
        self.assertEqual(result["error"]["code"], "pending_recovery")
        hint = result["next"][0]
        self.assertEqual(
            h.call(hint["tool"], hint["arguments"])["error"]["code"], "recovery_conflict"
        )
        self.assertIn(
            "Manual conflicting",
            h.call("read_record", {"path": "applications/acme-engineer-123.md"})["data"]["text"],
        )
        saved = json.loads(
            h.call("read_record", {"path": "applications/operations/state.json"})["data"]["text"]
        )
        self.assertEqual(saved["pending"], state["pending"])
        self.assertIn("Manual conflicting", path.read_text())

    def test_bad_dates_and_host_urls_are_argument_errors_without_mutation(self):
        run = self.run_apply()
        path = self.root / "applications/acme-engineer-123.md"
        before = path.read_bytes()
        for key, tracking in [
            ("checked", {"stage": "saved", "checked": "11 Oct 2026", "next_action": "Review"}),
            (
                "scheduled_start",
                {
                    "stage": "saved",
                    "checked": "2026-10-11",
                    "next_action": "Review",
                    "scheduled_start": "Thursday morning",
                    "schedule_state": "booked",
                },
            ),
        ]:
            result = self.api(
                "update_application",
                run_id=run,
                record="applications/acme-engineer-123.md",
                tracking=tracking,
                summary="fixture",
                evidence=["https://example.com"],
                idempotency_key="bad:" + key,
            )
            self.assertEqual(result["error"]["code"], "invalid_arguments")
            self.assertIn(key, result["error"]["message"])
            self.assertEqual(path.read_bytes(), before)
        for url in ["http://[x", "https://example.com:abc"]:
            result = self.api("check_duplicate", company="Acme", vacancy_url=url)
            self.assertEqual(result["error"]["code"], "invalid_arguments")
            self.assertIn("URL", result["error"]["message"])

    def test_explicit_legacy_roots_use_init_guard(self):
        root = self.root / "mistyped-legacy-root"
        request = [
            json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {"name": "bootstrap", "arguments": {}},
                }
            ),
        ]
        toolkit = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            [sys.executable, str(toolkit / "scripts/career_mcp.py"), "--root", str(root)],
            input="\n".join(request) + "\n",
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(
            json.loads(result.stdout.splitlines()[-1])["result"]["structuredContent"]["error"][
                "code"
            ],
            "missing_resource",
        )
        result = subprocess.run(
            [
                sys.executable,
                str(toolkit / "scripts/career_ops.py"),
                "--root",
                str(root),
                "start_run",
            ],
            input=json.dumps(
                {
                    "operation": "reconcile",
                    "scope": {},
                    "user_request": "fixture",
                    "idempotency_key": "typo",
                }
            ),
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["error"]["code"], "missing_resource")
        self.assertFalse(root.exists())

    def test_custom_tracking_fields_survive_and_file_identity_wins(self):
        run = self.run_apply()
        path = self.root / "applications/acme-engineer-123.md"
        tracking = {
            "stage": "saved",
            "checked": "2026-10-11",
            "next_action": "Review",
            "custom_note": "Keep this",
            "record": "wrong.md",
            "title": "Wrong title",
        }
        path.write_text(
            "# Acme — Senior Engineer\n\n<!-- tracking\n" + json.dumps(tracking) + "\n-->\n"
        )
        result = self.api(
            "update_application",
            run_id=run,
            record="applications/acme-engineer-123.md",
            tracking={"next_action": "Prepare"},
            summary="Fixture",
            evidence=["https://example.com"],
            idempotency_key="custom",
        )
        self.assertTrue(result["ok"], result)
        self.assertIn('"custom_note": "Keep this"', path.read_text())
        row = self.api("find_application", query="Acme")["data"]["matches"][0]
        self.assertEqual(row["record_path"], "applications/acme-engineer-123.md")
        self.assertEqual(row["title"], "Acme — Senior Engineer")

    def test_cancelled_prepare_suggests_a_new_key_without_bypassing_caps(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url="https://jobs.example.com/acme/123",
            source="employer",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="cancelled:prepare",
        )
        initial = self.api("prepare_application", **args)
        self.assertTrue(
            self.api(
                "cancel_reservation",
                attempt_id=initial["data"]["attempt_id"],
                summary="No external action started",
                idempotency_key="cancelled:cancel",
            )["ok"]
        )
        replay = self.api("prepare_application", **args)
        hint = replay["next"][0]
        self.assertEqual(hint["tool"], "prepare_application")
        self.assertIn("otherwise stop", hint["reason"])
        self.assertNotEqual(hint["arguments"]["idempotency_key"], args["idempotency_key"])
        self.assertEqual(hint, self.api("prepare_application", **args)["next"][0])
        result = Harness(self.root).call(hint["tool"], hint["arguments"])
        self.assertTrue(result["ok"], result)
        self.assertNotEqual(result["data"]["attempt_id"], initial["data"]["attempt_id"])
        self.assertEqual(len(self.ledger.load_state()["attempts"]), 2)

    def test_non_ascii_tracking_is_readable_and_reserved_files_stay_reserved(self):
        run = self.run_apply()
        result = self.api(
            "create_application",
            run_id=run,
            record="applications/cafe-engineer.md",
            company="Café",
            role="Engineer",
            vacancy_url="https://jobs.example.com/cafe",
            source="employer",
            fit_reason="Fixture",
            evidence=["https://example.com"],
            idempotency_key="unicode:create",
        )
        self.assertTrue(result["ok"], result)
        text = (self.root / "applications/cafe-engineer.md").read_text()
        self.assertIn('"company": "Café"', text)
        tracking = {"stage": "saved", "checked": "2026-10-11", "next_action": "Review"}
        (self.root / "applications/template.md").write_text(
            "# Template\n\n<!-- tracking\n" + json.dumps(tracking) + "\n-->\n"
        )
        result = self.api("find_application", query="Café")
        self.assertEqual(result["error"]["code"], "invalid_state")
        self.assertIn("template.md", result["error"]["message"])

    def test_damaged_tracking_markers_do_not_hide_numeric_employers(self):
        path = self.root / "applications/1910-genetics-engineer.md"
        tracking = {
            "stage": "applied",
            "checked": "2026-10-12",
            "next_action": "Monitor",
            "company": "1910 Genetics",
            "vacancy_url": "https://jobs.example.com/1910",
        }
        original = (
            "# 1910 Genetics — Engineer\n\n<!-- tracking\n" + json.dumps(tracking) + "\n-->\n"
        )
        for text in [
            original.replace("<!-- tracking", "<!--tracking"),
            original.replace("\n-->", "-->"),
            original.replace("<!-- tracking", "<!-- Tracking"),
        ]:
            path.write_text(text)
            result = self.api(
                "check_duplicate",
                company="1910 Genetics",
                vacancy_url="https://jobs.example.com/1910",
            )
            self.assertEqual(result["error"]["code"], "invalid_state")
            self.assertIn("malformed tracking", result["error"]["message"])
            self.assertIn(path.name, result["error"]["message"])
        path.write_text(original)
        (self.root / "applications/README.md").write_text(
            "# Guide\n\n<!-- tracking example\n{}\n-->\n"
        )
        result = self.api(
            "check_duplicate", company="1910 Genetics", vacancy_url="https://jobs.example.com/1910"
        )
        self.assertTrue(result["data"]["blocked"])

    def test_batch_forwards_pending_recovery_and_allows_diagnostic_reads(self):
        from job_harness.ledger import atomic_write, digest, encoded

        path = self.root / "applications/acme-engineer-123.md"
        state = self.ledger.load_state()
        state["pending"] = [
            {
                "path": "applications/acme-engineer-123.md",
                "before_sha256": digest(path.read_bytes()),
                "text": path.read_text() + "\nPlanned change\n",
            }
        ]
        atomic_write(self.ledger.state_path, encoded(state))
        result = self.api(
            "query_batch", steps=[{"id": "check", "tool": "reconcile_tracker", "arguments": {}}]
        )
        self.assertEqual(result["error"]["code"], "batch_failed")
        self.assertEqual(result["next"], result["data"]["results"][0]["result"]["next"])
        self.assertEqual(result["next"][0]["tool"], "rebuild_pipeline")
        self.assertTrue(
            self.api(
                "query_batch",
                steps=[
                    {
                        "id": "current",
                        "tool": "read_record",
                        "arguments": {"path": "applications/acme-engineer-123.md"},
                    },
                    {
                        "id": "pending",
                        "tool": "read_record",
                        "arguments": {"path": "applications/operations/state.json"},
                    },
                ],
            )["ok"]
        )
        self.assertIn("pending", self.ledger.load_state())
        hint = result["next"][0]
        self.assertTrue(Harness(self.root).call(hint["tool"], hint["arguments"])["ok"])
        self.assertNotIn("pending", self.ledger.load_state())

    def test_private_onboarding_guides_never_fall_back_to_original_candidate(self):
        root = self.root / "another-candidate"
        initialize(root)
        h = Harness(root)
        for name in [
            "docs/project.md",
            "docs/workflow.md",
            "docs/application-pipeline.md",
            "applications/briefs/template.md",
        ]:
            result = h.call("read_record", {"path": name})
            self.assertTrue(result["ok"], result)
            self.assertEqual(result["data"]["origin"], "ledger")
            (root / name).unlink()
            self.assertEqual(
                h.call("read_record", {"path": name})["error"]["code"], "missing_resource"
            )

    def test_setup_failure_leaves_no_partial_destination(self):
        ledger = self.root / "failed-init"
        with (
            patch.object(Ledger, "write_pipeline", side_effect=OSError),
            self.assertRaises(OSError),
        ):
            initialize(ledger)
        self.assertFalse(ledger.exists())
        self.assertFalse(list(self.root.glob(".job-harness-*")))
        empty = self.root / "empty-ledger"
        empty.mkdir()
        initialize(empty)
        self.assertTrue((empty / "config/career-ledger.json").exists())


if __name__ == "__main__":
    unittest.main()
