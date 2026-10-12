"""Human-selected local defaults; no credentials and no MCP workspace switching."""

import json
import os
import sys
from pathlib import Path

from .ledger import atomic_write, encoded


def settings_path():
    base = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
    return base / "job-harness/settings.json"


def default_workspace():
    if os.environ.get("JOB_HARNESS_WORKSPACE"):
        return Path(os.environ["JOB_HARNESS_WORKSPACE"]).expanduser()
    path = settings_path()
    if not path.exists():
        return None
    data = json.loads(path.read_text())
    if (
        not isinstance(data, dict)
        or data.get("schema_version") != 1
        or not isinstance(data.get("workspace"), str)
    ):
        raise ValueError("Invalid local Job Harness settings; supply --root explicitly.")
    return Path(data["workspace"])


def configure(workspace):
    from .bundles import assert_external_workspace
    from .service import Harness

    root = Path(workspace).expanduser().resolve()
    assert_external_workspace(root)
    result = Harness(root).call("bootstrap", {})
    if not result["ok"]:
        raise ValueError("Choose an initialized, readable private workspace.")
    path = settings_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.parent.chmod(0o700)
    atomic_write(path, encoded({"schema_version": 1, "workspace": str(root)}))
    path.chmod(0o600)
    return {
        "workspace": str(root),
        "settings": str(path),
        "note": "Host-local path only. Credentials remain in the host's credential store.",
    }


def connection(workspace, profile="focused"):
    root = Path(workspace).expanduser().resolve()
    if not (root / "config/career-ledger.json").is_file():
        raise ValueError("Choose an initialized private workspace before connecting a client.")
    # Framework launchers may spell sys.executable differently on macOS. Keep
    # the selected virtual environment; resolving its symlink loses the venv.
    interpreter = Path(sys.prefix) / "bin/python"
    command = (
        str(interpreter)
        if sys.prefix != sys.base_prefix and interpreter.is_file()
        else sys.executable
    )
    return {
        "command": command,
        "args": ["-P", "-m", "job_harness", "--root", str(root), "mcp", "--tool-profile", profile],
    }
