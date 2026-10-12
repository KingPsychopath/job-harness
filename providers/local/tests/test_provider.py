"""Synthetic contract, download integrity, HTTP boundary and failure tests; no SDK/weights."""
import hashlib
from http.client import HTTPConnection
import json
import io
import os
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch

from job_harness_local import artifacts, catalog
from job_harness_local.runtime import Runtime, decision_payload, embedding_payload, triage_payload, triage_consistency, validate_question
from job_harness_local.server import Gateway, Worker
from job_harness_local.profiles import summary
from job_harness_local.profile_data import profiles
from job_harness_local import profile_artifacts
from job_harness_local.kev_runtime import KevRuntime


class SyntheticWorker:
    metadata = {"synthetic": True}
    process = type("Process", (), {"is_alive": lambda self: True})()
    calls = []

    def call(self, route, payload):
        self.calls.append((route, payload))
        if payload.get("state") == "simulate-timeout":
            raise ValueError("provider_timeout: worker stopped")
        return {"synthetic": True}


class Contracts(unittest.TestCase):
    def test_compact_summary_matches_reviewed_catalogs(self):
        root = Path(__file__).resolve().parents[1]
        self.assertEqual(json.loads((root / "summary.json").read_text()), summary())
        self.assertEqual(json.loads((root / "model-profiles.json").read_text())["profiles"], profiles())
        entries = {item["id"]: item for item in summary()["profiles"]}
        self.assertEqual(entries["kev-0.8b"]["download_bytes"], 1835370519)
        self.assertEqual(entries["kev-4b"]["component_revisions"]["adapter"], "6cfce5c2fa4b4bd64026336ab649c5ca78857d52")
        self.assertEqual(entries["clef-flash"]["download_bytes"], 19083367679)
        self.assertTrue(all("files" not in item for item in entries.values()))

    def test_metadata_never_imports_or_downloads_weights(self):
        with tempfile.TemporaryDirectory() as tmp, patch("urllib.request.OpenerDirector.open", side_effect=AssertionError("network")):
            entry = catalog.info("embeddinggemma2-text", Path(tmp))
            self.assertFalse(entry["installed"])
            self.assertFalse(entry["automatic_download"])
            self.assertIsNone(entry["working_memory_bytes"])
            self.assertEqual(entry["download_bytes"], 164626432)
            self.assertEqual(catalog.model("laya-s256")["revision"], "gcs-1791231584156797")

    def test_unapproved_download_does_not_create_files_or_network(self):
        with tempfile.TemporaryDirectory() as tmp, patch("urllib.request.OpenerDirector.open", side_effect=AssertionError("network")):
            with self.assertRaisesRegex(ValueError, "download_not_approved"):
                artifacts.download(Path(tmp), "laya-s256", "wrong-token")
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_corrupt_installation_refuses_native_load(self):
        with tempfile.TemporaryDirectory() as tmp:
            cache = Path(tmp)
            spec = catalog.model("laya-s256")
            directory = catalog.directory(cache, spec)
            directory.mkdir(parents=True)
            (directory / spec["artifact"]).write_bytes(b"corrupt")
            (directory / "manifest.json").write_text(json.dumps({"id": "laya-s256", "revision": spec["revision"], "sha256": hashlib.sha256(b"corrupt").hexdigest()}))
            with self.assertRaisesRegex(ValueError, "model_integrity_error"):
                artifacts.verify(cache, "laya-s256")

    def test_decisions_preserve_nullable_and_score_contracts(self):
        state, questions = decision_payload({"state": "Fictional message", "questions": {"reply": {"type": "nullable_boolean", "condition": "Asks for action."}, "priority": {"type": "score", "rubric": ["Routine", "Urgent"]}}})
        self.assertEqual(state, "Fictional message")
        self.assertEqual((questions["reply"]["lower"], questions["reply"]["upper"]), (0.2, 0.8))
        self.assertEqual(questions["priority"]["rubric"], ["Routine", "Urgent"])
        for invalid in [{"type": "boolean", "condition": "Question", "threshold": float("nan")}, {"type": "nullable_boolean", "condition": "Question", "lower": 0.9, "upper": 0.2}, {"type": "choice", "criteria": {"only": "One option"}}, {"type": "score", "rubric": ["Only one"]}]:
            with self.assertRaises(ValueError):
                validate_question(invalid)

    def test_embeddings_refuse_modalities_token_ids_and_unbounded_input(self):
        self.assertEqual(embedding_payload({"input": ["already prefixed text"]}), ["already prefixed text"])
        for bad in [{"input": [1, 2, 3]}, {"input": {"image": "url"}}, {"input": "x" * 32769}, {"input": ["x"] * 33}, {"input": "valid", "encoding_format": "base64"}]:
            with self.assertRaises(ValueError):
                embedding_payload(bad)

    def test_gateway_cannot_be_used_as_generative_agent(self):
        with self.assertRaisesRegex(ValueError, "unsupported_generation"):
            triage_payload({"messages": [{"role": "user", "content": "Write my CV"}]})
        request = {"messages": [{"role": "system", "content": "label needs_reply urgency uncertain rationale"}, {"role": "user", "content": json.dumps({"text": "Application received. Please reply."})}], "response_format": {"type": "json_object"}}
        self.assertIn("Please reply", triage_payload(request))
        request["tools"] = []
        with self.assertRaisesRegex(ValueError, "unsupported_generation"):
            triage_payload(request)

    def test_native_probability_contract_rejects_impossible_values(self):
        runtime = Runtime.__new__(Runtime)
        for bad in [{"type": "boolean", "confidence": 0.5, "probability_true": 1.5}, {"type": "choice", "confidence": 0.2, "probabilities": {"one": 0.5, "two": 0.6}}, {"type": "score", "confidence": float("nan"), "value": 1, "scale": {"maximum": 2}}]:
            with self.assertRaisesRegex(ValueError, "provider_schema_error"):
                runtime.checked(bad)

    def test_triage_cross_field_consistency_preserves_requests(self):
        def results(label, reply, urgency="normal"):
            return {"label": {"value": label, "confidence": 0.99}, "needs_reply": {"value": reply, "abstained": False}, "urgency": {"value": urgency, "confidence": 0.99}}
        for label, reply in [("receipt", True), ("rejection", True), ("scheduling", False), ("assessment", False)]:
            decision = triage_consistency("A fictional update without guard keywords.", results(label, reply))
            self.assertTrue(decision["needs_reply"])
            self.assertTrue(decision["uncertain"])
            self.assertEqual(decision["urgency"], "review")
        footer = triage_consistency("Application received. https://example.invalid/?utm_source=fixture Questions?", results("receipt", False))
        self.assertFalse(footer["needs_reply"])
        self.assertFalse(footer["uncertain"])

    def test_download_recovers_manifest_after_interrupted_publication(self):
        data = b"synthetic fixture bytes"
        spec = {"id": "fixture", "revision": "one", "artifact": "fixture.bin", "download_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(), "md5": None, "source": "https://example.invalid/fixture", "license": "synthetic-test"}
        with tempfile.TemporaryDirectory() as temporary, patch.dict(catalog.CATALOG, {"fixture": spec}), patch("urllib.request.OpenerDirector.open", side_effect=AssertionError("network")):
            root = catalog.directory(Path(temporary), spec)
            root.mkdir(parents=True)
            (root / spec["artifact"]).write_bytes(data)
            result = artifacts.download(Path(temporary), "fixture", catalog.approval_token(spec))
            self.assertTrue(result["recovered_manifest"])
            self.assertEqual(artifacts.verify(Path(temporary), "fixture"), root / spec["artifact"])

    def test_multifile_download_and_completed_files_resume_without_network(self):
        payload = b"synthetic model bytes"
        item = {"path": "fixture.bin", "bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest(), "url": "https://example.invalid/no-network"}
        spec = {"id": "kev-0.8b", "revision": "synthetic-revision", "download_bytes": len(payload), "components": [{"role": "adapter", "revision": "synthetic-component", "files": [item]}]}
        with tempfile.TemporaryDirectory() as temporary, patch("job_harness_local.profile_artifacts.profile", return_value=spec), patch("urllib.request.OpenerDirector.open", return_value=io.BytesIO(payload)) as network:
            cache = Path(temporary)
            result = profile_artifacts.download(cache, "kev-0.8b", catalog.approval_token(spec))
            self.assertTrue(result["installed"])
            self.assertEqual(network.call_count, 1)
            root = profile_artifacts.verify(cache, "kev-0.8b")
            stray = root / "adapter" / "stray.safetensors"
            stray.write_bytes(b"unreviewed fixture bytes")
            with self.assertRaisesRegex(ValueError, "model_integrity_error"):
                profile_artifacts.verify(cache, "kev-0.8b")
            stray.unlink()
            (root / "manifest.json").unlink()
            network.side_effect = AssertionError("network")
            resumed = profile_artifacts.download(cache, "kev-0.8b", catalog.approval_token(spec))
            self.assertEqual(resumed["resumed_files"], 1)
            self.assertEqual(profile_artifacts.verify(cache, "kev-0.8b"), root)
            (root / "adapter" / "fixture.bin").write_bytes(b"wrong cached bytes")
            with self.assertRaisesRegex(ValueError, "model_integrity_error"):
                profile_artifacts.verify(cache, "kev-0.8b")

    def test_larger_alternatives_remain_metadata_only(self):
        with tempfile.TemporaryDirectory() as temporary, patch("urllib.request.OpenerDirector.open", side_effect=AssertionError("network")):
            for name in ["kev-4b", "clef-flash"]:
                with self.assertRaisesRegex(ValueError, "metadata_only"):
                    profile_artifacts.download(Path(temporary), name, catalog.approval_token(catalog.model(name)))
                with self.assertRaisesRegex(ValueError, "metadata_only"):
                    profile_artifacts.verify(Path(temporary), name)
            self.assertEqual(list(Path(temporary).iterdir()), [])

    def test_kev_metadata_preserves_question_identity_and_boolean_polarity(self):
        runtime = KevRuntime.__new__(KevRuntime)
        question = {"type": "nullable_boolean", "condition": "Requests a reply."}
        runtime.validate_metadata("reply", question, {"id": "reply", "type": "noul", "keys": ["false", "true"]})
        for wrong in [{"id": "another", "type": "noul", "keys": ["false", "true"]}, {"id": "reply", "type": "noul", "keys": ["true", "false"]}, {"id": "reply", "type": "boolean", "keys": ["false", "true"]}]:
            with self.assertRaisesRegex(ValueError, "provider_schema_error"):
                runtime.validate_metadata("reply", question, wrong)
        with self.assertRaisesRegex(ValueError, "provider_schema_error"):
            runtime.validate_metadata("purpose", {"type": "choice", "criteria": {"yes": "Yes", "no": "No"}}, {"id": "purpose", "type": "choice", "keys": ["yes", "extra"]})

    def test_unknown_rss_and_unsupported_windows_worker_are_explicit(self):
        with patch("job_harness_local.runtime.resource", None):
            from job_harness_local.runtime import peak_rss_bytes
            self.assertIsNone(peak_rss_bytes())
        with patch("job_harness_local.server.os.name", "nt"):
            with self.assertRaisesRegex(ValueError, "unsupported_runtime"):
                Worker({}, threads=1)

    def test_invalid_unicode_fails_admission(self):
        with self.assertRaisesRegex(ValueError, "invalid_arguments"):
            embedding_payload({"input": "\ud800"})

    def test_worker_timeout_stops_process_and_refuses_retry(self):
        class Process:
            alive = True
            def is_alive(self): return self.alive
            def join(self, timeout): pass
            def terminate(self): self.alive = False
        class Pipe:
            def send_bytes(self, value): pass
            def poll(self, timeout): return False
            def close(self): pass
        worker = Worker.__new__(Worker)
        worker.process, worker.pipe, worker.timeout = Process(), Pipe(), 0
        with self.assertRaisesRegex(ValueError, "provider_timeout"):
            worker.call("/v1/embeddings", {})
        self.assertFalse(worker.process.alive)
        with self.assertRaisesRegex(ValueError, "runtime_unavailable"):
            worker.call("/v1/embeddings", {})


@unittest.skipUnless(os.name == "posix", "Optional native worker uses POSIX process/descriptor isolation; metadata and HTTP contract tests remain portable")
class RealProcessBoundary(unittest.TestCase):
    def test_worker_starts_without_inheriting_host_credentials_or_cwd(self):
        with tempfile.TemporaryDirectory() as private, patch.dict(os.environ, {"OPENAI_API_KEY": "synthetic-test-key", "AWS_SESSION_TOKEN": "synthetic-test-token"}):
            initial = Path.cwd()
            try:
                os.chdir(private)
                worker = Worker({}, threads=1, timeout=2, startup_timeout=5)
                try:
                    self.assertFalse(worker.metadata["host_credentials_inherited"])
                    self.assertTrue(worker.metadata["python_isolated"])
                    self.assertTrue(worker.metadata["temporary_working_directory"])
                    self.assertEqual(worker.metadata["core_dump_limit_bytes"], 0)
                    self.assertTrue(worker.process.is_alive())
                finally:
                    worker.close()
                self.assertFalse(worker.process.is_alive())
            finally:
                os.chdir(initial)


class HttpBoundary(unittest.TestCase):
    def setUp(self):
        self.worker = SyntheticWorker()
        self.worker.calls = []
        self.server = Gateway(0, self.worker, {"synthetic": "fixture-v1"})
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=1)

    def request(self, method="POST", path="/v1/decisions", body='{"model":"synthetic","state":"fictional message","questions":{"reply":{"type":"boolean","condition":"Needs reply."}}}', headers=None):
        conn = HTTPConnection("127.0.0.1", self.server.server_port, timeout=2)
        conn.request(method, path, body=body, headers=headers or {"Content-Type": "application/json"})
        response = conn.getresponse()
        value = json.loads(response.read())
        conn.close()
        return response.status, value

    def test_loopback_direct_json_is_accepted_and_unlogged(self):
        status, response = self.request()
        self.assertEqual(status, 200)
        self.assertTrue(response["synthetic"])
        self.assertEqual(self.server.server_address[0], "127.0.0.1")

    def test_browser_origin_and_rebinding_host_never_reach_worker(self):
        for headers in [{"Content-Type": "application/json", "Origin": "https://attacker.invalid"}, {"Content-Type": "application/json", "Host": "attacker.invalid"}]:
            self.assertEqual(self.request(headers=headers)[0], 403)
        self.assertEqual(self.worker.calls, [])

    def test_malformed_and_oversized_requests_are_rejected(self):
        for body in ['["array"]', '{"model":NaN}', '{"model":"a","model":"b"}', '{bad']:
            self.assertEqual(self.request(body=body)[0], 400)
        self.assertEqual(self.request(headers={"Content-Type": "text/plain"})[0], 415)
        self.assertEqual(self.request(body="{}", headers={"Content-Type": "application/json", "Content-Length": "1048577"})[0], 413)
        self.assertEqual(self.request(path="/v1/audio/transcriptions")[0], 404)
        self.assertEqual(self.worker.calls, [])

    def test_unexpected_gateway_exception_is_generic_and_does_not_echo_input(self):
        with patch.object(self.worker, "call", side_effect=TypeError("synthetic-private-canary")):
            status, response = self.request()
            self.assertEqual(status, 500)
            self.assertEqual(response["error"]["code"], "gateway_failure")
            self.assertNotIn("synthetic-private-canary", json.dumps(response))

    def test_inference_timeout_is_visible_as_unavailable(self):
        status, response = self.request(body='{"model":"synthetic","state":"simulate-timeout","questions":{"reply":{"type":"boolean","condition":"Needs reply."}}}')
        self.assertEqual(status, 503)
        self.assertEqual(response["error"]["code"], "provider_timeout")


if __name__ == "__main__":
    unittest.main()
