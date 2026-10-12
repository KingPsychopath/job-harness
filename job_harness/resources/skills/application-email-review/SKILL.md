---
name: application-email-review
description: Review job application emails, reconcile outcomes and next actions in the candidate's private resume ledger, and update evidence-based lessons from both rejections and positive responses. Use for repeat inbox/application reviews, not sending replies or applying for jobs.
---

# Application email review

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the canonical ledger root through bootstrap or the trusted CLI --root/current project. If unavailable, locate the user's configured career ledger rather than creating a competing one. Read its AGENTS.md, README.md, docs/project.md, docs/workflow.md and context/README.md before substantive changes. Use applications/ for process and lessons; context/ for attributed career evidence. Current baseline is approved; retired variants and exact submitted artifacts are historical evidence.

Use the [career operations guide](../../guides/career-operations.md) and career MCP/CLI. Inspect unfinished runs and processed message IDs, then start a bounded review_inbox run. Log verified mail_reviewed events using immutable message IDs, saving evidence and updating canonical tracking blocks with automatic pipeline regeneration. Preserve applications/email-review-state.json if the ledger already has that coverage checkpoint; otherwise create a dated coverage checkpoint under applications/evidence/ with queries, boundaries and message IDs. The coordinator logs actions, not a substitute mailbox coverage claim. Finish the run with actual coverage, reply queue and outcome digest. Mail tidy is a separate career-inbox-tidy workflow when authorised; this review alone does not send replies or archive messages.

## Review and reconciliation

- Inspect git status and prior dated review/evidence files to determine the last covered period and message IDs. Default to an incremental review with a one-day overlap, plus targeted backfill for missed wording or employers. If the user asks for all history, page through that requested period. State the actual scope; do not equate a keyword search with every email reviewed.
- Prefer connected email search/read tools. Discover their schemas rather than assuming names. Search beyond unread mail and beyond rejection words: include invitations, technical stages, scheduling, rescheduling, withdrawals, offers and recruiter feedback. Search relevant known aliases when needed. Page until exhausted for each declared query. Exclude irrelevant job alerts/newsletters from detailed reading, but inspect human recruiter messages. If access is missing, discover the relevant connector and continue local work while connection is pending.
- Read complete relevant messages, recursively including MIME parts. Prefer text/plain, but inspect HTML when plain text lacks the substantive content (LinkedIn/Cord notices often do). Strip transport headers, tracking/authentication tokens, unrelated recommendations and repeated quoted chains from saved extracts. Preserve enough source text, sender, received timestamp, message/thread IDs and a usable mailbox link to audit each conclusion. Keep extracts in applications/evidence/ and label them processed extracts, not originals.
- Match employer plus role/posting ID and stage against application records, receipts and exact answer packets. A generic employer email with multiple pending roles is unassigned until attributable; two generic messages do not alone prove both roles rejected. Keep newer roles separate from older rejected roles. A rejection can establish review despite a missing submission receipt; it cannot reconstruct the submitted CV/answers. A reschedule is not a rejection.
- Update current stage and next action in existing records, retain submission facts and add a dated sourced outcome. Create minimal recovered records for missing vacancies, leaving unknown IDs, submission dates and CVs unknown. Avoid repeatedly nesting old Stage text; use a dated history section. Deduplicate by immutable message ID and do not append the same event on repeat runs. Do not regress a later stage based on an older acknowledgement. Align shortlist labels only where the role match is certain.

## Learn without inventing reasons

Compare the exact submitted PDF or entered text and answers with saved requirements. Current JSON is not necessarily what was sent. Read relevant ledger and interview notes before assessing gaps. If the posting was not retained and matters, retrieve the employer source; mark expired or changed listings explicitly.

Separate actual individual feedback, generic templates, clear requirement mismatches, missing evidence/presentation and hypotheses. An employer's list of common rejection reasons may explicitly not apply. Do not blame salary, identity, personality, ATS, layout or interview performance without evidence. Never claim access to successful candidates' CVs without such material.

Include positive responses. A later invitation for a distinct role at the same company can weaken an earlier company-wide or CV-wide diagnosis. Separate skills from commercial tenure, implementation from operating at scale, and small-team leadership from Staff influence. State plausible-fit rejections as unknown; do not turn every rejection into a new learning requirement. Without a complete denominator and comparable time windows, do not report response rates or causal effects of CV wording.

Save a dated review with changed outcomes, actionable next steps, scoped comparisons and unresolved attribution. Link from applications/README.md if present, and save a checkpoint listing queries/time boundaries, pagination completion and source message IDs so another run can resume and deduplicate. Keep career facts and baseline untouched unless the user requests edits or supplied firsthand corrections require ledger intake. Preserve exact submitted bytes even when variants are retired.

## Finish

Check changed local Markdown links and, when Git is available, git diff --check for documentation changes; verify source JSON parses and message IDs are unique. Use relevant PDF verification only if changing a CV. Inspect the final diff and follow the ledger's own commit rule; no Git repository is assumed for a newly initialized ledger. Report new rejections, positive movement, important uncertainties and the review link. Give urgent interview/scheduling requests priority in the summary.

This skill does not authorise sending messages, booking meetings, applying, changing email labels/read state, accepting recording or future-contact consent, or scheduling future runs. Do those only when requested. Reading mail and updating the private ledger are the intended repeat workflow.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
