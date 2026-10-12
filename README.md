# Job Harness

**Your career workspace. Any capable agent.**

Keep your job search moving without rebuilding context every time you switch chats or tools. Job Harness gives your agent sourced answers, searchable project and interview notes, exact application packets and a history it can resume. Your private workspace keeps the evidence; one Rust engine gives people and agents the commands to work with it.

![Your agent calls the local toolkit; your career history stays in a separate private workspace.](docs/assets/hero.svg)

[Get started](#start-your-workspace) · [Try the demo](#see-it-work) · [Connect an agent](#give-your-agent-the-tools) · [Choose capabilities](#add-only-what-you-need) · [Contribute](CONTRIBUTING.md)

## See it work

With the native binary installed, create a **fictional** workspace and find a project story:

```sh
jh demo --workspace "$HOME/Career/demo"
jh --root "$HOME/Career/demo" workspace search database
jh --root "$HOME/Career/demo" gui
```

The demo includes applications, a linked interview note and a project source. Search returns original paths and source identities. No employer is contacted, no application is submitted, and an existing workspace is never overwritten. [More runnable examples](examples/README.md)

![Application board in a fictional local workspace.](docs/assets/dashboard.png)

## From saved answers to the next interview

- **Answer once, reuse it where it fits.** Keep confirmed answers with their source, date and scope. Another agent can use them without asking you to repeat your history.
- **Find the story you need.** Search projects, notes, vacancies and earlier answers. Follow linked context or add optional embeddings for related passages.
- **Know exactly what you sent.** Prepare a packet for the vacancy with selected answers and the exact CV bytes. Keep your existing primary PDF, or create a separately approved variant.
- **Resume with the uncertainty intact.** An interrupted submission remains blocked until its external outcome is checked. Restarting an agent does not grant another submit.
- **Keep the next action visible.** Reconcile the outcomes your host finds in email, preserve interview notes and bring unresolved work into one review queue.

For a connected agent, a normal request can be:

> Review my recruiting emails, preserve anything needing a reply, reconcile receipts and rejections, then apply to suitable saved roles. Reuse my sourced answers and primary CV, check duplicates and keep verified outcomes in the tracker.

Your agent uses its own browser, mail and calendar connectors. Job Harness provides the shared context, coordination and recovery rules. [Six everyday workflows](resources/guides/workflows.md) · [GUI, CLI and session flow](docs/GUI.md)

## Start your workspace

Source installation is available now. Preview packages are held while an intermittent Windows notification stall is investigated. [Current validation status](docs/VALIDATION.md)

Published archives will be available through [GitHub Releases](https://github.com/KingPsychopath/job-harness/releases). Verify the SHA256 and put `job-harness` and `jh` on your PATH; native archives need no Rust or Python installation. [Installation, checksums and platform requirements](docs/INSTALL.md)

```sh
jh setup --workspace "$HOME/Career/workspace"
jh status
jh connect mcp
```

Setup creates or selects your separate private workspace and lets you choose what you want to do. It explains optional requirements and prints the next commands; **it never silently installs a model or grants account access**. Agents can use `--json`, select repeated `--capability ID` values, or inspect `--plan` without changing files.

Bring your existing PDF with `jh source import --input approved-cv.pdf`, then select its preserved path as `primary_cv` in workspace config. Add direct answers and notes with attribution. Bootstrap identifies missing configuration before application work. [Onboarding](resources/guides/job-harness-onboarding.md)

**Windows, macOS and Linux core.** Local by default. No VM, hosted service or model account is required. Optional desktop and model components have their own platform requirements.

## Give your agent the tools

| Drive it with | Connection | Access belongs to |
|---|---|---|
| An MCP-capable host | `jh connect mcp` prints the server configuration | Your existing agent host |
| An agent that can run commands | `jh call TOOL --input arguments.json` | The command-running host |
| An external ACP agent | `jh acp --agent /path/to/agent --prompt "Review my pipeline"` | That separately authenticated agent |
| A compatible local or remote model | `jh agent run --endpoint URL --model MODEL --prompt TASK` | Your selected runtime or provider |
| A person or embedded product | Local dashboard, optional desktop, or `Engine::call` | Your local host |

MCP offers tools; the ACP client launches your chosen agent. Compatible model sessions use the chat-completions/tool-call format and default to read-only tools. Remote keys are injected through a named environment variable, never exported with your workspace. [Agent wiring](docs/AGENTS.md) · [Access boundaries](docs/AUTH.md)

### Batch the predictable work

The focused MCP profile starts with 11 tools. Discover specialized schemas with `inspect_tools`, then batch reads and chain local operations with exact result references. The full profile exposes all 40 engine tools.

Save this as `batch.json`, then run `jh call execute_batch --input batch.json`:

<details>
<summary>A two-step batch with a result reference</summary>

```json
{
  "steps": [
    {
      "id": "run",
      "tool": "start_run",
      "arguments": {
        "operation": "reconcile",
        "scope": {"dry_run": true},
        "user_request": "Review unresolved local work",
        "idempotency_key": "example:review:start"
      }
    },
    {
      "id": "finish",
      "tool": "finish_run",
      "arguments": {
        "run_id": {"$ref": "run.data.run_id"},
        "status": "completed",
        "summary": "Local coordination example; no external action",
        "evidence": [],
        "idempotency_key": "example:review:finish"
      }
    }
  ]
}
```

</details>

Every call returns a compact envelope:

```json
{"schema_version":2,"ok":true,"data":{},"error":null,"next":[]}
```

Each child commits separately. If a later step fails, earlier results remain explicit; the caller resumes from them. `next` suggests useful calls, never permission. [Schemas, partial failure and recovery examples](resources/guides/job-harness-contract.md)

## Add only what you need

The base includes exact search, linked notes, application coordination, backups, MCP/CLI and the local dashboard. Optional components stay outside the core installation.

| Want to… | Add | Download / requirements | What leaves your machine |
|---|---|---|---|
| Find passages by meaning | Native EmbeddingGemma 2 provider | 157 MiB weights + separately described runtime | Selected text stays local with the local provider |
| Review bounded decisions | Native Laya provider | 647 MiB weights + runtime; confidence can abstain | Selected text stays local with the local provider |
| Draft or enrich supplied notes | Your agent or compatible generative model | Your existing host; local model or remote API | Only selected context, if you choose a remote endpoint |
| Create a CV from evidence | Typst | Optional compiler and selected fonts | Nothing required |
| Use a desktop window | Optional Tauri desktop package | OS webview and separately built application | Loopback workspace UI only |
| Import a supplied audio transcript | Compatible transcription endpoint | Optional audio-file adapter; recording is separate | Selected audio, if using a remote endpoint |

Inspect requirements with `jh capabilities` and model identities with `jh context models`. Runtime and weight downloads require explicit selection and pinned digests. Model support and measurements are listed per platform; a small model is not permission for unattended email archiving or hiring decisions. [Local models, Kev evaluation and measured limits](docs/MODELS.md)

The desktop's Connections panel checks local tooling and files. With the optional provider installed, review and confirm the exact Gemma or Laya download plan, or explicitly verify cached artifacts. Installed, verified and running remain distinct. There is no automatic model start.

### Search and preserve the original

```sh
jh workspace search "database"
jh notes import --input interview.md --kind personal_notes --key interview:original
jh notes list
```

Original notes, transcripts and provider summaries remain distinguishable. Proposed additions cite source hashes and lines, and you choose which items to accept. Missing dates and unsupported details stay unknown. [Notes, review and context packs](docs/INTAKE.md)

For semantic retrieval, select sources explicitly, index with your chosen embedding model, then search with the same revision. Results retain the original citations. [Runnable retrieval and typed-decision examples](examples/models/README.md)

### Turn selected evidence into useful documents

Keep an existing primary CV or use the optional [Typst CV generator](docs/CV.md) with a one-page default, typography knobs and explicit provenance. [Fictional CV example](examples/cv/example.pdf)

[Selected document templates](docs/DOCUMENTS.md) produce cover letters, application answers, interview stories, portfolio pages and period reviews in Markdown or JSON. A generated document never silently changes career facts or replaces your primary CV.

## Your data has a clear home

Human-readable Markdown holds attributed evidence and vacancy narratives. Selected JSON and PDFs hold approved content and exact artifacts. A durable coordinator records attempts; SQLite indexes and pipeline views can be rebuilt. Configuration chooses paths and workflows. Credentials remain in your host.

```sh
jh workspace export --output "$HOME/Career/backup.jhb"
jh --root "$HOME/Career/restored" workspace import --input "$HOME/Career/backup.jhb"
```

A full private bundle carries history, exact artifacts and unresolved attempts into a new workspace. Software packages contain no candidate profile, model weights or credentials. Bundles are plaintext ZIPs: encrypt them separately before sharing. [Architecture](docs/ARCHITECTURE.md) · [Backup and recovery](docs/BUNDLES.md)

## Guarantees and limits

The engine preserves sources and uncertainty; it cannot prove what an external site did without evidence. A run records intent, not authority. Search results and model suggestions are not qualifications. Local coordination supports one workspace across processes on one host; multi-host storage needs a separate transactional design.

This is a private preview. Native packages are unsigned; platform signing, public registries, auto-update and hosted tenancy are separate release work. Exact-head CI and component-specific runtime checks are the evidence for supported builds. [Security](SECURITY.md) · [Review record](docs/REVIEW.md) · [Roadmap](docs/ROADMAP.md)

## Build with us

The career loop uses the toolkit. The development loop improves it against synthetic cases and compatibility fixtures. Keep personal data out of both fixtures and screenshots; keep reusable guidance under `resources/`.

```sh
cargo run -p job-harness-dev -- repository
cargo test --workspace --locked
cargo run -p job-harness-dev -- package --binary target/release/job-harness
```

[Contributing](CONTRIBUTING.md) · [Development commands](docs/DEVELOPMENT.md) · [Tool contracts](resources/guides/job-harness-contract.md) · [License](LICENSE)

MIT for the reusable software; [third-party components](docs/THIRD_PARTY.md) retain their own licenses. Your private career data belongs to you.
