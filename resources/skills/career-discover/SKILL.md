---
name: career-discover
description: Find suitable roles for the candidate on named job boards, tracked lists or a broader requested search, checking shared application history before saving a shortlist.
---

# Discover suitable roles

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md, [operations guide](../../guides/career-operations.md), approved answers and profile decisions. Start a discover run with source scope and requested role count. Broad discovery can search relevant boards and official employer listings; report which sources were actually searched rather than claiming the whole market.

Use available browser tools for authenticated boards under the configured preference and user request. Check current official role details, captured date, posting ID, location/office cadence and base compensation; unknown salary is an open question, not proof of meeting the floor. Apply compensation preferences and role-specific exceptions from the configured answer/profile records. Do not invent new hard filters.

Use `find_application` and `check_duplicate` for company, URL and posting ID; inspect company matches for aliases, limits and cooldowns. A listing on two boards is one vacancy. Save new eligible vacancies with `create_application`; retain source snapshots/links, fit reasons, concerns and evidence. Existing entries get a sourced update rather than a second record. Distinct postings at one employer remain distinct.

Finish with a deduplicated shortlist, excluded roles with concrete reasons, sources searched and unknowns. Discovery alone does not authorise applications or recruiter messages. If applying was also requested, continue using career-apply with a bounded apply run.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
