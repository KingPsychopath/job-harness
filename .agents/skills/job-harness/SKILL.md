---
name: job-harness
description: Use the native Job Harness tools with a private career workspace, or find its generic operation and development guidance.
---

# Job Harness

If setup is the task, start with [installation](../../../docs/INSTALL.md) and
[onboarding](../../../resources/guides/job-harness-onboarding.md). Use an existing
native installation or MCP connection before building this checkout. Keep the
private career workspace outside the software repository; cloning alone does
not install or connect the tools. Optional models are not required.

Start with the installed native CLI or the host's Job Harness MCP. Call `bootstrap` for the requested operation, scope and available capabilities. Read its focused skill and relevant workspace records through `read_record`; returned `.agents/skills/...` and `docs/...` aliases are embedded resources, not a requirement for filesystem symlinks.

Use `inspect_tools` for specialized schemas, `query_batch` for independent reads and `execute_batch` for ordered local changes. Follow paging and committed partial results. A run records intent, not permission. External work uses the host's connectors under the user's request; unknown submissions require outcome reconciliation.

The full generic operation skills have one source at [resources/skills](../../../resources/skills). For toolkit development, read [AGENTS.md](../../../AGENTS.md) and [development](../../../docs/DEVELOPMENT.md), then use the [maintenance skill](../../../resources/skills/harness-maintain/SKILL.md). Personal facts, credentials and private workspace bundles never belong in this software checkout.
