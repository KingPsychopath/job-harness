"""Fresh wheel install outside the source tree; no secrets, network or personal data."""

import json
import os
import subprocess
import tempfile
import venv
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message="Verification failed"):
    if not condition:
        raise AssertionError(message)


def verify():
    wheel = sorted((ROOT / "dist").glob("*.whl"))[-1]
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        require(all((n.startswith(("job_harness/", "job_harness-")) for n in names)), names)
        require(
            not any(("/context/" in n or "/applications/" in n or "/config/" in n for n in names)),
            "Verification failed",
        )
        metadata = archive.read(next((n for n in names if n.endswith("/METADATA")))).decode()
        require(
            "Requires-Dist:" not in metadata.split("Provides-Extra:", 1)[0], "Verification failed"
        )
        require(
            "job_harness/resources/skills/career-apply/SKILL.md" in names, "Verification failed"
        )
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        environment = root / "runtime"
        venv.EnvBuilder(with_pip=True).create(environment)
        python = environment / "bin/python"
        command = environment / "bin/job-harness"
        env = {
            k: v for k, v in os.environ.items() if k not in {"PYTHONPATH", "JOB_HARNESS_WORKSPACE"}
        }
        env["XDG_CONFIG_HOME"] = str(root / "host-config")
        subprocess.run(
            [str(python), "-m", "pip", "install", "--no-deps", "--no-index", str(wheel)],
            cwd=root,
            env=env,
            check=True,
            capture_output=True,
        )

        def run(*args, input=None):
            return subprocess.run(
                [str(command), *args],
                input=input,
                cwd=root,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

        require("0.1.0" in run("--version").stdout, "Verification failed")
        workspace = root / "private"
        run("--root", str(workspace), "init")
        run("configure", "--workspace", str(workspace))
        value = json.loads(
            run("call", "bootstrap", input='{"operation":"apply","capabilities":[]}').stdout
        )
        require(value["ok"] and value["data"]["health"]["records"] == 0, "Verification failed")
        value = json.loads(
            run(
                "call", "read_record", input='{"path":".agents/skills/career-apply/SKILL.md"}'
            ).stdout
        )
        require(value["ok"] and value["data"]["origin"] == "toolkit", "Verification failed")
        value = json.loads(run("connect", "mcp").stdout)
        require(
            value["data"]["command"] == str(python) and value["data"]["args"][-1] == "focused",
            "Verification failed",
        )
        require(value["data"]["args"][:2] == ["-P", "-m"], "Verification failed")
        # A host may launch from an arbitrary cwd: planted modules must not shadow installed code.
        (root / "job_harness.py").write_text('raise RuntimeError("cwd shadowing")')
        (root / "json.py").write_text('raise RuntimeError("cwd shadowing")')
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "wheel-check", "version": "1"},
            },
        }
        response = subprocess.run(
            [value["data"]["command"], *value["data"]["args"]],
            input=json.dumps(request) + "\n",
            cwd=root,
            env=env,
            check=True,
            text=True,
            capture_output=True,
        )
        require(
            json.loads(response.stdout)["result"]["serverInfo"]["name"] == "job-harness",
            "Verification failed",
        )
        bundle = root / "private.jhb"
        run("workspace", "export", "--output", str(bundle))
        run("--root", str(root / "restored"), "workspace", "import", "--input", str(bundle))
        require((root / "restored/config/career-ledger.json").exists(), "Verification failed")
    print(
        "PASS fresh wheel: CLI, bundled resources, host selection, MCP configuration, private bundle round trip; no runtime dependencies"
    )


if __name__ == "__main__":
    verify()
