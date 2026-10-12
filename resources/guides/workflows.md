# A few useful paths

Choose the work you need now. An agent bootstraps the closest operation, inspects unfinished work and reads only relevant sources. These playbooks select existing capabilities; they hold no second tracker.

| Situation | Bring forward | Ask your agent |
|---|---|---|
| Building experience | Recent work, project links, learning and feedback | “Save this rough note and connect it to my project.” |
| Exploring a move | Preferences, evidence gaps and selected opportunities | “Help me prepare for a move using what I have already recorded.” |
| Searching | Applications, replies, interviews and next actions | “Review what needs me, then handle these suitable roles.” |
| Daily recruiting review | Replies, general recruiter conversations and progress | “Check what needs me and prepare archive candidates for my review; do not apply.” |
| Starting somewhere | New responsibilities, goals, milestones and learning | “Review my first month and help me capture useful examples.” |

These views can overlap. They never infer employment status or change facts. A selected presentation belongs to the current request, not an identity label.

## Capture recent work

Paste a rough note through `intake_note`, or import a text file with `notes import`. Keep the exact source kind and original. Link an existing project/application when known. Unknown dates, speakers and associations stay unknown. The agent may produce a cited proposal, then present selected items for review. A reviewed interpretation still differs from a verified career fact.

## Review a period

Request `context_pack` with a task, selected paths/project/company and optional period. The pack returns bounded passages, source hashes, date provenance and gaps. An explicit project anchors retrieval even when the query words differ. Occurrence and recording dates are separate; rough periods remain rough. Read complete relevant records before drafting a performance review, portfolio or CV claim.

## Prepare for a move

Read current preferences and answer sources. Retrieve relevant projects, prior interviews and supplied writing examples in one `query_batch`. Show missing facts only when they block the requested output. Reuse confirmed answers within their scope; do not turn a generated inference about personality into a preference.

## Prepare an application

Bootstrap `apply`, check duplicates and read the exact vacancy. Use `workflow_check` for scoped answers and current form readback; `preview_packet` shows exact artifacts and changes. `prepare_application` freezes/reserves once. After a fresh check, begin once, submit once through the host, and finish with actual confirmation or `outcome_unknown`. A repeated begin cannot grant another submit. Saved authorization carries through ordinary preparation; tools do not grant new authority.

## Review the day

Use `daily_check` or `jh daily check` for saved state, or supply fresh host mail observations for an actual scanned period. Bring general recruiter conversations forward even without a vacancy record. `daily_update` records reviewed evidence and exact role changes; it does not apply or send replies. Archive work needs a frozen human-confirmed selection, fresh readback, a one-shot host action and verified outcomes. [Daily guide and task prompts](daily-review.md)

## Prepare for or debrief an interview

Bootstrap `prepare_interview`. Retrieve the application and relevant earlier notes with `context_pack`. The agent explains the company, selects two truthful project stories and drafts likely questions. Afterward, import your notes, a supplied transcript or a provider summary with its actual source kind. Preserve original and reviewed versions side by side. Proposed next steps are suggestions until accepted; an accepted next-step note does not send an email or book a meeting.

## Review progress in a new role

Capture supplied responsibilities and milestones; link projects without inventing ownership or outcomes. Use a period context pack to draft a review or a list of evidence gaps. Export a selected document through `document render`; the rest of the private workspace stays separate.

## One working loop

Read → retrieve → decide → prepare → act once through the host → verify → retain evidence.

Use `query_batch` for independent reads and `execute_batch` for sequential local changes with exact result references. A partial batch reports committed steps; resuming starts from those results. `review_queue` combines unresolved local work with supplied recruiting observations; it cannot prove a mailbox scan happened. See [host observations](host-observations.md) and [contracts](job-harness-contract.md).

The toolkit loop is separate: reproduce friction with fictional fixtures, improve code or a focused skill, verify the contracts and package, then release the software. Development findings never become candidate facts.
