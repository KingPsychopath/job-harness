"""Behavioral tests in disposable ledgers; no real mail, browser or application changes."""

import json
import multiprocessing
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from career_ops import ROOT, CareerError, Ledger, atomic_write
from install_career_tools import install


def contender(root, run_id, packet_id, gate, queue):
    gate.wait()
    try:
        result = Ledger(root).dispatch(
            "reserve_submission",
            {"run_id": run_id, "packet_id": packet_id, "idempotency_key": run_id + ":reserve"},
        )
        queue.put(("reserved", result["attempt"]["id"]))
    except CareerError as error:
        queue.put(("blocked", str(error)))


class CareerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        for name in ["applications", "context/profile", "output", "config"]:
            (self.root / name).mkdir(parents=True)
        shutil.copy(
            ROOT / "job_harness/defaults/career-workflows.json",
            self.root / "config/career-workflows.json",
        )
        (self.root / "applications/acme-engineer-123.md").write_text(
            "# Acme — Senior Engineer\n\n- Stage: saved\n- Source URL: https://jobs.example.com/acme/123?utm_source=old\n"
        )
        (self.root / "applications/approved-answers.md").write_text(
            "# Answers\n\n## Salary\nMinimum £80k; target £100k.\n"
        )
        (self.root / "context/profile/application-decisions.md").write_text(
            "# Decisions\n\n## Salary\nUser-confirmed; preserve base versus total.\n"
        )
        (self.root / "output/cv.pdf").write_bytes(b"%PDF-1.7\nfixture\n")
        (self.root / "applications/answers.json").write_text('{"experience":6}')
        self.ledger = Ledger(self.root)
        self.serial = 0

    def tearDown(self):
        self.temp.cleanup()

    def call(self, name, **args):
        return self.ledger.dispatch(name, args)

    def run_apply(self, dry=False, cap=1):
        self.serial += 1
        return self.call(
            "start_run",
            operation="apply",
            scope={"sources": ["employer"], "max_submissions": cap, "dry_run": dry},
            user_request="Apply to this fixture role",
            idempotency_key=f"run:{self.serial}",
        )["run"]["id"]

    def packet(self, run):
        return self.call(
            "prepare_packet",
            run_id=run,
            record="applications/acme-engineer-123.md",
            company="Acme",
            vacancy_url="https://jobs.example.com/acme/123",
            source="employer",
            posting_id="123",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key=run + ":packet",
        )["packet_id"]

    def reserve(self, run, packet):
        return self.call(
            "reserve_submission", run_id=run, packet_id=packet, idempotency_key=run + ":reserve"
        )["attempt"]["id"]

    def test_read_scope_and_retrieval(self):
        self.assertEqual(len(self.call("get_approved_answer", query="salary")["matches"]), 2)
        self.assertEqual(len(self.call("find_application", query="Acme")["matches"]), 1)
        for path in [".", "../secrets.md", "/etc/passwd", ".git/config", "scripts/career_ops.py"]:
            with self.assertRaises(CareerError):
                self.call("read_record", path=path)
        (self.root / "context/escape.md").symlink_to("/etc/passwd")
        with self.assertRaises(CareerError):
            self.call("read_record", path="context/escape.md")
        (self.root / "applications/escape-role.md").symlink_to("/etc/passwd")
        with self.assertRaisesRegex(ValueError, "escapes the ledger"):
            self.call("find_application", query="Acme")
        (self.root / "applications/escape-role.md").unlink()
        (self.root / "applications/approved-answers.md").unlink()
        (self.root / "applications/approved-answers.md").symlink_to("/etc/passwd")
        with self.assertRaises(CareerError):
            self.call("get_approved_answer", query="salary")

    def test_dry_run_and_source_scope(self):
        run = self.run_apply(dry=True)
        packet = self.packet(run)
        with self.assertRaisesRegex(CareerError, "Dry runs"):
            self.reserve(run, packet)
        with self.assertRaisesRegex(CareerError, "source outside"):
            self.call(
                "prepare_packet",
                run_id=run,
                record="applications/acme-engineer-123.md",
                company="Acme",
                vacancy_url="https://jobs.example.com/acme/123",
                source="unrequested",
                cv_path="output/cv.pdf",
                answers_path="applications/answers.json",
                idempotency_key="outside",
            )

    def test_duplicate_url_and_unknown_outcome(self):
        run = self.run_apply()
        packet = self.packet(run)
        attempt = self.reserve(run, packet)
        first = self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        self.assertTrue(first["ready_for_single_submit"])
        replay = self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        self.assertFalse(replay["ready_for_single_submit"])
        with self.assertRaises(CareerError):
            self.call("begin_submission", attempt_id=attempt, idempotency_key="second-begin")
        self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="outcome_unknown",
            summary="Browser interrupted after Submit",
            evidence=[],
            idempotency_key="uncertain",
        )
        other = Ledger(self.root)
        result = other.dispatch(
            "check_duplicate",
            {"company": "Acme", "vacancy_url": "https://jobs.example.com/acme/123/?utm_source=new"},
        )
        self.assertTrue(result["blocked"])
        with self.assertRaises(CareerError):
            self.call(
                "cancel_reservation",
                attempt_id=attempt,
                summary="Timeout is not evidence",
                idempotency_key="cancel",
            )
        with self.assertRaises(CareerError):
            self.call(
                "finish_run",
                run_id=run,
                status="completed",
                summary="All done",
                evidence=[],
                idempotency_key="finish",
            )

    def test_confirmed_packet_receipt_and_repeat_run(self):
        run = self.run_apply()
        packet = self.packet(run)
        attempt = self.reserve(run, packet)
        self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        (self.root / "applications/receipt.txt").write_text("ATS confirmed receipt 321")
        result = self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="submitted",
            summary="ATS receipt 321 confirmed",
            evidence=["https://jobs.example.com/receipt/321"],
            receipt_path="applications/receipt.txt",
            idempotency_key="submitted",
        )
        self.assertEqual(result["status"], "submitted")
        self.assertTrue((self.root / result["evidence"][-1]).exists())
        snapshot = json.loads(
            (self.root / "applications/operations/packets" / (packet + ".json")).read_text()
        )
        self.assertIn("ledger_dirty", snapshot)
        self.assertIn("applications/approved-answers.md", snapshot["answer_source_sha256"])
        self.assertIn("Stage: saved", (self.root / snapshot["source_record"]["path"]).read_text())
        self.assertEqual(
            self.call("find_application", query="Acme")["matches"][0]["stage"], "applied"
        )
        repeated = self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="submitted",
            summary="ATS receipt 321 confirmed",
            evidence=["https://jobs.example.com/receipt/321"],
            receipt_path="applications/receipt.txt",
            idempotency_key="submitted",
        )
        self.assertEqual(result, repeated)
        with self.assertRaises(CareerError):
            self.packet(self.run_apply())
        self.call(
            "finish_run",
            run_id=run,
            status="completed",
            summary="One verified submission",
            evidence=["applications/receipt.txt"],
            idempotency_key="finish",
        )

    def test_packet_tamper_and_identity_mismatch(self):
        run = self.run_apply()
        packet = self.packet(run)
        attempt = self.reserve(run, packet)
        data = json.loads(
            (self.root / "applications/operations/packets" / (packet + ".json")).read_text()
        )
        (self.root / data["cv"]["path"]).write_bytes(b"changed")
        with self.assertRaisesRegex(CareerError, "changed"):
            self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        with self.assertRaisesRegex(CareerError, "identity"):
            self.call(
                "prepare_packet",
                run_id=run,
                record="applications/acme-engineer-123.md",
                company="Acme",
                vacancy_url="https://jobs.example.com/other/456",
                source="employer",
                cv_path="output/cv.pdf",
                answers_path="applications/answers.json",
                idempotency_key="wrong-job",
            )

    def test_atomic_reservation_between_processes(self):
        run1, run2 = self.run_apply(), self.run_apply()
        packet1, packet2 = self.packet(run1), self.packet(run2)
        ctx = multiprocessing.get_context("spawn")
        gate, queue = ctx.Barrier(2), ctx.Queue()
        jobs = [
            ctx.Process(target=contender, args=(self.root, run, packet, gate, queue))
            for run, packet in [(run1, packet1), (run2, packet2)]
        ]
        for job in jobs:
            job.start()
        results = [queue.get(timeout=10)[0] for _ in jobs]
        for job in jobs:
            job.join(timeout=10)
            self.assertEqual(job.exitcode, 0)
        self.assertEqual(sorted(results), ["blocked", "reserved"])

    def test_event_retry_and_crash_recovery(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            kind="mail_reviewed",
            summary="Interview invitation",
            evidence=["https://mail.google.com/mail/u/0/#all/example"],
            idempotency_key="gmail:message:review",
            tracking={
                "stage": "screening",
                "checked": "2026-10-11",
                "next_action": "Book a screening",
            },
        )
        original = atomic_write

        def crash(path, data):
            if path.name == "acme-engineer-123.md":
                raise OSError("simulated crash")
            original(path, data)

        with patch("career_ops.atomic_write", crash), self.assertRaises(OSError):
            self.call("record_event", **args)
        self.assertIn("pending", self.ledger.load_state())
        retry = Ledger(self.root).dispatch("record_event", args)
        self.assertEqual(retry["tracking"]["stage"], "screening")
        self.assertNotIn("pending", self.ledger.load_state())
        text = (self.root / "applications/acme-engineer-123.md").read_text()
        self.assertEqual(text.count("Interview invitation"), 1)
        with self.assertRaisesRegex(CareerError, "Idempotency"):
            self.call("record_event", **{**args, "summary": "Different conclusion"})
        with self.assertRaisesRegex(CareerError, "regress"):
            self.call(
                "record_event",
                **{
                    **args,
                    "idempotency_key": "older-ack",
                    "tracking": {
                        "stage": "applied",
                        "checked": "2026-10-11",
                        "next_action": "Wait",
                    },
                },
            )

    def test_run_cap_after_confirmed_submission(self):
        run = self.run_apply(cap=1)
        packet = self.packet(run)
        attempt = self.reserve(run, packet)
        self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="submitted",
            summary="Confirmed",
            evidence=["https://jobs.example.com/receipt/321"],
            idempotency_key="done",
        )
        self.call(
            "create_application",
            run_id=run,
            record="applications/bravo-engineer-456.md",
            company="Bravo",
            role="Engineer",
            vacancy_url="https://jobs.example.com/bravo/456",
            source="employer",
            fit_reason="Fixture",
            evidence=["https://jobs.example.com/bravo/456"],
            idempotency_key="discover",
        )
        packet2 = self.call(
            "prepare_packet",
            run_id=run,
            record="applications/bravo-engineer-456.md",
            company="Bravo",
            vacancy_url="https://jobs.example.com/bravo/456",
            source="employer",
            cv_path="output/cv.pdf",
            answers_path="applications/answers.json",
            idempotency_key="packet2",
        )["packet_id"]
        with self.assertRaisesRegex(CareerError, "cap reached"):
            self.call(
                "reserve_submission",
                run_id=run,
                packet_id=packet2,
                idempotency_key="reserve-second",
            )

    def test_cross_run_scope_and_cancelled_reservation(self):
        run = self.run_apply()
        packet = self.packet(run)
        narrow = self.call(
            "start_run",
            operation="apply",
            scope={
                "sources": ["employer"],
                "max_submissions": 1,
                "company": "Other",
                "dry_run": False,
            },
            user_request="Apply only to Other",
            idempotency_key="narrow",
        )["run_id"]
        with self.assertRaisesRegex(CareerError, "company outside"):
            self.reserve(narrow, packet)
        attempt = self.reserve(run, packet)
        self.call(
            "cancel_reservation",
            attempt_id=attempt,
            summary="Form not submitted; user changed scope",
            idempotency_key="cancel",
        )
        self.assertFalse(
            self.call(
                "check_duplicate", company="Acme", vacancy_url="https://jobs.example.com/acme/123"
            )["blocked"]
        )
        fresh = self.call(
            "reserve_submission", run_id=run, packet_id=packet, idempotency_key="reserve-new"
        )["attempt"]["id"]
        self.assertNotEqual(attempt, fresh)

    def test_booking_completion_preserves_history(self):
        run = self.run_apply()
        args = {
            "run_id": run,
            "record": "applications/acme-engineer-123.md",
            "evidence": ["https://jobs.example.com/meeting/123"],
        }
        self.call(
            "update_application",
            **args,
            tracking={
                "stage": "screening",
                "checked": "2026-10-11",
                "next_action": "Attend screening",
                "schedule_state": "booked",
                "scheduled_start": "2026-10-12T11:00:00+01:00",
            },
            summary="Booking confirmed by employer",
            idempotency_key="booking",
        )
        result = self.call(
            "update_application",
            **args,
            tracking={
                "schedule_state": "completed",
                "checked": "2026-10-12",
                "next_action": "Await feedback",
            },
            summary="Owen confirms screening completed",
            idempotency_key="completed",
        )
        self.assertNotIn("scheduled_start", result["tracking"])
        self.assertIn(
            "Previous booked time: 2026-10-12T11:00:00+01:00",
            (self.root / args["record"]).read_text(),
        )
        self.assertFalse(self.call("reconcile_tracker")["pipeline_stale"])

    def test_manual_edit_conflict_never_overwritten(self):
        run = self.run_apply()
        args = dict(
            run_id=run,
            record="applications/acme-engineer-123.md",
            kind="mail_reviewed",
            summary="Invitation",
            evidence=["https://mail.google.com/mail/u/0/#all/example"],
            idempotency_key="mail:conflict",
            tracking={"stage": "screening", "checked": "2026-10-11", "next_action": "Choose time"},
        )
        original = atomic_write

        def crash(path, data):
            if path.name == "acme-engineer-123.md":
                raise OSError("simulated crash")
            original(path, data)

        with patch("career_ops.atomic_write", crash), self.assertRaises(OSError):
            self.call("record_event", **args)
        target = self.root / args["record"]
        manually_edited = target.read_text() + "\nNew manual evidence that must be kept.\n"
        target.write_text(manually_edited)
        with self.assertRaisesRegex(CareerError, "conflicts with a manual edit"):
            Ledger(self.root).dispatch("record_event", args)
        self.assertEqual(target.read_text(), manually_edited)
        self.assertIn("pending", self.ledger.load_state())

    def test_recovered_receipt_preserves_newer_interview(self):
        run = self.run_apply()
        attempt = self.reserve(run, self.packet(run))
        self.call("begin_submission", attempt_id=attempt, idempotency_key="begin")
        self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="outcome_unknown",
            summary="Browser interrupted",
            evidence=[],
            idempotency_key="unknown",
        )
        self.call(
            "finish_run",
            run_id=run,
            status="partial",
            summary="One unresolved attempt",
            evidence=[],
            idempotency_key="partial",
        )
        review = self.call(
            "start_run",
            operation="review_inbox",
            scope={},
            user_request="Review emails",
            idempotency_key="review",
        )["run_id"]
        self.call(
            "update_application",
            run_id=review,
            record="applications/acme-engineer-123.md",
            tracking={
                "stage": "screening",
                "checked": "2026-10-11",
                "next_action": "Choose a screening time",
            },
            summary="Screening invitation received",
            evidence=["https://mail.google.com/mail/u/0/#all/invite"],
            idempotency_key="invite",
        )
        self.call(
            "finish_submission",
            attempt_id=attempt,
            outcome="submitted",
            summary="Original receipt found",
            evidence=["https://mail.google.com/mail/u/0/#all/receipt"],
            idempotency_key="recovered",
        )
        record = self.call("find_application", query="Acme")["matches"][0]
        self.assertEqual(record["stage"], "screening")
        self.assertEqual(record["next_action"], "Choose a screening time")

    def test_skill_installation_is_linked_idempotent_and_conflict_safe(self):
        home = self.root / "any-client-skills"
        with patch("builtins.print"):
            install(home)
            install(home)
        link = home / "career-apply"
        self.assertTrue(link.is_symlink())
        self.assertEqual(link.resolve(), (ROOT / ".agents/skills/career-apply").resolve())
        link.unlink()
        link.mkdir()
        (link / "SKILL.md").write_text("Unrelated existing skill")
        with self.assertRaisesRegex(ValueError, "Conflicting skill"):
            install(home)
        self.assertEqual((link / "SKILL.md").read_text(), "Unrelated existing skill")

    def test_another_profile_uses_configured_sources_and_defaults(self):
        custom = "context/profile/custom-answers.md"
        (self.root / custom).write_text(
            "# Answers\n\n## Salary\nCandidate-confirmed target: 120000 EUR.\n"
        )
        config = {
            "schema_version": 1,
            "timezone": "UTC",
            "primary_cv": "output/cv.pdf",
            "preferred_browser": "any-test-client",
            "answer_sources": [custom],
        }
        (self.root / "config/career-ledger.json").write_text(json.dumps(config))
        plan = self.call("plan_workflow", operation="apply")
        self.assertEqual(plan["ledger"], str(self.root.resolve()))
        self.assertEqual(plan["settings"], config)
        self.assertIn(custom, plan["shared_reads"])
        self.assertNotIn("applications/approved-answers.md", plan["shared_reads"])
        answers = self.call("get_approved_answer", query="salary")["matches"]
        self.assertEqual(len(answers), 1)
        self.assertIn("120000 EUR", answers[0]["text"])
        run = self.run_apply()
        packet = self.packet(run)
        snapshot = json.loads(
            (self.root / "applications/operations/packets" / (packet + ".json")).read_text()
        )
        self.assertEqual(list(snapshot["answer_source_sha256"]), [custom])

    def test_configuration_cannot_expand_read_allowlist(self):
        config = {"schema_version": 1, "answer_sources": ["../private.md"]}
        (self.root / "config/career-ledger.json").write_text(json.dumps(config))
        with self.assertRaises(CareerError):
            self.call("plan_workflow", operation="review_inbox")

    def test_changed_saved_answers_block_stale_packet(self):
        run = self.run_apply()
        attempt = self.reserve(run, self.packet(run))
        with (self.root / "applications/approved-answers.md").open("a") as stream:
            stream.write("\n## New preference\nUpdated direct user preference.\n")
        with self.assertRaisesRegex(CareerError, "Canonical answers changed"):
            self.call("begin_submission", attempt_id=attempt, idempotency_key="begin-after-change")
        state = self.ledger.load_state()
        self.assertEqual(state["attempts"][attempt]["status"], "reserved")
        self.call(
            "cancel_reservation",
            attempt_id=attempt,
            summary="Refresh changed answers before submitting",
            idempotency_key="cancel-stale",
        )

    def test_client_neutral_mcp_config_requires_no_client_install(self):
        (self.root / "config/career-ledger.json").write_text(
            '{"schema_version":1,"timezone":"UTC"}'
        )
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/install_career_tools.py"),
                "--print-mcp",
                "--root",
                str(self.root),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        info = json.loads(result.stdout)
        self.assertEqual(set(info), {"command", "args"})
        self.assertEqual(
            info["args"],
            [
                "-P",
                "-m",
                "job_harness",
                "--root",
                str(self.root.resolve()),
                "mcp",
                "--tool-profile",
                "focused",
            ],
        )

    def test_mcp_and_cli_share_functions(self):
        (self.root / "config/career-ledger.json").write_text(
            '{"schema_version":1,"timezone":"UTC"}'
        )
        messages = [
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-06-18"},
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "find_application", "arguments": {"query": "Acme"}},
            },
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "read_record", "arguments": {"path": "/etc/passwd"}},
            },
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "find_application", "arguments": {"query": "Acme", "root": "/"}},
            },
        ]
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/career_mcp.py"), "--root", str(self.root)],
            input="\n".join(json.dumps(m) for m in messages) + "\n",
            capture_output=True,
            text=True,
            check=True,
        )
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(rows), 5)
        self.assertIn("reserve_submission", [t["name"] for t in rows[1]["result"]["tools"]])
        self.assertEqual(len(rows[2]["result"]["structuredContent"]["data"]["matches"]), 1)
        self.assertTrue(rows[3]["result"]["isError"])
        self.assertTrue(rows[4]["result"]["isError"])
        cli = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/career_ops.py"),
                "--root",
                str(self.root),
                "find_application",
            ],
            input='{"query":"Acme"}',
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(json.loads(cli.stdout), rows[2]["result"]["structuredContent"])


if __name__ == "__main__":
    unittest.main()
