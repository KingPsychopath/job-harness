# Daily career check and update

A daily review brings forward replies, recruiting conversations, interviews and unfinished local work. It can use an existing connected agent; no new model or mailbox is required. Job Harness checks and records supplied evidence. The host reads email and performs any approved external action.

## Two small commands

```sh
jh --root /absolute/private/workspace daily check
jh --root /absolute/private/workspace daily check --input daily-check.json
jh --root /absolute/private/workspace daily check --observations applications/evidence/daily-mail.json --limit 20
jh --root /absolute/private/workspace daily update --input daily-update.json
```

Both commands return the engine's version 2 JSON envelope for piping. A JSON file works on Windows, macOS and Linux; omit `--input` to supply JSON on stdin. `daily check` with no JSON reads only saved workspace state. It does not connect an inbox or claim a fresh scan. `daily update` needs the reviewed evidence/actions required by its schema and performs local updates only.

Check flags `--observations`, `--source-sha256`, `--offset` and `--limit` are optional conveniences for the equivalent JSON fields. Observation paths are relative to the private workspace. If a flag and JSON input specify the same field, their values must agree; a conflict fails rather than silently overriding the input. Follow returned paging before describing the entire queue.

For a fresh read-only host check without saving a file first, supply the complete snapshot in the JSON argument `mail_observations`. Use either inline observations or `mail_observations_path`, never both. Inline input is checked in memory and not retained. To record it later, deliberately save the sanitized snapshot under an allowed workspace path and recheck that file's byte hash before `record_review`; an inline object's hash is not interchangeable with differently formatted JSON file bytes.

Inspect exact inputs with `jh schemas daily_check` and `jh schemas daily_update`. In focused MCP, load these with `inspect_tools`; the calls behave the same as the CLI. Bootstrap the matching operation and read `.agents/skills/career-daily-review/SKILL.md` through `read_record`.

With the current source build, try the [fictional rehearsal](../../examples/daily-review/README.md). It records two supplied messages in a new external workspace and archives nothing. The unchanged v0.2.0 archive predates these daily commands. Existing valid workspaces can be selected without resetting them; missing daily routes in an older workflow configuration use embedded defaults.

## Use the agent and connections you already have

Saved-state check:

```text
Check my saved career work with Job Harness. Show unresolved submissions,
interviews, replies and recruiting conversations already recorded. Do not claim
you scanned my inbox or change records, labels or applications.
```

Fresh check through a mail-connected host:

```text
Check recruiting mail since my last verified checkpoint, including general
recruiter outreach and ongoing conversations. Read complete relevant messages
and the latest thread, record actual coverage, and show what needs my reply.
Keep exact role attribution separate from archive eligibility. Prepare archive
candidates for review; do not archive, send replies or apply.
```

Reviewed update:

```text
Update my saved progress from these verified observations. Attribute outcomes to
the exact role, keep general recruiter conversations visible, and preserve
unknowns. Show the archive plan for human review, then use only its confirmed
selected message IDs through my mail connection and record verified outcomes.
Do not send replies, book meetings or create new applications.
```

These are task prompts, not a schedule or extra permission. An unattended run can check or prepare a plan; it must stop at the required human review for external changes. Your host controls its connector access and scheduling.

## What counts as a fresh check

Supply the source, observed time, requested period/queries, exhausted pages, failed reads and exclusions. Retain immutable message/thread IDs, usable mail links and enough source text to inspect a decision. Keep request-bearing/latest message IDs separate from confirmation messages and compare them with actual replies. [Host observation shapes](host-observations.md)

An incomplete scan remains incomplete. A saved checkpoint or local queue does not prove that new mail was read. Reuse prior verified message actions to avoid processing the same result twice, but read current thread state before changing labels.

The supplied snapshot is bounded to one MB and 1,000 rows. Save it under `applications/evidence/` or an allowed `context/` JSON path. Its version 1 fields are `schema_version`, `provider`, `mailbox_id`, `observed_at`, `coverage` and `items`. Each row names a single `message_id` plus its `thread_id`, all observed `thread_message_ids`, `latest_message_id`, bounded sanitized original `text` and its `source_sha256`, category, reply state, request/reply message IDs, full-read/thread-completeness observations and provider labels. Optional company, record path, posting URL/ID, summary and mail link support attribution. Set `supports_message_archive` from the host's real capability, and `inbox_label` to that provider's actual inbox label. Do not invent provider identities, timestamps, content hashes or completeness. The tool checks that the exact text matches its digest.

Categories include `rejection`, `application_confirmation`, `reply_request`, `information`, `correspondence` and `unknown`; correspondence need not belong to a vacancy. A supplied source-text hash records the host's observation, not independent proof of a connector read. Never include passwords, access tokens, sessions or OTPs. The authoritative nested shapes are in `schemas daily_check`/`schemas daily_update`.

## Recruiting conversations are their own work

A recruiter asking about availability, a potential role or your preferences can need a reply before there is a vacancy record. Keep that message in the communications queue; do not invent an application to represent it. If a role is known, retain its canonical path and posting/URL identity. A company-only or contradictory match stays ambiguous and cannot update one of its applications by guesswork.

Application attribution does not decide whether a message can be archived. An exact role match can still contain an unanswered request. A general conversation can be resolved, but needs sufficient current evidence and an explicitly reviewed archive selection before a host changes labels.

Retaining communications does not promote an application stage automatically. For a requested exact role update, read the canonical record and use existing `update_application` with its current hash and reviewed evidence. Keep newer stages and unknown submissions intact. This local correction is separate from `daily_update` and does not create a new application or authorize a submission.

## Archive a reviewed selection

First prepare candidates with exact message IDs, links, reasons and the observed thread state. Present that set for human review and freeze the confirmed selection. Read current state again before acting; a changed message/thread returns to review. The host archives only the exact selected target set and preserves unrelated labels/read state. Prefer message-level actions; a thread-only host needs the fully observed, unchanged membership and every member eligible within the reviewed selection. Never widen a selected message into an unreviewed mixed thread.

Approval metadata must describe an actual human-reviewed selection. Continue within that confirmed selection without asking again unless fresh evidence changes it. Call `begin_archive` separately; it cannot run inside `execute_batch`. This keeps a retried local batch from granting another external archive attempt.

Approval fields preserve the host's supplied attribution; they do not authenticate a human identity. The host must obtain the actual reviewed selection rather than fill those fields with an agent's own decision.

Retain source and plan identities, original labels, action IDs and verified resulting labels. Record a timeout or uncertain response as unknown and inspect the host before retrying. Recording a local plan, approving it or opening a connection does not prove an archive occurred. No email reply, interview booking or new application is part of this operation.

Use a bounded `daily_update` or `tidy_inbox` run and a distinct idempotency key for each different action. The `daily_update` action selects the local step:

| Action | What it records or allows |
|---|---|
| `record_review` | Retains the supplied snapshot identified by path/hash and the resulting review; it does not infer or change application stages. |
| `approve_archive` | Freezes human-confirmed `selected_action_ids` from the identified plan/hash with `approved_by` and `approval_evidence`; it queues work, without archiving mail. |
| `withdraw_archive` | Records a human's withdrawal of still-queued selected work with a reason and attributed evidence; started/unknown work requires reconciliation. |
| `begin_archive` | Checks fresh path/hash observations against that selection and returns one-shot host permits. A changed thread returns to review. |
| `record_archive_outcome` | Saves each action's observed outcome, message IDs, resulting labels and evidence after the host acts. |
| `reconcile_archive` | Resolves an uncertain action from subsequent host evidence; it does not grant another blind archive attempt. |

For `record_review`, supply `action`, `run_id`, `idempotency_key`, `mail_observations_path` and `expected_source_sha256`. Approval supplies the same action/run/key identity plus `plan_id`, `expected_plan_sha256`, selected action IDs and human approval attribution. Beginning also supplies fresh observation path/hash. In focused MCP, call the separately advertised `begin_mail_archive` tool directly with those begin fields and no `action` field. CLI/full-profile `daily_update` with `action=begin_archive` uses the same boundary. Neither begin form may run in a batch. Outcome recording supplies `plan_id` and `results`, each with `action_id`, `outcome` (`archived`, `failed` or `outcome_unknown`), `observed_message_ids`, `observed_labels` keyed by message ID and evidence links/paths. Read the current schemas before composing a batch; use identities returned by previous results, not guessed IDs.

`daily_check` returns the supplied/saved source identity in `data.observations` and its proposed `data.archive_plan`. `record_review` preserves that plan and returns its `plan_id`/`plan_sha256` for subsequent selection. In a batch, references include the envelope's `data`, such as `check.data.observations.sha256` and `run.data.run_id`. A recorded plan is still unapproved; its proposed action IDs are not permits. The five-minute fresh-read requirement applies to beginning an archive, not to retaining an accurately dated historical review.

Communications can merge multiple mailboxes. `observations.sources` retains per-provider/mailbox coverage and freshness, with explicit gaps and bounded source counts. The proposed archive plan belongs to one explicit mailbox and its newest selected scan; a merged queue never authorizes cross-mailbox archiving. Filter a complete thread with `provider`, `mailbox_id` and `thread_id` when normal paging reports a thread boundary. Partial newer observations keep older unresolved requests visible.

`record_review` preserves exact snapshot bytes under content-hashed communications evidence. Known archive outcomes require `readback_source_path`/`readback_source_sha256` identifying retained host evidence with the exact action/provider/mailbox/thread identity, observation time, target IDs and labels. Readback must be after begin and not in the future. `archived` records the observed absence of the inbox label; it does not establish who caused that state. Explicit `label_changes:true` permits disclosed unrelated label drift, retained as `label_differences` alongside the hashed readback. Never alter original before-label evidence to make a result appear clean.

For withdrawal, supply the exact queued plan/hash and selected IDs, actual human `approved_by`/`approval_evidence`, and `reason`. An already archived action with the same identity cannot currently be re-approved, even if someone moves it back to the inbox. Preserve that guard and report the limitation; do not reset coordination or manufacture a changed definition to bypass it.

## Finish with the useful queue

Report what was actually scanned, the messages needing a response, general recruiting conversations, verified role changes, confirmed archive outcomes and uncertainty. Include usable email links and next actions. Keep local recording distinct from host actions and retain unfinished work for the next check.

For a hiring conversation, ask: “Brief me for this company and exact role using our recruiting messages, submitted CV/answers and original interview notes. Explain what the company does, how my evidence fits and what remains unknown.” The `prepare_interview` workflow batches those existing sources with `context_pack` and filtered `daily_check`; a lost posting or missing submitted CV remains a visible gap.

The focused catalog exposes 12 entry tools, including the two one-shot boundaries. Specialized daily reads and local updates remain discoverable with `inspect_tools` and composable in batches. `begin_mail_archive` is intentionally separate so a successful batch never leaves an external action permit buried in its results.
