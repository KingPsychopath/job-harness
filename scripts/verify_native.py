#!/usr/bin/env python3
"""Verify a copied native executable outside the checkout using only stdlib.

Python is the test driver, never a product dependency. The copied binary runs
with a clean PATH that contains no Python, package code, model keys or host defaults.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    source = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="job-harness-native-") as directory:
        base = Path(directory).resolve()
        installed = base / "bin" / ("job-harness.exe" if os.name == "nt" else "job-harness")
        installed.parent.mkdir()
        shutil.copy2(source, installed)
        installed.chmod(0o700)
        workspace = base / "career"
        restored = base / "restored"
        host = base / "host"
        host.mkdir()
        env = {"PATH": str(installed.parent), "HOME": str(host), "XDG_CONFIG_HOME": str(host / "settings"), "LC_ALL": "C"}
        # Windows runtime lookup needs its OS directory, not model keys or user configuration.
        for name in ("SystemRoot", "WINDIR"):
            if name in os.environ: env[name] = os.environ[name]
        if os.name == "nt":
            temporary = base / "tmp"
            temporary.mkdir()
            env.update({"TEMP":str(temporary),"TMP":str(temporary),"USERPROFILE":str(host),"APPDATA":str(host / "AppData")})
        checks: list[str] = []

        def run(*arguments: str, root: Path = workspace, value=None):
            result = subprocess.run([str(installed), "--root", str(root), *arguments],
                input=None if value is None else json.dumps(value), text=True,encoding="utf-8", capture_output=True,
                cwd=base, env=env, timeout=30)
            if result.returncode:
                raise AssertionError(f"Native command {arguments[:2]} failed: {result.stdout[:500]}")
            output = json.loads(result.stdout)
            if "ok" in output:
                assert output["schema_version"] == 2 and output["ok"], output
                return output["data"]
            return output

        version = subprocess.run([str(installed), "--version"],text=True,encoding="utf-8",capture_output=True,
            cwd=base,env=env,check=True,timeout=10).stdout.strip()
        assert version.startswith("job-harness ")
        checks.append("copied executable; no Python on PATH")
        run("init")
        assert run("call", "bootstrap")["contract_version"] == "2.0"
        checks.append("initialize and bootstrap")
        specs = run("schemas")
        assert len(specs) == 40
        skill = run("call", "read_record",value={"path":".agents/skills/career-apply/SKILL.md"})
        assert skill.get("origin") == "toolkit" and "apply" in str(skill).lower()
        observations = run("call", "read_record", value={"path":"docs/host-observations.md"})
        assert observations.get("origin") == "toolkit"
        assert "completeness_confirmed" in str(observations) and "request_message_ids" in str(observations)
        workflows = run("call", "read_record", value={"path":"docs/workflows.md"})
        assert workflows.get("origin") == "toolkit"
        assert all(name in str(workflows) for name in ["Building experience","Exploring a move","Searching","Starting somewhere"])
        checks.append("embedded schemas and focused skill outside checkout")

        pdf = b"%PDF-1.4\nSynthetic native-package fixture, never an actual CV.\n%%EOF\n"
        (workspace / "output" / "primary-cv.pdf").write_bytes(pdf)
        config_path = workspace / "config" / "career-ledger.json"
        config = json.loads(config_path.read_text(encoding='utf-8'))
        config["primary_cv"] = "output/primary-cv.pdf"
        config_path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        record = workspace / "applications" / "example.md"
        record.write_text("# Example Company — Engineer\n\n<!-- tracking\n" + json.dumps({
            "stage":"saved", "checked":"2026-10-12", "company":"Example Company",
            "vacancy_url":"https://example.com/roles/demo", "source":"Synthetic verification",
            "next_action":"Review synthetic vacancy"}) + "\n-->\n\nSynthetic record only.\n", encoding="utf-8")
        indexed = run("workspace", "index")
        assert indexed["documents"] > 0
        found = run("workspace", "search", "Synthetic")
        assert found["matches"]
        checks.append("native SQLite index and source references")

        supplied = base / "supplied.txt"
        supplied.write_text("Built a synthetic dashboard.\nUnselected supplied detail stays in the original.\n", encoding="utf-8")
        imported = run("source", "import", "--input", str(supplied))
        source_path = imported["path"]
        assert (workspace / source_path).read_bytes() == supplied.read_bytes()
        assert imported["facts_extracted"] is False and imported["primary_cv_changed"] is False
        selection = run("document", "template", "--kind", "application_answers",root=base / "not-initialized")
        assert not (base / "not-initialized").exists()
        selection["sections"][0]["text"] = "Built a synthetic dashboard."
        selection["sections"][0]["citations"] = [{"path":source_path,"sha256":imported["sha256"],"start_line":1,"end_line":1}]
        selected_file = base / "selection.json"
        selected_file.write_text(json.dumps(selection), encoding="utf-8")
        document_path = "output/application-answer.md"
        rendered = run("document", "render", "--input", str(selected_file), "--output", document_path)
        assert rendered["citations_checked"] == 1 and rendered["claims_independently_verified"] is False
        document_bytes = (workspace / document_path).read_bytes()
        assert b"Unselected supplied detail" not in document_bytes
        outcomes = run("workspace", "outcomes")
        assert outcomes["counts"]["pending"] == 1 and outcomes["counts"]["rejected"] == 0
        assert outcomes["counts"]["submitted_denominator"] == 0 and outcomes["counts"]["submission_unknown"] == 1
        assert outcomes["discovery_channel"]["groups"][0]["value"] is None
        rejected_output = subprocess.run([str(installed),"--root",str(workspace),"document","render","--input",str(selected_file),"--output","context/new-facts.md"],text=True,encoding="utf-8",capture_output=True,cwd=base,env=env,timeout=10)
        assert rejected_output.returncode != 0 and json.loads(rejected_output.stdout)["error"]["code"] == "path_denied"
        assert not (workspace / "context" / "new-facts.md").exists()
        status = run("status", "--json")
        assert status["health"]["records"] == 1 and status["attention"]["live_mail_checked"] is False
        human_status = subprocess.run([str(installed),"--root",str(workspace),"status"],text=True,encoding="utf-8",capture_output=True,cwd=base,env=env,timeout=10,check=True).stdout
        assert "Job Harness · Workspace" in human_status and "Next tool:" in human_status
        checks.append("explicit original import, cited selection-only render and descriptive outcomes")

        bundle = base / "private.jhb"
        run("workspace", "export", "--output", str(bundle))
        run("workspace", "import", "--input", str(bundle),root=restored)
        assert hashlib.sha256((restored / "output" / "primary-cv.pdf").read_bytes()).digest() == hashlib.sha256(pdf).digest()
        assert (restored / "applications" / "example.md").read_bytes() == record.read_bytes()
        assert (restored / source_path).read_bytes() == supplied.read_bytes()
        assert (restored / document_path).read_bytes() == document_bytes
        assert run("call", "bootstrap",root=restored)["health"]["records"] == 1
        checks.append("private bundle transfer preserves CV and application bytes")

        for profile, expected in [("focused",11),("full",len(specs))]:
            child = subprocess.Popen([str(installed), "--root",str(workspace),"mcp","--tool-profile",profile],
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding="utf-8",cwd=base,env=env)
            messages: queue.Queue = queue.Queue()
            def collect():
                try:
                    for line in child.stdout:
                        messages.put(json.loads(line))
                except Exception as error:
                    messages.put(error)
            thread = threading.Thread(target=collect,daemon=True)
            thread.start()
            def request(identifier, method, params):
                child.stdin.write(json.dumps({"jsonrpc":"2.0","id":identifier,"method":method,"params":params})+"\n")
                child.stdin.flush()
                while True:
                    response = messages.get(timeout=10)
                    if isinstance(response,Exception): raise response
                    if response.get("id")==identifier:
                        assert "error" not in response, response
                        return response["result"]
            try:
                initialized = request(1,"initialize",{"protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"native-artifact-verifier","version":"1"}})
                assert initialized["serverInfo"]["name"] == "job-harness"
                child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');child.stdin.flush()
                catalog = request(2,"tools/list",{})
                assert len(catalog["tools"]) == expected
                result = request(3,"tools/call",{"name":"bootstrap","arguments":{}})
                assert result["structuredContent"]["schema_version"] == 2 and result["structuredContent"]["ok"]
                assert json.loads(result["content"][0]["text"]) == result["structuredContent"]
            finally:
                child.stdin.close()
                try: child.wait(timeout=3)
                except subprocess.TimeoutExpired: child.kill();child.wait(timeout=3)
                thread.join(timeout=1)
            checks.append(f"MCP stdio {profile}: initialize, {expected} tools, bootstrap")
        print(json.dumps({"ok":True,"version":version,"checks":checks},separators=(",",":")))


if __name__ == "__main__":
    main()
