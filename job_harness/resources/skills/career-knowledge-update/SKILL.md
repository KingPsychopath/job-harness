---
name: career-knowledge-update
description: Incorporate the candidate's new career facts, application answers, preferences or interview notes into the shared evidence ledger with attribution and reusable defaults.
---

# Update shared career knowledge

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md, [operations guide](../../guides/career-operations.md) and docs/workflow.md. Start an update_knowledge run. Identify whether each supplied item is a firsthand fact, preference, interview recollection, recruiter feedback, coaching or hypothesis.

Save/read interview notes under context/interviews and link the exact vacancy. Update confirmed role/project/profile evidence with source/date; preserve unknowns and corrections. A calendar event does not prove attendance and a rejection template does not prove why. Career facts live under context, process facts under applications, coaching in briefs/practice.

For a new reusable direct application answer update the canonical answer/profile sources returned in settings.answer_sources with date, attribution, allowed question scope and unresolved limits. For supporting career facts also update the relevant context record. Draft from existing evidence instead of repeatedly prompting for the same story. Use get_approved_answer and complete source records before asking.

Do not alter the CV selection unless requested; substantive approved baseline changes need the ledger's claim map (if present) and its verification workflow. Log knowledge_updated with paths to the durable evidence, rebuild pipeline if the process changed, finish with what changed and what remains uncertain, and follow the ledger's commit rule for task-owned files. Chat memory is not canonical; do not save credentials or authentication codes.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
