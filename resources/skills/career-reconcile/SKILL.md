---
name: career-reconcile
description: Audit and reconcile the candidate's application tracker, incomplete runs, duplicate records and stale outcomes using source evidence without blindly retrying submissions.
---

# Reconcile career tracking

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md and [operations guide](../../guides/career-operations.md). Call reconcile_tracker, inspect unfinished runs and start a reconcile run. Recover any interrupted local writes with rebuild_pipeline; if a manual edit conflicts, inspect both versions and resolve deliberately instead of resetting coordinator state.

Match exact employer/role/posting and receipts before fixing statuses or combining records. Legacy prose is historical evidence, not a live status check. Do not promote needs-review to applied by guessing, close another role from a generic employer email, or infer interview completion from elapsed time. Add structured tracking when sources establish it; preserve dated history.

For reserved attempts that never began, resume or cancel them within their original authorised scope. For submitting/outcome_unknown attempts, inspect employer portal/mail and finish_submission with confirmed or negative evidence. Never repeat Submit from an unfinished attempt. A partial/blocked run may retain an unknown attempt for recovery.

Use source-keyed update_application/record_event for confirmed fixes; older evidence cannot regress a stage without a sourced correction reason. Reconcile requested external board stages through the configured browser connector and log verified changes. Refresh generated pipeline, check local links/state and finish with unresolved cases, sources checked and actions taken. Coding a new tracker does not resolve missing historical evidence.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
