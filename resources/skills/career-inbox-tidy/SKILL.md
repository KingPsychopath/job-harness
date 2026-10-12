---
name: career-inbox-tidy
description: Archive resolved application confirmations and rejections, and foreground unanswered recruiting emails without losing scheduling requests.
---

# Tidy application mail

For each related scan/thread group, fetch `workflow_check` with `inspect_tools`; its inline `coverage` and `mail` schemas preserve exhausted pages, exact posting conflicts and request-bearing/latest message IDs. Use `review_queue` with a bounded supplied observation file to retain unanswered, unknown or ambiguous mail and pending note reviews. These are local checks, not inbox actions.

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md and [operations guide](../../guides/career-operations.md). Start a tidy_inbox run within the requested dates or current inbox. Read prior email checkpoints and action IDs; use the available mail connector.

Read each candidate message in full and its later thread before changing labels. An acknowledgement containing a requested assessment, booking or declaration is actionable. Resolve rejection attribution by exact vacancy; a rejection for one posting doesn't close all applications at that company. Treat reminders as superseded only with later evidence or the candidate's explicit report.

Archive individual resolved message IDs, never whole threads containing a later request. Preserve existing job-search labels and unread status unless the user asked otherwise. Star/label current reply and booking requests using existing meaningful labels; avoid creating duplicate labels. Do not send replies just because tidying was authorised.

For every label change save a sanitized evidence JSON under applications/evidence with immutable message ID, thread ID, mailbox link, classification, original labels and verified resulting labels. That makes archive changes reversible. Use `list_events` to skip an already recorded change, then `record_event` with kind mail_archived and key `mail:<provider>:<message-id>:archive:<disposition>`. For restored/foregrounded mail use a separate event key and summary. Update exact vacancy tracking if its stage changed, without regressing from newer evidence.

Check archived scheduling messages when requested. Report the searched period and exhausted queries, current response queue with email links, and unresolved attribution. Finish the run; follow the ledger's commit rule for task-owned evidence/records. Invoke email-review separately only if broader learning/outcome review is needed.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
