---
name: job-harness
description: Use the native Job Harness tools with a private career workspace, or find its generic operation and development guidance.
---

# Job Harness

Start with the installed native CLI or the host's Job Harness MCP. Call `bootstrap` for the requested operation, scope and available capabilities. Read its focused skill and relevant workspace records through `read_record`; returned `.agents/skills/...` and `docs/...` aliases are embedded resources, not a requirement for filesystem symlinks.

Use `inspect_tools` for specialized schemas, `query_batch` for independent reads and `execute_batch` for ordered local changes. Follow paging and committed partial results. A run records intent, not permission. External work uses the host's connectors under the user's request; unknown submissions require outcome reconciliation.

The full generic operation skills have one source at [resources/skills](../../../resources/skills). For toolkit development, read [AGENTS.md](../../../AGENTS.md) and [development](../../../docs/DEVELOPMENT.md), then use the [maintenance skill](../../../resources/skills/harness-maintain/SKILL.md). Personal facts, credentials and private workspace bundles never belong in this software checkout.
