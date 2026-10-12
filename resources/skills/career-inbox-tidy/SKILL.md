---
name: career-inbox-tidy
description: Archive resolved application confirmations and rejections, and foreground unanswered recruiting emails without losing scheduling requests.
---

# Tidy application mail

Use `daily_check` for the saved queue and supplied fresh mail observations; it is read-only. For specialized scan/thread checks, fetch `workflow_check` with `inspect_tools`. Its `coverage` and `mail` shapes preserve exhausted pages, exact posting conflicts and request-bearing/latest message IDs. [Daily review](../../guides/daily-review.md) explains the frozen human-reviewed archive lifecycle. These local tools do not read or change the inbox themselves.

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md and [operations guide](../../guides/career-operations.md). Start a tidy_inbox run within the requested dates or current inbox. Read prior email checkpoints and action IDs; use the available mail connector.

Read each candidate message in full and its later thread before changing labels. An acknowledgement containing a requested assessment, booking or declaration is actionable. Resolve rejection attribution by exact vacancy; a rejection for one posting doesn't close all applications at that company. Treat reminders as superseded only with later evidence or the candidate's explicit report.

Prepare a bounded candidate plan with exact message IDs, links, reasons and current thread evidence. Present it for human review and freeze only the confirmed selected messages. Before the connected host archives each selected message, read back its current message/thread state; changes return it to review. Archive individual resolved message IDs, never whole mixed threads containing a later request. Preserve unrelated labels and unread status unless the user requested otherwise. Show current reply/booking requests prominently; change labels only within the user's scope. Do not send replies just because tidying was authorised.

For each confirmed archive, retain the frozen plan/selection identity and sanitized evidence with immutable message/thread IDs, mailbox link, classification, original labels and verified resulting labels. Finish its local action state only after the host verifies the outcome. A timeout remains unknown until inspected; never blindly repeat it. For other authorized label changes, use `list_events` to skip recorded actions and `record_event` with a stable message/action key. Update exact vacancy tracking with evidence without regressing newer stages. General recruiter conversations remain visible even without an application record; attribution and archive eligibility are separate.

Check archived scheduling messages when requested. Report the searched period and exhausted queries, current response queue with email links, and unresolved attribution. Finish the run; follow the ledger's commit rule for task-owned evidence/records. Invoke email-review separately only if broader learning/outcome review is needed.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
