---
name: career-daily-review
description: Check daily career progress and recruiting communications, prepare a human-reviewed email tidy plan, and record verified local updates. Use for a daily check or update, not new applications or unsolicited replies.
---

# Review the day

Bootstrap `daily_check` or `daily_update` for the user's requested scope, then read the private workspace's instructions. Use the matching daily tool or CLI command and its current schema; [daily review](../../guides/daily-review.md) describes inputs, prompts and archive review. Reuse existing answers and evidence. Load only sources relevant to the queue.

A daily check is read-only. Without fresh host observations, report saved work and say the inbox was not scanned. With a connected mail host, read complete relevant messages and their latest thread, page the declared query window, and supply dated coverage and immutable message identities. A saved observation is evidence of that earlier read; do not present it as a current scan. Failed reads, unexhausted pages and unreadable sources remain visible.

Include recruiter outreach, role discussions, invitations, assessments, booking changes, offers and feedback even when no application exists. Preserve general or ambiguous messages in the communications queue. Attribute application outcomes only by exact role/posting identity; company alone does not close all its roles. Role attribution and archive eligibility are separate decisions.

Bring unanswered requests forward with usable email links. A confirmation containing an assessment, booking or declaration is still actionable. Compare request-bearing and latest message IDs with actual replies; an old confirmation lacking a reply does not reopen an answered invitation. Do not infer completion from a star, label, unread flag or an earlier snapshot.

A daily update retains supplied communications and their archive-review state; it does not infer application stages, apply for jobs, send replies or book meetings. If the user requests an exact role change, use the existing `update_application` tool with reviewed source evidence and the current record hash separately. Preserve newer stages and unknown submission outcomes. General recruiting communications do not require inventing an application record. Report committed local changes and remaining uncertainties separately.

For tidying, prepare a bounded archive candidate set and present exact messages, reasons and source freshness for human review. Freeze the reviewed plan and act only on confirmed selected message IDs through the user's connected mail host. Attribute actual human approval; do not manufacture approval evidence. Once confirmed, continue within that selection without asking again unless the fresh read changes it. No tool suggestion is archive permission. Exclude unanswered requests, ambiguous classification and messages without sufficient current thread evidence. Never archive a whole mixed thread because one old message is resolved.

Before each approved archive, read back the selected message/thread and compare it with the frozen source. If it changed, return it for review. Call `begin_mail_archive` directly in focused MCP (or `daily_update` with `action=begin_archive` through CLI/full mode), separately from `execute_batch`; one-shot permits must be consumed only by the following authorized host action. Save original labels and hashed readback evidence with message/thread IDs and a usable link. Recorded outcomes describe observed final inbox state, not proof of who caused it. Disclose unrelated label drift; do not invent causation. A human may withdraw still-queued selections; begun or unknown actions require reconciliation. Do not blindly repeat an uncertain label change.

Finish with actual scan coverage, the reply/action queue, general recruiting messages, verified tracking changes, selected archive outcomes and unresolved work. Do not add new applications to a daily update. Discovery/application, sending replies and scheduling use their focused workflows only when the user requests those actions.
