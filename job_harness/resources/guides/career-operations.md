# Career operations with Job Harness

Job Harness combines portable skills with typed MCP/CLI functions for one private career ledger. The same callable API can serve agents or a GUI. External mail, browser and calendar actions are supplied by the host; a plan/run does not grant authority.

Start with [onboarding](job-harness-onboarding.md). Read the [version 2 contract](job-harness-contract.md) for schemas, batching, output/error shapes and submission recovery. The core uses Python's standard library; no model or vendor SDK is required.

## Operations

| Request | Operation / focused skill |
|---|---|
| Scan job emails and show who needs a reply | review_inbox / application-email-review |
| Archive resolved receipts/rejections and foreground requests | tidy_inbox / career-inbox-tidy |
| Find suitable roles on named sources | discover / career-discover |
| Apply to a bounded number of suitable roles | apply / career-apply |
| Arrange authorised recruitment meetings | schedule / career-schedule |
| Prepare a company conversation | prepare_interview / career-interview-prep |
| Save new facts, answers or preferences | update_knowledge / career-knowledge-update |
| Reconcile duplicate/stale records and unfinished attempts | reconcile / career-reconcile |

Call bootstrap with the operation and the host's capabilities. It returns settings, the focused skill, required shared reads, recovery counts and missing capabilities. Read complete relevant source records before drafting or asking the candidate a question. Capabilities are declared by the host and must still be verified live; a missing connector does not prevent independent local work.

## Efficient path

1. `bootstrap`: discover routing, configuration and recovery in one read-only call.
2. `query_batch`: fetch relevant records, approved answers and exact/employer duplicate checks together.
3. `execute_batch`: start a bounded run and chain local record updates using prior-result references. Each mutation keeps its own idempotency key. Earlier commits survive a later failure.
4. For applications, `prepare_application` freezes/reserves with one call. Use frozen bytes; call `begin_submission` separately immediately before one external Submit, then `finish_submission` with observed evidence.
5. For external tracking, `plan_adapter` combines stage and note into one provider-neutral action. The host changes only differences, verifies live state and records it with `record_adapter_result`.
6. `finish_run`: save actual scope, results, exceptions and evidence. Commit completed task-owned records under the ledger's rules.

The default public responses contain summaries and artifact references. Full source records are explicit reads; history/results are paged. `next` provides ready-to-use suggested calls, not automatic permission. Stop on structured errors and follow recovery guidance. Do not reset state or invent a fresh key to bypass a blocked attempt.

## Facts, configuration and history

Career facts/attribution live under context. Approved CV wording lives under content. Exact vacancy records and dated process outcomes live under applications. Pipeline Markdown/JSON are generated views. Shared runs/attempts/action IDs and immutable packets/receipts live under applications/operations; they are not another fact store.

Personal configuration lives in each ledger's config/career-ledger.json: timezone, primary PDF, browser preference and answer-source paths. Salary/contact/consent answers stay in attributed records. Generic workflow defaults ship with the toolkit; optional config/career-workflows.json overrides routing for one ledger. No candidate identity is hardcoded into the package or generic skills.

Legacy prose remains historical and can be stale. A generic employer receipt is insufficient to attribute an outcome across several vacancies. Older acknowledgements must not regress a newer interview stage. Booked, proposed, completed and outcome unknown remain distinct.

## Recovery

Harness calls are serialized within one instance, including GUI threads. Separate instances/processes use shared read locks and an exclusive write lock, atomic fsynced writes and write-ahead record transactions. An interrupted local update is recovered without repeating an external action. Conflicting manual edits stop recovery and must be reconciled while preserving both versions. This works for agents sharing one local checkout; separate machines/copies do not share a lock.

After begin_submission, a crash or unknown result blocks retries indefinitely. Inspect portal/mail and resolve with actual confirmation or negative evidence. An unstarted reservation may be cancelled. Replayed begin results never permit another Submit. Approved-answer changes invalidate an unstarted frozen packet.

Archive/reply/book actions remain external transactions. Save immutable provider IDs and before/after observations, then record verified events. An interrupted action must be inspected live before retrying. Recording an event cannot make an external provider transaction atomic.

Never put credentials, OTPs, cookies, private session tokens, transport headers or unrelated mail in records or packets. Preserve only necessary evidence. URLs supplied to a local tool are references; the agent/driver must actually read and verify their contents.

## Local transport

The supported transport is local stdio JSON-RPC following the [MCP tool contract](https://modelcontextprotocol.io/specification/2025-06-18/server/tools). No network listener, unattended scheduler or credential-reading tool is exposed. Trusted launch configuration chooses the root; an agent cannot switch it through a tool argument. Paths reject traversal and escaping symlinks.
