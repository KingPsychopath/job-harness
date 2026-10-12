# Job Harness — onboarding

Job Harness is reusable tooling for a private job-search ledger. It provides workflows, typed tools, bounded batches, submission coordination and evidence-backed tracking. An agent or GUI supplies the external mail, browser and calendar capabilities. The native Rust binary on Windows/macOS/Linux is the supported local runtime; Shared cross-machine storage needs a separate coordinator.

## How it runs

```mermaid
flowchart LR
    A[Agent or GUI] --> H[Job Harness API]
    H --> C[Schemas, batches and coordination]
    C --> W[Private candidate ledger]
    A --> R[Host runtime and permissions]
    R --> P[Mail, job sites and calendar]
    R --> H
```

The agent supplies decisions; Job Harness runs predictable local steps and keeps durable evidence. The host runtime supplies live actions and permissions. Each private workspace holds one candidate's configuration and career records.

## Four separate homes

| Home | Contains | Share with the toolkit? |
|---|---|---|
| `crates/` and generic skills/guides | Tool contracts, coordination, adapters, workflow defaults and onboarding | Yes, through the code-only repository or native release |
| Each ledger's `config/` | Timezone, CV path, answer-source paths and optional workflow overrides | No; owner-specific configuration |
| Each ledger's `context/`, `content/`, `applications/`, `output/` | Facts, approved CV, preferences, vacancies, history, exact packets and receipts | No; private candidate data |
| Agent/GUI host | Model choice, connector bindings, credentials, user permissions and scheduling | No; host-owned runtime/access |

Configuration selects data; it is not evidence for a career claim. Each candidate has a separate ledger/root and coordinator. Switching root is a trusted host startup choice, never an argument available to an agent tool. Agents sharing one candidate must share the same checkout/coordinator. Copies on different machines have independent locks.

## Try it without personal information

From the toolkit directory, create an empty private ledger outside it:

```sh
job-harness --root /path/to/private-ledger init
job-harness --root /path/to/private-ledger call bootstrap <<'JSON'
{"operation":"reconcile","capabilities":[]}
JSON
job-harness schemas execute_batch
job-harness config-schema ledger
```

Initialization builds a temporary sibling directory and moves it into place only after success. It refuses to replace a nonempty directory. It creates blank answer/evidence records and neutral configuration; it copies no salary, contact details, CV, demographic answer or career history. Set primary_cv to a repository-relative PDF path, set the IANA timezone, and supply attributed career records/approved answers. Missing personal facts remain missing. Generic skills, operating guides and workflow defaults stay in the toolkit; read_record resolves them there and returns origin=toolkit. They are not copied into each candidate ledger. A private file at the same allowed path is an explicit owner override.

Wire a stdio MCP client to this command, using absolute paths. Embedded resources work outside the checkout:

```text
job-harness --root /path/to/private-ledger mcp --tool-profile focused
```

Use `job-harness --root /path/to/private-ledger connect mcp` to print a client-neutral command/args pair with a native executable and fixed workspace. Skill installation is optional; any agent can read the focused skill returned by bootstrap through read_record. A fresh client session may be needed after configuring a new MCP. Always test bootstrap through the actual connection; registration alone is not live-tool proof.

For a Rust host or GUI backend:

```rust
use job_harness_core::Engine;
use serde_json::json;
let harness = Engine::open("/path/to/private-ledger")?;
let result = harness.call("bootstrap", &json!({"operation":"apply","capabilities":[]}));
// Render data/error/next. The host binds external providers and authorization.
```

Use `query_batch` to fetch related records/answers together, `execute_batch` to organize local updates with result references, and `prepare_application` to freeze/reserve with one call. See the [contract and working examples](job-harness-contract.md). Actual external submissions still use a separate begin/one-submit/outcome boundary.

## First useful input

With an existing CV, `job-harness source import --input approved-cv.pdf` retains the exact supplied file and provenance. Set it as primary only after your explicit review. Without a CV, start with a rough note through `notes import` or `intake_note`. The original remains intact and missing dates or ownership remain unknown. An agent can propose linked interpretations; accept selected items separately. No mandatory questionnaire or model installation is needed.

Choose a [short playbook](workflows.md) for capture, a period review, a move, an application, an interview or a new role. The choice changes focus, never employment facts. `context_pack` presents bounded source history; the agent handles interpretation and writing.

## Share tooling, never a ledger

Install the native binary or clone the code-only repository. Generic resources ship inside the native binary; there is no second source-export implementation. Each recipient creates their own external workspace or imports an explicitly private .jhb bundle. Bundles contain personal data and are never part of the software distribution. Host credentials and connector sessions remain separate.
