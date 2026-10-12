# Use Job Harness with your agent

Use the agent you already have. Job Harness supplies a private career workspace and tools for reading sources, checking duplicates, organizing applications and preserving submission evidence. Your agent host supplies its own model, authentication, browser, mail and calendar access. Neither MCP nor a built-in model is mandatory: an agent that can run commands can use the CLI directly.

| Your starting point | Next step |
|---|---|
| Installed core and an MCP-capable agent | [Connect MCP](#connect-mcp) |
| Agent that can run local commands | [Use the CLI directly](#use-the-cli-directly) |
| Toolkit checkout open in an agent project | [Build and set up a separate workspace](#open-the-checkout-in-your-agent) |

[Installation](INSTALL.md) covers native releases, optional desktop use and leaving the toolkit. Private career data belongs outside this repository.

## Connect MCP

First initialize or select your private folder:

```sh
jh setup --workspace "$HOME/Career/workspace" --capability mcp --json
jh --root "$HOME/Career/workspace" connect mcp
```

The second command returns an executable in `data.command` and arguments in `data.args`. Put those in your chosen host's MCP server configuration; the outer JSON format depends on that host. The executable path and workspace are fixed when the host starts the server. This command does not register a server, authenticate a model or connect your inbox.

Reload the host if needed, then ask its agent to call `bootstrap`. Read the returned focused skill and your workspace's `AGENTS.md`, inspect unfinished work and batch relevant reads. Confirm that bootstrap succeeds through the actual connection; saving a configuration is only the setup step.

The default focused profile exposes 12 starting tools. `inspect_tools` provides specialized schemas when needed; the full profile exposes the complete catalog. Generic guides and focused skills are embedded in the binary and can be read with `read_record`; installing skills into a particular agent is optional.

## Use the CLI directly

Give the agent the binary location and private workspace path. Use `--root` explicitly so work cannot accidentally select a different person's data. For example, on macOS/Linux:

```sh
jh --root "$HOME/Career/workspace" call bootstrap <<'JSON'
{"operation":"reconcile","capabilities":[]}
JSON
jh schemas query_batch
```

On any platform, the same input can be saved as a UTF-8 `bootstrap.json` file and passed with `jh --root PATH call bootstrap --input bootstrap.json`. In PowerShell, quote your actual path and use `jh.exe` if that is the installed name. The JSON input is identical across platforms.

CLI calls and MCP calls use the same engine and version 2 result: check `ok`, then `data` or `error`, and follow `next`. Use error codes rather than interpreting prose. Follow paging before claiming a complete scan. Related reads use `query_batch`; related local changes use `execute_batch` with explicit result references. [Tool contract and examples](../resources/guides/job-harness-contract.md)

## Open the checkout in your agent

Cloning this repository gives the agent the toolkit source and reusable guidance. It does **not** create a personal career workspace or load your career history. For the easiest installation, use the [core release](INSTALL.md#download-the-core-no-compiler-required). To build the checkout, install Rust and the platform C toolchain, then run:

```sh
gh repo clone KingPsychopath/job-harness
cd job-harness
cargo build --release --bins --locked
./target/release/jh setup --workspace "$HOME/Career/workspace" --json
./target/release/jh --root "$HOME/Career/workspace" connect mcp
```

In PowerShell, use `.\target\release\jh.exe`, and `(Join-Path $HOME "Career/workspace")` for the workspace argument. The repository pins its Rust toolchain; desktop and model-provider builds are separate optional components.

Open the cloned folder as your agent project and give it this prompt, replacing the private path:

```text
Use Job Harness for my career work. Read the repository's AGENTS.md. Use the
installed CLI, or build the core from this checkout if needed. Set up or select
/absolute/private/career-workspace outside this repository; preserve any existing
records. Use that path explicitly for every CLI call. Bootstrap, read the focused
skill and help me add supplied CV, notes and confirmed answers. Keep original
sources and ask only for missing facts. Use this host's connectors only within my
request. Do not put my career data, credentials or private bundles in the repo.
```

Setup saves a host-local default folder, but explicit `--root` remains useful for agents. Repeated setup selects an existing valid workspace without resetting its history. `--plan --json` previews setup without creating files or saving a default. [First-source onboarding](../resources/guides/job-harness-onboarding.md#add-your-first-sources)

## Give the agent a bounded task

```text
Use Job Harness with my private workspace. Bootstrap for this task, read its
focused skill and relevant sources, and inspect unfinished work. Batch related
reads and local updates. Reuse scoped answers and my primary CV, check duplicates,
and distinguish observed outcomes from proposed actions. Use this host's
connectors within my request and save evidence of what actually happened.
```

A recorded run describes intent; it does not grant permission. Before an authorized application submission, freeze the reviewed CV and answers, reserve the application, begin once, submit once externally, and record the verified result. Saved submission proof is evidence of the outcome; an application-confirmation email is a separate observed source. A submission already in progress or with an unknown result must be inspected externally before retrying.

The harness coordinates local records; it does not grant your host access to email, job sites or calendars. Configure only the connectors you need in that host. Your agent's native tools remain subject to its own permissions and can have a broader reach than the engine's file controls.

## Optional ACP handoff

If you want Job Harness to launch a separately installed ACP agent:

```sh
jh --root /absolute/private/workspace acp \
  --agent /absolute/path/to/acp-agent --prompt "Review my unfinished career work"
```

This is an ACP client, not a bundled agent. Authenticate your selected agent separately. The client starts a session with a fixed workspace and a Job Harness MCP server. It advertises no terminal or filesystem-write capability and rejects permission requests by default; your own host may provide an interactive approval flow. Startup includes unfinished-work health. The selected agent's other tools remain outside the engine's file controls.

## Optional direct model connection

A compatible tool-capable model can use the optional bounded adapter:

```sh
jh --root /absolute/private/workspace agent run \
  --endpoint http://127.0.0.1:11434/v1 --model YOUR_TOOL_CAPABLE_MODEL \
  --prompt "Review the application tracker and identify unresolved work"
```

It uses the OpenAI-compatible chat/tool-call format. Compatible Ollama or LM Studio endpoints are options; tool support must be verified for the selected model. Remote endpoints require HTTPS. A host can securely inject a key for one command and name it with `--api-key-env MODEL_API_KEY`; never put the key in a prompt, configuration file or career record.

The adapter bounds steps, calls, time and response sizes. It sends the prompt and selected tool results, so choose a trusted endpoint before sharing private material. It defaults to read-only tools. `--allow-local-writes` enables supported local changes; it does not authorize external submissions, email or calendar actions. Local models and their runtimes are separate [optional downloads](MODELS.md).

## Improve the toolkit separately

Use [harness-maintain](../resources/skills/harness-maintain/SKILL.md) for software changes. Reproduce problems with fictional data, test contracts and recovery, and keep development files outside career evidence. Coaching, model output and development logs do not become candidate facts merely because an agent wrote them.
