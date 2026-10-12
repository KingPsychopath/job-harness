"""Private, local-only MCP stdio adapter for career_ops. No network listener."""

import argparse
import json
import sys
from pathlib import Path

from . import __version__
from .contracts import SPECS
from .service import ERROR_RECOVERY, Harness

PROTOCOL = "2025-06-18"
FOCUSED_TOOLS = {
    "bootstrap",
    "inspect_tools",
    "read_record",
    "query_batch",
    "execute_batch",
    "get_run",
    "prepare_application",
    "begin_submission",
    "finish_submission",
    "finish_run",
    "rebuild_pipeline",
}
INSTRUCTIONS = (
    "Private career ledger tools. First call bootstrap for task/capability routing and read its focused skill. Use query_batch/execute_batch for bounded local work and prepare_application to freeze/reserve. "
    "Canonical vacancy records and sourced answer files control facts; tools do not grant authority. "
    "External mail/browser/calendar actions use available connectors under the user request. "
    "Before applying: freeze packet, reserve, begin, submit once, record outcome. "
    "Never repeat a submitting/unknown attempt. Log verified actions with source IDs; finish with evidence. "
    "No credentials, OTPs or raw email transport headers belong in the ledger. "
    "In the focused profile, inspect_tools loads specialised schemas on demand; execute their local steps through query_batch/execute_batch. "
    "These tools only mutate local records. They cannot send mail, book meetings or submit applications themselves."
)


def handle(ledger, message, initialized, tool_profile="full"):
    if (
        not isinstance(message, dict)
        or message.get("jsonrpc") != "2.0"
        or not isinstance(message.get("method"), str)
    ):
        return {
            "jsonrpc": "2.0",
            "id": None,
            "error": {"code": -32600, "message": "Invalid request"},
        }, initialized
    method = message["method"]
    if "id" not in message:
        return None, initialized
    response = {"jsonrpc": "2.0", "id": message["id"]}
    params = message.get("params", {})
    if not isinstance(params, dict):
        response["error"] = {"code": -32602, "message": "params must be an object"}
        return response, initialized
    if method == "initialize":
        response["result"] = {
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "job-harness", "version": __version__},
            "instructions": INSTRUCTIONS,
        }
        return response, True
    if method == "ping":
        response["result"] = {}
    elif not initialized:
        response["error"] = {"code": -32000, "message": "Initialize first"}
    elif method == "tools/list":
        response["result"] = {
            "tools": [
                {"name": name, **spec}
                for name, spec in SPECS.items()
                if tool_profile == "full" or name in FOCUSED_TOOLS
            ]
        }
    elif method == "tools/call":
        name = params.get("name")
        if (
            tool_profile == "focused"
            and isinstance(name, str)
            and name in SPECS
            and name not in FOCUSED_TOOLS
        ):
            result = {
                "schema_version": 2,
                "ok": False,
                "data": None,
                "error": {
                    "code": "deferred_tool",
                    "message": "This known tool is deferred in the focused profile.",
                    "retryable": False,
                    "recovery": ERROR_RECOVERY["deferred_tool"],
                },
                "next": [
                    {
                        "tool": "inspect_tools",
                        "arguments": {"names": [name], "include_schemas": True},
                        "reason": "Load this specialised tool schema before batching it.",
                    }
                ],
            }
        else:
            result = ledger.call(name, params.get("arguments", {}))
        response["result"] = {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(result, ensure_ascii=False, separators=(",", ":")),
                }
            ],
            "structuredContent": result,
            "isError": not result["ok"],
        }
    else:
        response["error"] = {"code": -32601, "message": "Method not found"}
    return response, initialized


def main(argv=None, default_root=None, default_profile="focused"):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=default_root, required=default_root is None)
    parser.add_argument("--tool-profile", choices=["focused", "full"], default=default_profile)
    args = parser.parse_args(argv)
    ledger = Harness(
        args.root,
        allow_uninitialized=default_root is not None
        and args.root.resolve() == Path(default_root).resolve(),
    )
    initialized = False
    for line in sys.stdin.buffer:
        if len(line) > 1_000_000:
            response = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32600, "message": "Message too large"},
            }
        else:
            try:
                response, initialized = handle(
                    ledger, json.loads(line), initialized, args.tool_profile
                )
            except (ValueError, UnicodeDecodeError, RecursionError):
                response = {
                    "jsonrpc": "2.0",
                    "id": None,
                    "error": {"code": -32700, "message": "Parse error"},
                }
        if response is not None:
            print(json.dumps(response, ensure_ascii=True, separators=(",", ":")), flush=True)


if __name__ == "__main__":
    main()
