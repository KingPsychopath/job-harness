#!/usr/bin/env python3
"""Real selected provider probe on frozen fictional cases; no install/download or quality ranking."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--model", required=True, choices=["laya-s256", "kev-0.8b"])
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--harness", type=Path)
    args = parser.parse_args()
    fixture_path = Path(__file__).with_name("triage-fixture.json")
    raw_fixture = fixture_path.read_bytes()
    fixture = json.loads(raw_fixture)
    base = args.endpoint.rstrip("/")
    if not base.startswith("http://127.0.0.1:"):
        raise ValueError("Use this explicitly selected loopback provider")
    def request(path, payload=None):
        started = time.monotonic()
        data = json.dumps(payload).encode() if payload is not None else None
        with urllib.request.urlopen(urllib.request.Request(base + path, data=data, headers={"Content-Type": "application/json"}), timeout=35) as response:
            result = json.load(response)
        return result, round((time.monotonic() - started) * 1000, 3)
    root_url = base[:-3] if base.endswith("/v1") else base
    with urllib.request.urlopen(root_url + "/health", timeout=5) as response:
        health = json.load(response)
    rows = []
    warm = []
    for case in fixture["cases"]:
        for order in ["original", "reversed"]:
            questions = copy.deepcopy(fixture["questions"])
            if order == "reversed":
                for name in ["label", "urgency"]:
                    questions[name]["criteria"] = dict(reversed(list(questions[name]["criteria"].items())))
            payload = {"model": args.model, "state": case["text"], "questions": questions}
            first, first_ms = request("/decisions", payload)
            last, last_ms = request("/decisions", payload)
            assert first["model_revision"] == args.revision and last["model_revision"] == args.revision
            results = last["results"]
            assert results["label"]["value"] in questions["label"]["criteria"]
            assert results["needs_reply"]["value"] in [None, False, True]
            assert isinstance(results["action"]["value"], bool)
            assert 0 <= results["priority"]["value"] <= 2
            assert last["canonical_changes"] is False
            warm.append(last_ms)
            rows.append({"id": case["id"], "order": order, "truth": {"label": case["expected_label"], "needs_reply": case["expected_needs_reply"]}, "raw_decision": last, "first_shape_http_ms": first_ms, "warm_repeat_http_ms": last_ms})
    guarded = []
    harness_rows = []
    for case in fixture["cases"]:
        result, elapsed = request("/chat/completions", {"model": args.model, "messages": [{"role": "system", "content": "Return ONLY JSON {label,needs_reply,urgency,uncertain,rationale}."}, {"role": "user", "content": json.dumps({"text": case["text"]})}], "response_format": {"type": "json_object"}, "stream": False})
        decision = json.loads(result["choices"][0]["message"]["content"])
        if case["expected_needs_reply"] is True:
            assert decision["needs_reply"] is True or decision["uncertain"] is True
        guarded.append({"id": case["id"], "decision": decision, "http_elapsed_ms": elapsed})
        if args.harness:
            with tempfile.TemporaryDirectory(prefix="job-harness-typed-smoke-") as temporary:
                selected = Path(temporary) / "message.txt"
                selected.write_text(case["text"])
                started = time.monotonic()
                completed = subprocess.run([str(args.harness.resolve()), "--root", temporary, "context", "triage", "--input", str(selected), "--endpoint", base, "--model", args.model, "--revision", args.revision], capture_output=True, text=True, timeout=35)
                response = json.loads(completed.stdout)
                assert completed.returncode == 0 and response["ok"] and response["data"]["archive_permitted"] is False
                harness_rows.append({"id": case["id"], "elapsed_ms": round((time.monotonic() - started) * 1000, 3), "response": response})
    flips = [a["id"] for a, b in zip(rows[::2], rows[1::2]) if a["raw_decision"]["results"]["label"]["value"] != b["raw_decision"]["results"]["label"]["value"]]
    report = {"measurement_date": "2026-10-12", "scope": "Real HTTP/native feasibility smoke on six fictional cases and two option orders; no private facts, quality ranking, automated actions or model downloads.", "fixture_sha256": hashlib.sha256(raw_fixture).hexdigest(), "model": args.model, "revision": args.revision, "health": health, "raw_cases": rows, "guarded_triage": guarded, "rust_cli_triage": harness_rows, "observed_option_order_label_flips": flips, "warm_repeat_http_ms": {"values": warm, "median": statistics.median(warm), "maximum": max(warm)}, "quality_claim": "No general accuracy or calibration claim; inspect every raw error and abstention."}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"model": args.model, "cases": len(fixture["cases"]), "rows": len(rows), "warm_median_ms": statistics.median(warm), "order_flips": flips, "output": str(args.output)}))


if __name__ == "__main__":
    main()
