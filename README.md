<div align="center">
  <img src="docs/assets/icon.svg" width="72" height="72" alt="Job Harness">
  <h1>Job Harness</h1>
  <p><strong>Context. Commands. A workspace.</strong><br>Run your job search with any capable agent, without losing your history.</p>
  <p><a href="https://github.com/KingPsychopath/job-harness/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/KingPsychopath/job-harness/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Private preview" src="https://img.shields.io/badge/status-private_preview-7959b8">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-287d72"></a>
  <img alt="Python 3.11 and newer" src="https://img.shields.io/badge/Python-3.11%2B-243444"></p>
</div>

![The reusable harness and a separate private workspace, connected to the agent and host.](docs/assets/hero.svg)

Job Harness gives an agent dependable tools for a private career workspace: find existing applications, reuse sourced answers, organize inbox work, prepare applications and preserve verified outcomes. CLI, MCP and Python clients share one typed API. Your host supplies the model, browser, mail and calendar connectors.

- **Your workspace is the durable memory.** Facts, approved answers, CVs, vacancies and receipts survive agent changes and interrupted runs.
- **Tools do the predictable work.** Compact results, schema discovery, ordered batches and result references reduce repeated coordination.
- **History protects the next action.** Duplicate checks and a separate begin/submit/finish boundary prevent an interrupted application from silently being retried.
- **Personal data stays separate.** The reusable package contains no candidate profile. Private bundles move your workspace between installations.

This is a **private preview**, prepared for later open-source distribution. No public PyPI, npm, Homebrew or desktop-app release exists. The local engine needs no model key, cloud service or VM.

## Get started

Python 3.11+ on macOS or Linux. Authenticate GitHub to clone this private repository:

```sh
gh repo clone KingPsychopath/job-harness
cd job-harness
python3 -m venv .venv
.venv/bin/pip install .
.venv/bin/job-harness --version
```

Create a workspace **outside the code checkout**, add your approved CV and attributed answers, then select it:

```sh
.venv/bin/job-harness --root "$HOME/Career/workspace" init
# Edit the workspace's config/career-ledger.json and approved answer files.
.venv/bin/job-harness configure --workspace "$HOME/Career/workspace"
.venv/bin/job-harness call bootstrap <<'JSON'
{"operation":"apply","capabilities":[]}
JSON
```

Bootstrap reports missing sources/CV/capabilities; a blank workspace never invents personal facts. `--root` overrides your host-local default. [Install options](docs/INSTALL.md).

## Connect your agent

```sh
.venv/bin/job-harness connect mcp
.venv/bin/job-harness --root "$HOME/Career/workspace" mcp --tool-profile focused
```

Use the printed command/args in an MCP client. The focused profile advertises 11 tools; `inspect_tools` discovers specialized schemas, which run through `query_batch` / `execute_batch`. The full profile exposes all 29 tools directly. Hosts using ACP can give the same MCP/CLI to their agent; no agent framework is embedded.

A useful first request:

```text
Use Job Harness with my configured private workspace. Bootstrap for the task,
read its focused skill and relevant facts, and inspect unfinished work first.
Batch related reads and local updates. Use my saved answers and primary CV,
check duplicates, and keep actual outcomes separate from proposals. Use the
host's existing connectors within my request; do not invent missing facts.
```

[Agent workflow](docs/AGENTS.md) · [Tool contract](job_harness/resources/guides/job-harness-contract.md) · [Authentication](docs/AUTH.md)

## Move your private workspace

```sh
.venv/bin/job-harness workspace export --output "$HOME/Career/workspace.jhb"
.venv/bin/job-harness --root "$HOME/Career/restored" workspace import \
  --input "$HOME/Career/workspace.jhb"
```

A full workspace bundle preserves receipts, packets and unresolved attempts. `--kind profile` transfers reusable facts/answers and the primary CV, excluding application history. Imports require a new/empty destination. Bundles are private plaintext ZIPs with a versioned hash manifest; keep them outside Git and encrypt separately before sharing. Git history and host settings are excluded; common credential filenames are filtered, but file contents are not scanned for secrets. [Bundle format and recovery](docs/BUNDLES.md).

## The layers

![Workspace, harness, agent or GUI, and host connectors with credentials outside the workspace.](docs/assets/architecture.svg)

| Layer | Responsibility |
|---|---|
| Private workspace | Attributed facts, preferences, CV, application history and exact evidence |
| Reusable harness | Schemas, retrieval, batches, duplicate guards, coordinator and bundle transfers |
| Host | Model access, browser/mail/calendar sessions, credentials and action permissions |
| Agent or GUI | Decisions and presentation using the same API |

A future GUI can call `Harness(root).call(tool, arguments)` and render `data`, `error` and `next`. Hosting it for multiple users needs per-user authorization and a transactional shared coordinator; local filesystem locks are for one host. [Architecture and runtime choices](docs/ARCHITECTURE.md).

## Develop

```sh
.venv/bin/pip install -e '.[dev]'
make check PYTHON=.venv/bin/python
make check-package PYTHON=.venv/bin/python
```

The tests use synthetic temporary workspaces, including crash recovery, concurrency, duplicate prevention, bundle safety, wheel resources and independent MCP/schema checks. There are no live employer submissions in CI.

[Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [Development](docs/DEVELOPMENT.md) · [Changelog](CHANGELOG.md) · [Roadmap](docs/ROADMAP.md)

MIT for the reusable software. A candidate's private data is their own and is separate from the software distribution.
