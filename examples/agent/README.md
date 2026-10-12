# One workspace, any capable agent

The harness supplies tools and durable evidence. Your agent supplies decisions;
your host supplies authenticated browser, mail and calendar connectors.

```sh
job-harness --root "$HOME/Career/workspace" init
job-harness configure --workspace "$HOME/Career/workspace"
job-harness connect mcp
```

Use the returned `data.command` and `data.args` in your MCP client. The native
executable includes schemas, skills and guides; it runs outside this checkout.
The focused profile advertises eleven tools. `inspect_tools` discovers the
specialized tools for invocation inside bounded batches. `--tool-profile full`
advertises every tool directly.

## An agent's first request

> Bootstrap for my task, read its focused skill and inspect unfinished work.
> Load sourced facts and answers only as needed. Batch related reads and local
> updates. Check duplicates before preparing an application. Freeze and reserve
> the exact CV and answers, begin once, perform the authorized external action
> once through my host, and record its verified outcome. Preserve uncertainty.

## Pipe bounded reads

```sh
job-harness call query_batch --input examples/agent/inspect.json
```

CLI stdout is compact JSON. Failures use the same version 2 `data/error/next`
envelope and a nonzero exit status. Batches resolve exact data references; they
are not a shell or JavaScript interpreter. Use `job-harness schemas TOOL` before
constructing a specialized mutation.

## ACP is a client-agent connection

```sh
job-harness connect acp
job-harness acp --agent /absolute/path/to/your-acp-agent \
  --agent-arg=--acp --prompt 'Inspect my unfinished job-search work. Make no external changes.'
```

The stable ACP v1 client initializes your external agent, starts a session at the
selected workspace, attaches Job Harness's focused MCP and sends the prompt.
Authenticate that agent separately using its host. No model key is required by
Job Harness. Interactive permission requests are cancelled; this client does
not auto-approve external actions, provide a terminal, or grant file access.
The external executable still has the OS permissions of the user launching it;
ACP capabilities do not sandbox a process. Use a suitable trusted agent.
The attached MCP enforces coordination-only tools by default, including batch
children: submission boundaries, tracker corrections and human note acceptance
are denied. `--allow-external-actions` enables the regular harness profile only
when your trusted agent host has the connectors and explicit task authority.
Timeouts close the agent process group and its attached MCP descendants.

## A direct local or remote model

The optional host adapter accepts an OpenAI-compatible Chat Completions API.
Choose a tool-capable model supported by your endpoint; model availability and
behavior belong to that provider.

```sh
# Local Ollama. Select a model already installed in your local host.
jh agent run --endpoint http://localhost:11434/v1 --model YOUR_LOCAL_MODEL \
  --prompt 'Inspect unfinished work and explain what needs attention.'

# Local LM Studio uses the same tool-call wire format.
jh agent run --endpoint http://localhost:1234/v1 --model YOUR_LOADED_MODEL \
  --prompt 'Find my saved applications and summarize the next steps.'

# Remote compatible provider: inject MODEL_KEY from your secure host store.
# The flag names the environment variable; it is never the secret value.
jh agent run --endpoint https://YOUR_PROVIDER/v1 --model YOUR_MODEL \
  --api-key-env MODEL_KEY --max-steps 8 --max-tool-calls 32 \
  --prompt 'Review my pipeline. Make no changes.'
```

Only declared read-only tools are available by default. `--allow-local-writes`
explicitly allows drafting and local coordination. Submission boundaries,
tracker updates, human note acceptance and archival remain unavailable in direct
model loops. This adapter supplies no external
connectors or application authority. Remote endpoints require HTTPS; redirects
are disabled. Prompts, provider bodies, tool results and context are bounded.
The loop reports actual provider token usage where supplied and counts actual
tool calls. It loads relevant workspace records through tools, rather than
uploading a workspace automatically. Provider-specific APIs can use an ACP
agent or another host adapter without changing the domain core.

[OpenAI tool format](https://developers.openai.com/api/reference/resources/chat)
· [Ollama compatibility](https://docs.ollama.com/api/openai-compatibility)
· [LM Studio tools](https://lmstudio.ai/docs/developer/openai-compat/tools)

## Local human view

```sh
job-harness gui
```

Open the printed loopback URL. The small dashboard reads pipeline records and
recovery metadata through the same engine, with kanban, attention, sourced company
context, note review and connection metadata. It has no submission button or
generic tool endpoint. Start `job-harness gui --allow-local-writes` to enable named
human review routes for importing, proposing, accepting and archiving notes and
reviewing tracker updates. Acceptance requires selected items, a supplied review
statement and the current proposal hash; CV facts are not promoted automatically.
Its session cookie is temporary, HttpOnly and SameSite Strict. Reads require the
exact local host; writes also require the exact origin and session CSRF nonce.
Stop the process to close the local session.

```sh
job-harness notes import --input supplied-notes.md --kind personal_notes \
  --occurred-period 'around Q2 2024' --source-label 'Project notes' --key note-1
job-harness notes list
job-harness notes review NOTE_ID --include-original
job-harness schemas propose_note
job-harness call context_pack --input context-request.json
```

Keep rough dates rough and leave unknown dates unknown. Original, proposed and
accepted context remain separate; source hashes and line citations travel with
every interpretation.
