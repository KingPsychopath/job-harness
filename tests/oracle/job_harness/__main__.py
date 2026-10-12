"""python -m job_harness: init, schemas, call, mcp, export. JSON in, JSON out."""

import argparse
import json
import sys
from pathlib import Path

from . import __version__
from .contracts import SPECS
from .service import Harness


def emit_call(root, tool, path=None, *, allow_uninitialized=False):
    try:
        raw = path.read_text() if path else (sys.stdin.read() if not sys.stdin.isatty() else "")
        args = json.loads(raw or "{}")
    except (OSError, ValueError):
        result = {
            "schema_version": 2,
            "ok": False,
            "data": None,
            "error": {
                "code": "invalid_arguments",
                "message": "Input must be a JSON object.",
                "retryable": False,
                "recovery": "Pass JSON through stdin or --input.",
            },
            "next": [],
        }
    else:
        result = Harness(root, allow_uninitialized=allow_uninitialized).call(tool, args)
    print(json.dumps(result, ensure_ascii=True, separators=(",", ":")))
    if not result["ok"]:
        raise SystemExit(1)


def emit_setup(action, value):
    """Setup failures are compact JSON too; never leak an OS traceback/path."""
    try:
        data = action(value)
        result = {"schema_version": 2, "ok": True, "data": data, "error": None, "next": []}
    except (ValueError, OSError) as error:
        result = {
            "schema_version": 2,
            "ok": False,
            "data": None,
            "error": {
                "code": "invalid_arguments" if isinstance(error, ValueError) else "io_error",
                "message": str(error)
                if isinstance(error, ValueError)
                else "Setup storage operation failed.",
                "retryable": False,
                "recovery": "Inspect the destination; preserve existing data and choose a new/empty directory.",
            },
            "next": [],
        }
    print(json.dumps(result, ensure_ascii=True, separators=(",", ":")))
    if not result["ok"]:
        raise SystemExit(1)


def legacy_cli():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("tool")
    parser.add_argument("--input", type=Path)
    args = parser.parse_args()
    emit_call(args.root, args.tool, args.input)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="version", version="%(prog)s " + __version__)
    parser.add_argument(
        "--root", type=Path, help="Trusted private ledger, separate from toolkit installation"
    )
    sub = parser.add_subparsers(dest="command", required=True)
    call = sub.add_parser("call")
    call.add_argument("tool")
    call.add_argument("--input", type=Path)
    schemas = sub.add_parser("schemas")
    schemas.add_argument("tool", nargs="?", choices=sorted(SPECS))
    config = sub.add_parser("config-schema")
    config.add_argument("kind", choices=["ledger", "workflows"])
    mcp = sub.add_parser("mcp")
    mcp.add_argument("--tool-profile", choices=["focused", "full"], default="focused")
    sub.add_parser("init")
    configure = sub.add_parser("configure", help="Select a local default workspace")
    configure.add_argument("--workspace", type=Path, required=True)
    connect = sub.add_parser("connect", help="Print MCP launch configuration without credentials")
    connect.add_argument("target", choices=["mcp"])
    connect.add_argument("--tool-profile", choices=["focused", "full"], default="focused")
    workspace = sub.add_parser("workspace", help="Private workspace bundle import/export")
    actions = workspace.add_subparsers(dest="workspace_action", required=True)
    bundle_export = actions.add_parser("export")
    bundle_export.add_argument("--output", type=Path, required=True)
    bundle_export.add_argument("--kind", choices=["workspace", "profile"], default="workspace")
    bundle_import = actions.add_parser("import")
    bundle_import.add_argument("--input", type=Path, required=True)
    args = parser.parse_args()
    if args.root is None and args.command in {"call", "mcp", "workspace", "connect"}:
        from .host import default_workspace

        try:
            args.root = default_workspace()
        except (OSError, ValueError):
            parser.error("Invalid local settings; supply --root explicitly.")
    if args.command in {"init", "call", "mcp", "workspace", "connect"} and args.root is None:
        parser.error("--root is required; toolkit and private ledger are separate")
    if args.command == "call":
        emit_call(args.root, args.tool, args.input)
    elif args.command == "schemas":
        print(json.dumps(SPECS[args.tool] if args.tool else SPECS, indent=2))
    elif args.command == "config-schema":
        from .configuration import LEDGER_SCHEMA, WORKFLOW_SCHEMA

        print(json.dumps(LEDGER_SCHEMA if args.kind == "ledger" else WORKFLOW_SCHEMA, indent=2))
    elif args.command == "mcp":
        from .mcp import main as serve

        serve(["--root", str(args.root), "--tool-profile", args.tool_profile])
    elif args.command == "init":
        from .workspace import initialize

        emit_setup(initialize, args.root)
    elif args.command == "configure":
        from .host import configure

        emit_setup(configure, args.workspace)
    elif args.command == "connect":
        from .host import connection

        emit_setup(lambda root: connection(root, args.tool_profile), args.root)
    elif args.command == "workspace":
        from .bundles import export_bundle, import_bundle

        if args.workspace_action == "export":
            emit_setup(lambda root: export_bundle(root, args.output, args.kind), args.root)
        else:
            emit_setup(lambda root: import_bundle(root, args.input), args.root)


if __name__ == "__main__":
    main()
