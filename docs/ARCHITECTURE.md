# Architecture

A durable workspace owns career evidence. Execution can be a process, an external agent, a compatible model endpoint or a GUI. The engine never depends on a particular agent account, model SDK, browser or VM.

![Boundaries and responsibilities](assets/architecture.svg)

## Four code layers

| Crate | Responsibility | Dependencies it must avoid |
|---|---|---|
| `job-harness-domain` | Typed attempt and application vocabulary | Filesystem effects, model providers, connector sessions |
| `job-harness-core` | Schemas, capabilities, locks, journal, artifacts, pipeline, transfers, derived index and CV preparation | Agent SDKs and model access |
| `job-harness-protocol` | Official SDK MCP server and ACP client, optional model host, loopback dashboard | Canonical career facts and stored credentials |
| `job-harness` | CLI arguments, host root selection and installation entry point | Candidate-specific defaults |

Capabilities are grouped by purpose: configuration, queries, runs, tracking, submissions, adapter coordination and batches. All transports call `Engine::call`; adapters do not reimplement the submission rules. The pure domain crate can be checked for WebAssembly without building a browser runtime. The filesystem engine requires a native host.

## Data boundaries

Markdown is canonical for attributed evidence, scoped answers and vacancy narratives. JSON selects CV content and structural settings. The schema-1 coordinator is canonical for runs, attempt state, immutable artifact references and idempotency. Generated pipeline files are disposable views.

A derived SQLite database stores documents, content hashes, vacancy projections, citations and full-text search. Refresh updates changed rows inside one transaction. It neither promotes a hypothetical answer into a fact nor grants action permission. Search results identify source paths, classification and hashes; read the source before using a claim. Guards read canonical state and exact files, even if the index is missing or incorrect.

Keep one coordinator during the language migration. Moving it into SQLite would require a deliberate new storage version and recovery design because Markdown writes still cross that transaction boundary. Do not silently introduce two authorities. Local state rewriting is appropriate to this bounded single-candidate workload; scale is a future storage migration, not a claim that filesystem locks work across hosts.

## Durable local action, uncertain external action

Under one exclusive workspace lock: write and sync content-addressed artifacts, commit state plus pending canonical writes atomically, replay the journal by before/after hashes, rebuild views, then refresh the index best effort. A cache failure cannot turn a committed action into permission to retry.

Read helpers do not acquire nested write transactions. Every journal path is checked again during recovery. Manual edits that conflict with pending work are preserved and require explicit evidence-backed resolution.

The submit boundary is separate: prepare/reserve → begin once → host submits once → finish with evidence. A begin replay never returns another submit permit. Begun or unknown attempts never expire. Local transactions cannot prove what an employer, mail provider or calendar did.

## Tool and context flow

Bootstrap gives focused routing, configuration gaps and unfinished-work health. Read metadata first and full relevant records on demand. `query_batch` groups independent reads; `execute_batch` runs ordered local writes with exact `$ref` values. Each child commits separately. Arbitrary expressions, shell commands and whole-workspace dumps are outside these tools.

A fixed root is selected at trusted host startup. No MCP argument switches candidates. External documents are untrusted input, never authorization. Optional model sessions share only explicitly supplied context and tool results; no workspace bulk upload occurs by default.

## Inner and outer loops

The career loop uses capabilities to discover, prepare, reconcile, schedule and preserve outcomes. Its durable continuation is the workspace, including unfinished runs and uncertain attempts.

The toolkit loop observes friction, reproduces it in a synthetic fixture, improves code or a focused skill, tests contracts and recovery, then releases a reviewed artifact. Use `harness-maintain` for this work. Skills are operational guidance; invariants belong in code and tests.

Compute and isolation are host choices. Additional sandboxes are useful for untrusted execution; persistent processes or VMs may suit other workloads. Neither is required to call a local tool or replaces provenance, access control and durable state.
