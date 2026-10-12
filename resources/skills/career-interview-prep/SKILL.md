---
name: career-interview-prep
description: Prepare a concise company and screening brief for the candidate using the exact vacancy, earlier interviews, current company sources and truthful personal examples.
---

# Prepare a company conversation

Use `context_pack` to fetch a bounded cited handoff for the exact company, project, source paths or supplied period; query gaps, rough dates and stable pack hashes stay visible. Read only missing/truncated sources with the returned `read_refs`. Fetch its specialised schema with `inspect_tools` when focused MCP is in use.

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md, [operations guide](../../guides/career-operations.md), docs/application-pipeline.md, the exact vacancy and linked brief. Start a prepare_interview run. Search relevant context/interviews, including undated notes, and useful prior prep chats when available.

Refresh material company facts from official sources and date them. Explain who buys the product, the problem it solves and what this particular role owns. Connect two sourced personal stories with their actual scope/limits. Provide a natural career-history opening, likely questions with speakable answer routes, and employer questions with hypothetical answers and implications. Use applications/briefs/template.md; revise the canonical company brief rather than creating another competing one.

Keep company self-report, recruiter account, user evidence, coaching and hypothetical answers distinct. Don't promote interview practice into CV facts. For roles with a system-design interview, begin with actors/decisions, requirements, invariants, durable data, flow, recovery and scale before tools.

Save the useful coaching and sources, link the brief in the vacancy record, log the update and finish with the brief plus material unanswered terms/process questions. Check local links and follow the ledger's commit rule for task-owned files.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
