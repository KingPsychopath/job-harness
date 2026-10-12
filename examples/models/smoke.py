#!/usr/bin/env python3
"""Explicit real-runtime smoke with fictional texts; requires installed SDKs + approved cached weights."""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import platform
import select
import signal
import subprocess
import sys
import time
import tempfile
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", required=True, type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--harness", type=Path, help="also exercise a supplied Rust job-harness binary in a temporary fictional workspace")
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[2]
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(repository / "providers" / "local")
    process = subprocess.Popen([sys.executable, "-m", "job_harness_local", "--cache-dir", str(args.cache_dir.resolve()), "serve", "--embedding-model", "embeddinggemma2-text", "--decision-model", "laya-s256", "--port", "0", "--threads", str(args.threads), "--timeout", "30"], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        if not select.select([process.stdout], [], [], 60)[0]:
            raise RuntimeError("provider did not report ready within60seconds")
        ready_line = process.stdout.readline()
        if not ready_line:
            raise RuntimeError("provider failed before reporting ready")
        ready = json.loads(ready_line)
        base = ready["endpoint"]

        def request(route, payload):
            started = time.monotonic()
            connection = urllib.request.Request(base + route, data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(connection, timeout=35) as response:
                    result = json.load(response)
                    status = response.status
            except urllib.error.HTTPError as error:
                result, status = json.load(error), error.code
            return status, result, round((time.monotonic() - started) * 1000, 3)

        documents = ["Reduced database response times by removing repeated SQL queries.", "Built an accessible calendar interface for selecting appointments.", "Added image cropping and colour controls to a photo editor.", "Wrote recovery tests that reconcile interrupted application attempts."]
        rows = ["title: none | text: " + documents[i % 4] for i in range(32)]
        status, indexed, index_ms = request("/embeddings", {"model": "embeddinggemma2-text", "input": rows})
        assert status == 200 and len(indexed["data"]) == 32
        assert all(len(row["embedding"]) == 768 and all(math.isfinite(n) for n in row["embedding"]) for row in indexed["data"])
        query_times = []
        for _ in range(5):
            status, query, elapsed = request("/embeddings", {"model": "embeddinggemma2-text", "input": ["task: search result | query: speeding up slow database queries"]})
            assert status == 200
            query_times.append(elapsed)
        vector = query["data"][0]["embedding"]
        scores = [sum(a * b for a, b in zip(vector, indexed["data"][i]["embedding"])) for i in range(4)]
        assert max(range(4), key=lambda i: scores[i]) == 0
        status, decisions, decision_ms = request("/decisions", {"model": "laya-s256", "state": "Please choose an interview time this week.", "questions": {"intent": {"type": "choice", "instructions": "Choose the requested action.", "criteria": {"meeting": "Choose a time for an interview.", "test": "Complete a written assessment."}}, "action": {"type": "boolean", "condition": "The message asks the reader to act."}, "reply": {"type": "nullable_boolean", "condition": "The message asks for a written reply."}, "priority": {"type": "score", "instructions": "Rate time sensitivity.", "rubric": ["No action requested.", "Action this week.", "Action today."]}}})
        assert status == 200, decisions
        assert decisions["results"]["intent"]["value"] in {"meeting", "test"}
        assert isinstance(decisions["results"]["action"]["value"], bool)
        assert decisions["results"]["reply"]["value"] in {None, True, False}
        assert 0 <= decisions["results"]["priority"]["value"] <= 2
        mixed = []
        for selected in ["We received your application. Please reply with interview availability.", "Unfortunately we cannot offer this role. Are you available for another team's interview?"]:
            status, result, elapsed = request("/chat/completions", {"model": "laya-s256", "messages": [{"role": "system", "content": "Return ONLY JSON {label,needs_reply,urgency,uncertain,rationale}."}, {"role": "user", "content": json.dumps({"text": selected})}], "response_format": {"type": "json_object"}, "stream": False})
            assert status == 200, result
            decision = json.loads(result["choices"][0]["message"]["content"])
            assert decision["needs_reply"] is True and decision["uncertain"] is True
            mixed.append({"elapsed_ms": elapsed, "decision": decision})
        status, rejected, _ = request("/chat/completions", {"model": "laya-s256", "messages": [{"role": "user", "content": "Write an invented CV."}]})
        assert status == 400 and rejected["error"]["code"] == "unsupported_generation"
        report = {"measurement_date": "2026-10-12", "scope": "Real native inference via HTTP on fictional selected inputs; no candidate workspace or audio. Retrieval timing is embedding generation for32passages, not full workspace indexing.", "hardware": {"system": platform.system(), "architecture": platform.machine(), "logical_cpus": os.cpu_count()}, "provider": ready, "embedding_batch": {"passages": 32, "dimensions": 768, "http_elapsed_ms": index_ms, "native_metrics": indexed["metrics"]}, "warm_query_http_ms": query_times, "retrieval": {"toy_cosine_scores": scores, "expected_passage_rank": 1, "quality_claim": "one synthetic ranking smoke, not a relevance benchmark"}, "typed_decisions": {"http_elapsed_ms": decision_ms, "result": decisions}, "mixed_request_triage": mixed, "generative_request_rejected": rejected}
        if args.harness:
            with tempfile.TemporaryDirectory(prefix="job-harness-model-smoke-") as temporary:
                root = Path(temporary) / "workspace"
                executable = args.harness.resolve()
                def harness(command):
                    started = time.monotonic()
                    completed = subprocess.run([str(executable), "--root", str(root), *command], capture_output=True, text=True, timeout=35)
                    result = json.loads(completed.stdout)
                    assert completed.returncode == 0 and result["ok"], result
                    return {"elapsed_ms": round((time.monotonic() - started) * 1000, 3), "response": result}
                initialized = harness(["init"])
                for key in ("workspace", "ledger"):
                    initialized["response"]["data"][key] = "<temporary synthetic workspace>"
                sources = root / "context" / "projects"
                sources.mkdir(parents=True, exist_ok=True)
                (sources / "database.md").write_text("# Fictional database project\n\nSource: synthetic fixture; no real career evidence.\n\nReduced response times by eliminating repeated SQL reads.\n")
                (sources / "images.md").write_text("# Fictional photo editor\n\nSource: synthetic fixture; no real career evidence.\n\nAdded image cropping and colour controls to a photo editing interface.\n")
                common = ["--endpoint", base, "--model", "embeddinggemma2-text", "--revision", ready["models"]["embeddinggemma2-text"], "--document-prefix", "title: none | text: ", "--query-prefix", "task: search result | query: ", "--timeout-seconds", "30"]
                index = harness(["context", "index", *common, "--source", "context/projects/database.md", "--source", "context/projects/images.md"])
                found = harness(["context", "search", *common, "--query", "speeding up slow database queries"])
                assert found["response"]["data"]["matches"][0]["path"] == "context/projects/database.md"
                message = root / "message.txt"
                message.write_text("Application received. Please reply with interview availability.")
                triage = harness(["context", "triage", "--input", str(message), "--endpoint", base, "--model", "laya-s256", "--revision", ready["models"]["laya-s256"]])
                assert triage["response"]["data"]["decision"]["needs_reply"] is True
                assert triage["response"]["data"]["decision"]["uncertain"] is True
                assert triage["response"]["data"]["archive_permitted"] is False
                report["rust_harness"] = {"init": initialized, "index": index, "search": found, "triage": triage}
        encoded = json.dumps(report, indent=2, allow_nan=False) + "\n"
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(encoded)
        print(encoded)
    finally:
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)


if __name__ == "__main__":
    main()
