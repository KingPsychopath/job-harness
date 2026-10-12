---
name: career-ops
description: Find and run the candidate's standard career operations, inspect run history, or explain the job-search system. Use a focused career skill for a specific task.
---

# Career operations

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Use the canonical ledger root returned by bootstrap or configured through the CLI --root; do not create a competing tracker. Read its AGENTS.md and [operations guide](../../guides/career-operations.md).

Call bootstrap with the requested operation, scope and available capabilities; read only its focused skill/shared records. If MCP is unavailable, use the returned CLI fallback or job-harness --root <ledger> call <tool>, with JSON through stdin/--input. Both return the same version 2 envelope. Use query_batch for related reads, execute_batch for ordered local updates with {$ref: "step.data.field"} references, and prepare_application for packet/reservation work. See the [contract](../../guides/job-harness-contract.md) for partial failure, paging and errors. In focused MCP mode use inspect_tools to fetch needed specialized schemas, then execute those calls as batch steps. Read data/error/next; don't parse error prose or dump full state.

Use execute_batch to start a bounded run recording the actual user request; inspect unfinished runs before restarting work. A run records scope, not permission. Complete authorised work using available mail/browser tools; log verified actions with `record_event`, then `finish_run`. Use the live user's instructions and saved defaults; ask only for missing facts or tool-required confirmations. Return completed actions, pending items, evidence links and unresolved facts.

These helpers enforce local ledger rules. They do not themselves send emails, browse boards, book meetings or grant external authority. Another chat must open this repo or load these tools to share the ledger.
