"""Optional independent SDK/schema integration check, using a disposable empty ledger."""

import asyncio
import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
try:
    from jsonschema import Draft202012Validator
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client
except ImportError:
    raise SystemExit(
        "QA-only dependencies missing: install mcp and jsonschema in a disposable environment."
    )

from job_harness.ledger import ROOT
from job_harness.workspace import initialize


async def verify(profile):
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary) / "ledger"
        initialize(root)
        params = StdioServerParameters(
            command=sys.executable,
            cwd=str(ROOT),
            args=["-m", "job_harness", "--root", str(root), "mcp", "--tool-profile", profile],
        )
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                ready = (await session.initialize()).model_dump(by_alias=True)
                assert ready["serverInfo"]["name"] == "job-harness"
                tools = {
                    t.name: t.model_dump(by_alias=True) for t in (await session.list_tools()).tools
                }
                for spec in tools.values():
                    Draft202012Validator.check_schema(spec["inputSchema"])
                    Draft202012Validator.check_schema(spec["outputSchema"])

                async def call(name, arguments):
                    result = (await session.call_tool(name, arguments)).model_dump(by_alias=True)
                    value = result["structuredContent"]
                    Draft202012Validator(tools[name]["outputSchema"]).validate(value)
                    assert result["isError"] == (not value["ok"])
                    assert json.loads(result["content"][0]["text"]) == value
                    return value

                schemas = await call(
                    "inspect_tools", {"names": ["find_application"], "include_schemas": True}
                )
                assert schemas["data"]["tools"][0]["inputSchema"]["type"] == "object"
                assert (await call("bootstrap", {"operation": "apply", "capabilities": []}))["ok"]
                batch = await call(
                    "query_batch",
                    {
                        "steps": [
                            {
                                "id": "answers",
                                "tool": "get_approved_answer",
                                "arguments": {"query": "answers"},
                            },
                            {
                                "id": "roles",
                                "tool": "find_application",
                                "arguments": {"query": "Acme"},
                            },
                        ]
                    },
                )
                assert batch["data"]["completed"] == 2
                denied = await call("read_record", {"path": "/etc/passwd"})
                assert denied["error"]["code"] == "path_denied"
                print(
                    f"PASS independent MCP SDK ({profile}): initialize, {len(tools)} schemas, deferred discovery, bootstrap, batch, typed error and text/structured parity"
                )


if __name__ == "__main__":
    for profile in ["focused", "full"]:
        asyncio.run(verify(profile))
