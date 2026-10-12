# Host observations and scoped reuse

The host reads mail, boards and forms. Job Harness checks supplied observations; it does not claim a provider read occurred. Batch related checks through `workflow_check`, then attach the observed artifact/hash and result to an existing run event. There is no second inbox or answer-memory database.

```json
{"checks":[{"id":"coverage","kind":"coverage","observation":{"source":"mail-connector","observed_at":"2026-01-02T10:00:00Z","requested_period":"2026-01-01 through 2026-01-02; recruiting query","pages_read":2,"pagination_exhausted":false,"failed_reads":["message-17"],"exclusions":["newsletters"],"checkpoint":"page-3"}}]}
```

Coverage describes the requested window and exclusions. Pagination and failed reads stay visible; an incomplete scan cannot support “everything checked.” A checkpoint is continuation data, never an auth token.

## Thread attribution

```json
{"checks":[{"id":"thread","kind":"mail","observation":{"provider":"mail-connector","thread_id":"thread-1","message_ids":["confirmation-1","invite-2"],"request_message_ids":["invite-2"],"latest_message_id":"invite-2","vacancy_url":null,"posting_id":null,"company":"Example","candidate_records":[],"requires_reply":true,"replied_to_message_ids":[],"observed_at":"2026-01-02T10:00:00Z"}}]}
```

Use provider/thread/message identity first, then posting ID and vacancy URL. A company alone cannot disambiguate two jobs. `candidate_records` are workspace-relative canonical application paths; an unmatched or ambiguous message goes to review. Evaluate the latest unanswered request, including assessment/action requests, before archiving. The check never grants archive permission or changes a stage. Store source references, hashes and relevant excerpts, rather than cloning an entire mailbox.

Every supplied posting ID/URL is compared against canonical identity fields. A contradiction or missing canonical identity stays in review. List IDs of messages needing action separately from application confirmations; a replied invitation does not become actionable because an earlier application confirmation lacks a reply. Missing request/latest-message evidence remains incomplete.

For `review_queue`, save a bounded JSON observation file under an allowed workspace text path and pass `mail_observations_path`. Its shape is:

```json
{"observed_at":"2026-01-02T10:00:00Z","items":[{"id":"thread-1","subject":"Choose an interview time","company":"Example","thread_url":"https://example.test/mail/thread-1","needs_reply":true,"needs_review":false,"attribution":"proposed_exact_identity","latest_message_id":"invite-2","request_message_ids":["invite-2"]},{"id":"thread-2","subject":"Unclear role attribution","needs_reply":null,"needs_review":true,"attribution":"ambiguous"}]}
```

`messages` is an accepted alias for `items`; maximum 1,000 objects/one MB. `needs_attention` is an accepted alias for `needs_reply`. Known resolved rows are omitted; unknown request state and ambiguous attribution stay visible. The returned source hash and observed date describe supplied evidence, not a new inbox scan. The queue also includes pending note proposals and discloses its note-review paging bound.

## Answer reuse

Structured metadata belongs alongside the canonical approved answer, not in another AI memory store. Preserve confirmation source/date, country/employer scope, polarity, validity and supersession. Legacy plain answers remain usable after reading their attribution; do not fabricate structure or confirmation dates.

```json
{"checks":[{"id":"sponsorship","kind":"answer","observation":{"question_key":"sponsorship_needed","country":"GB","employer":"Example","polarity":"negated","as_of":"2026-01-02","answer":{"question_key":"sponsorship_needed","value":false,"source_path":"applications/approved-answers.md","source_sha256":"REPLACE_WITH_ACTUAL_HASH","confirmed_at":"2026-01-01","valid_until":null,"countries":["GB"],"employers":[],"polarity":"affirmative","superseded_by":null}}}]}
```

Only an explicit boolean can invert question polarity. An answer about UK sponsorship cannot answer a Canadian sponsorship question. Employer exceptions remain scoped. A changed source, expired/uncertain validity or superseded default requires reconciliation; in-scope saved answers can be reused without repeated questions. The caller supplies the normalized property and metadata from the actual source; this check does not infer meaning or certify a claim.

## Form readback

```json
{"checks":[{"id":"form","kind":"form","observation":{"vacancy_url":"https://example.test/job/123","page_revision":"form-2","readback_revision":"form-2","observed_at":"2026-01-02T10:00:00Z","completeness_confirmed":true,"fields":[{"id":"availability","prompt":"Office days","expected":3,"observed":3,"source_kind":"saved_answer","source_path":"applications/approved-answers.md","source_sha256":"REPLACE_WITH_ACTUAL_HASH"}],"uploads":[{"path":"output/cv.pdf","sha256":"REPLACE_WITH_ACTUAL_HASH","observed_name":"cv.pdf","observed_present":true}],"parsed_claim_changes":[]}}]}
```

Inspect → resolve from approved sources → identify missing fields → fill through the host → read back current values and attachment state → prepare/reserve → begin once → submit once externally → finish with evidence.

Every resolved field names a profile, saved answer or drafted narrative source; unresolved stays explicit. A rerender invalidates old field references. After ATS CV parsing, record altered dates, duplicate employment rows, cleared uploads or other claim changes in `parsed_claim_changes`; reconcile them before preparation. Expected/observed values are compared exactly. An uploaded filename alone does not prove the employer received the file; retain the host's actual observations and the frozen packet identity.

Never include passwords, session tokens or OTPs in observations. Credential fields stay host-owned. Readback supports preparation and does not authorize submission or replace the one-submit guard. Evidence from the final confirmation differs from evidence of a filled form.

An empty observation needs explicit `completeness_confirmed:true`; false blocks readiness. This is a host attestation, not proof that a provider was inspected. Upload names default to the source basename; `expected_name` can describe a deliberately renamed frozen upload. Shape contracts are inline in `schemas workflow_check`; do not inspect implementation source to discover required fields.
