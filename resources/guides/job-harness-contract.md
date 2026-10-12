# Job Harness tool contract — version 2

The Rust API, MCP and CLI share `Engine::open(root)?.call(tool, &arguments)`. The reusable entry points require an initialized config/career-ledger.json at that root; a typo returns missing_resource without creating a competing ledger. Every call returns:

```json
{"schema_version":2,"ok":true,"data":{},"error":null,"next":[]}
```

Each tool has a strict input schema and its own output schema. `job-harness schemas TOOL` prints both. Results are validated before returning. `data` contains the tool's typed result; `next` contains suggested calls with arguments and a reason, not permission to execute them. Error results have stable `code`, `message`, `retryable` and `recovery` fields. Do not parse English error messages to decide what to do. MCP returns the same envelope in structuredContent and serialized text for compatible clients; isError follows `ok`. CLI emits one compact JSON object to stdout and exits nonzero on failure, including partial batches.

Version 2 changes the public MCP/CLI result shape. The historical Python implementation is a test-only compatibility oracle; the native runtime invokes no Python. Existing JSON tool inputs and disk schema remain compatible. Upgrade consumers to read `data.record_path`, `data.attempt_id` and `data.page`; do not rely on raw coordinator state. The on-disk schema remains version 1, preserving current runs and receipts.

## Deferred tool discovery

The native MCP defaults to a focused catalog of 11 tools. inspect_tools returns concise descriptions, or exact input/output schemas for up to 10 named tools. Specialized local calls run as steps in query_batch/execute_batch after their schemas are loaded. This avoids sending every specialized definition to every model call. begin_submission stays a separately advertised one-shot tool. Use --tool-profile full for clients needing the complete catalog directly. Compatibility launchers may keep the full catalog; choose a profile explicitly. CLI/Rust always support all tools. Calling a known deferred tool directly in focused MCP returns deferred_tool with a schema-discovery suggestion; it is not an unknown tool. A next suggestion naming a deferred tool must be wrapped as a batch step in this profile.

Hosts needing per-child authorization must validate the batch's child tools and resolved arguments before execution, or use the full profile with direct tools. A client approval for execute_batch can cover multiple local operations; its name alone does not express child-level permissions. begin_submission is always separate, and batches never execute external actions.

## Start and retrieve

Call `bootstrap` with an optional operation, scope and capabilities available in the host. It returns personal settings from the configured ledger, task routing, recovery counts, configuration_issues, missing capabilities and a CLI fallback. Configuration issues identify missing canonical sources or an unconfigured/missing primary CV for application work; they do not certify that existing facts are complete or correct. Capability names describe behavior, such as `mail.read` or `board.sync`; the host binds them to its own connector implementations. Supplying capability strings is not evidence of access or user authority. Bootstrap is read-only and starts no run.

Read only the focused skill and relevant records. find_application accepts optional stage and text query; neither is required when listing the tracker. Page through metadata rather than loading the full generated pipeline for a narrow task. Use `query_batch` for independent reads and `execute_batch` for ordered local writes/reads. `get_run`, `find_application`, `get_approved_answer`, `list_events` and `reconcile_tracker` accept offset/limit; default limit is 10, maximum 50. Pages return total and next_offset; do not claim exhaustive coverage while it is non-null. Complete source records and answer sections are never silently clipped. read_record returns origin=ledger or toolkit; missing generic workflow assets resolve read-only from the toolkit. Duplicate checks return counts and a truncation flag; narrow retrieval/read full employer records when truncated.

## Batching and chaining

A failed batch forwards its failing child's recovery hints in the outer next list. Diagnostic read_record steps remain available during a pending journal. Batches have 1–20 uniquely named steps and a 250 KB request limit. A value containing exactly `{"$ref":"step.data.field"}` substitutes that prior successful result field. Numeric components can select array elements. References are data lookups, never JavaScript, shell or expression evaluation. Only preceding results are available.

```json
{
  "steps": [
    {"id":"start","tool":"start_run","arguments":{
      "operation":"reconcile","scope":{},
      "user_request":"Reconcile the fixture tracker","idempotency_key":"fixture:run"}},
    {"id":"inspect","tool":"get_run","arguments":{
      "run_id":{"$ref":"start.data.run_id"}}}
  ]
}
```

Each local mutation commits separately with its own idempotency key. A batch is **not** one transaction: on first failure it returns completed results, failed_step and skipped IDs. Earlier successes remain durable. Resolve the failure, then replay identical steps with the same keys or continue deliberately; changed input under an existing key is rejected. A crash between steps never authorizes repeating an external action. `query_batch` rejects writes; both batches reject nesting and `begin_submission`. Step arguments are validated after references resolve. Invalid tool names/boundaries are rejected before any step runs; an invalid later value may fail after earlier local steps have committed.

## Application boundary

`prepare_application` combines packet freezing and reservation. It returns exact artifact paths/hashes, packet_id, attempt_id and a suggested begin call. If reservation fails, the frozen packet may remain for inspection; no attempt has begun. Replaying identical input with unchanged source bytes returns its existing reservation. Changed CV, exact-answer bytes or canonical answer sources produce artifact_changed: cancel only an unstarted reservation, then use a new preparation key; a begun attempt still requires outcome reconciliation. It does not upload, begin or submit.

Call `begin_submission` separately at the actual external Submit boundary. Only a fresh `ready_for_single_submit=true` allows one external Submit, subject to the host's active user scope/confirmation rules. A replay returns false and the attempt's current status. Preserve the observed result and call `finish_submission`. Run scope also accepts max_attempts; its default is three times max_submissions, including negative/cancelled attempts, to bound repeated portal failures. Unknown/begun attempts never expire and block another attempt. Negative resolutions need evidence. Resolving any older attempt preserves a later pipeline stage or closed outcome; a missing canonical record returns a typed error before the attempt is resolved. Use `get_packet` for verified immutable artifact paths when recovering; changes to approved answers invalidate an unstarted packet.

## Provider adapters and a GUI

`plan_adapter` emits one combined stage-and-note action, source revision and plan_id. A `board.sync` driver reads current state, changes only differences, and returns live observed stage/note plus evidence. `record_adapter_result` checks the host-attested observations against the current plan under the coordinator lock before recording a verified sync. The host is responsible for attesting that these came from a live read; the local harness cannot independently prove provider liveness. A changed canonical record invalidates the plan. The driver must not echo desired values as verification. `drive_board(harness, driver, arguments)` provides the host-side orchestration: one initial read, change only differences, one final read after changes, then a verified local event. Missing capabilities or a non-executable plan return adapter_not_executable, so a plan alone is not reported as sync success. Identical result replays remain valid after the run finishes. Its BoardStateDriver interface requires read, set_stage and add_note; the host supplies the implementation. An adapter failure is logged once with a stable failure_code, provider and exact record; two consecutive matching recorded failures produce stop_retrying guidance. Authentication/CAPTCHA/upload failures do not imply submission success.

Board state plans describe a connector-neutral driver boundary; there is no built-in browser robot. Agents can drive the same plan through available UI/API tools; a GUI/backend can provide a native driver. The harness never assumes a selector, browser brand, mailbox vendor or model. Driver permissions remain the host's responsibility. There is no always-on HTTP service or remote coordinator.

Harness.call serializes concurrent threads with a reentrant lock; separate instances/processes use shared read locks and exclusive write locks around coordinator snapshots. Readers wait for an active writer; a pending journal without an active writer still requires recovery. Replaying rebuild_pipeline refreshes the derived views again, even if the same record state recurs. Other mutations retain their saved result on identical replay. pending_recovery includes a ready rebuild call; read_record remains available under a shared lock so a recovery conflict's current and pending versions can be inspected. Structured tracking blocks take precedence over legacy filename filters; a malformed structured title or damaged tracking marker fails closed. Literal documentation examples must use a distinct marker such as `<!-- tracking example`. Reserved guide/view/answer filenames cannot become vacancies. Cancelled preparation replays suggest a new key only while the run is active and instruct the caller to stop unless the cancellation cause is resolved and the role remains in scope; all normal scope, duplicate and cap checks still apply. A GUI can call this API directly, render data/error/next and use the published schemas for forms. A separate authenticated service can wrap it later. MCP exposes tools to agents; ACP may connect a host to an agent, but it is not required by the ledger API. No agent framework is embedded.

## Measure workflow efficiency

Response bytes, tokens, model round trips, retries, elapsed time and cost measure different things. A smaller response is not proof of an equivalent token, cost or latency reduction. The host should record these alongside verified completed applications/inbox actions, and compare cost/time per correct completion over comparable tasks. The harness cannot infer model calls or billed cost from local events. Measure them in the host; do not label local coordination events as model round trips. Prefer completion events or bounded/backoff waits over waking a model for unchanged polls. Local batches remove intermediate model decisions; they do not remove the underlying external actions.
