---
name: career-schedule
description: Arrange authorised recruitment meetings, confirm timezone and calendar availability, book employer slots, accept invitations and update the candidate's pipeline.
---

# Schedule hiring conversations

Read mentioned sources through read_record (batch related reads); generic guides/skills may return origin=toolkit. For native file tools, use the returned ledger/toolkit root for each source. Avoid repeatedly loading broad views or unrelated context.

Resolve the ledger root with bootstrap, then read its AGENTS.md, [operations guide](../../guides/career-operations.md), exact vacancy, current thread and scheduling defaults. Start a schedule run scoped to the company/record. An options question authorises inspection, not sending or booking. For requested scheduling, complete the authorised reply/booking/calendar steps without asking again about routine choices.

Read the latest employer request and later messages; don't rebook a superseded reminder. Use employer calendar links, inspect current slots and check the candidate's real calendar for conflicts before booking. Follow the candidate's selected time over an earlier general preference. State date, time and timezone explicitly using configured preferences and employer evidence; resolve named-zone versus fixed-offset ambiguities. Retrieve the fallback booking link from saved answers; actual available slots control commitments.

An availability reply is proposed or availability-sent, never booked. After booking verify the employer confirmation and exact time. Accept the existing calendar invitation; avoid creating a duplicate event. If only a proposal exists, a requested personal hold must be clearly tentative and have no employer guests. Honour saved recording/transcription preferences without inferring consent from the booking.

Send a routine confirmation or earlier-date request when authorised. Save exact sent wording/ID and before/after evidence. Log booking_confirmed, calendar_accepted and reply_sent as separate source-keyed events; set scheduled_start only with booked state and an explicit UTC offset. Update pipeline automatically through update_application. Finish with booked/proposed distinction, calendar proof and pending employer decisions. A passing date does not establish completion.

In focused MCP, load named specialised schemas with inspect_tools and run them as batch steps. Use query_batch for related reads and execute_batch for ordered local updates; pipe prior result fields with $ref. Read the version 2 data/error/next envelope. Page until next_offset is null when claiming exhaustive coverage. Use plan_adapter and record_adapter_result for requested external board sync, with actual live observations. See the [tool contract](../../guides/job-harness-contract.md) when recovering or composing batches.
