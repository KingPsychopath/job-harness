---
name: career-interview-prep
description: Prepare a concise company and screening brief for the candidate using the exact vacancy, earlier interviews, current company sources and truthful personal examples.
---

# Prepare a company conversation

For “Brief me for COMPANY/ROLE”, bootstrap `prepare_interview`, read the private workspace's instructions and resolve the exact canonical vacancy with `find_application`. Distinguish multiple roles at one company. Start a bounded preparation run; this does not book or send anything.

Batch related reads through `query_batch`: `read_record` for the exact application and linked brief, `get_packet`/`preview_packet` for its frozen submission when available, `context_pack` for relevant project stories and original prior interview notes, and `daily_check` filtered by `company`/`record` for retained recruiting communications. Read linked communications evidence and its coverage; saved history is not a fresh inbox scan. Include useful supplied earlier prep conversations when available, without treating coaching as facts.

Use the actual submitted packet and answers when discussing what the employer has seen. Read the packet's frozen sources and their hashes rather than substituting the current CV or approved-answer file. If the submitted CV, answers, vacancy or earlier mail was not retained, state that gap; do not reconstruct an exact submission from today's baseline. For source passages, keep occurrence/recording dates, rough periods and pack gaps visible; read complete relevant originals through returned `read_refs` before making a claim.

Generic guides/skills returned by `read_record` may have `origin=toolkit`; installation does not require symlinks. Fetch specialized schemas with `inspect_tools` when needed. Use the [operations guide](../../guides/career-operations.md) and any existing private preparation template; avoid loading unrelated broad views or creating a competing company history.

Refresh material company facts from official sources and date them. Explain who buys the product, the problem it solves and what this particular role owns. Connect two sourced personal stories with their actual scope/limits. Provide a natural career-history opening, likely questions with speakable answer routes, and employer questions with hypothetical answers and implications. Use `applications/briefs/template.md` when present; revise the linked canonical company brief instead of creating another competing one. No dedicated company-brief tool or new tracker is required.

Keep company self-report, recruiter account, user evidence, coaching and hypothetical answers distinct. Don't promote interview practice into CV facts. For roles with a system-design interview, begin with actors/decisions, requirements, invariants, durable data, flow, recovery and scale before tools.

Save the useful coaching and sources, link the brief in the vacancy record, log the update and finish with the brief plus material unanswered terms/process questions. Check local links and follow the ledger's commit rule for task-owned files.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
