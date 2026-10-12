# Wire any agent or model

The workspace is durable context, the engine provides tools, and the host owns model access and external connectors. No provider is required by the core.

## External MCP host

```sh
job-harness connect mcp
job-harness --root /absolute/private/workspace mcp --tool-profile focused
```

Use the returned command and arguments in a supporting client. Start with bootstrap for the requested operation/capabilities, read its focused skill and the workspace's AGENTS.md, inspect unfinished work, then batch relevant reads. Specialized schemas are available through `inspect_tools`; full mode exposes them directly.

## External ACP agent

```sh
job-harness --root /absolute/private/workspace acp \
  --agent /absolute/path/to/acp-agent --prompt "Review my unfinished career work"
```

This is an ACP **client** launching a user-selected agent process. It initializes a session with a fixed workspace and the Job Harness stdio MCP server. Authenticate that agent separately. The harness advertises no terminal or filesystem-write capability and cancels permission requests; your own agent host may offer a richer interactive approval flow. It does not silently approve actions. ACP startup supplies unfinished-work health; external agent native tools are outside the engine's filesystem controls.

## Direct local or compatible remote model

```sh
job-harness agent run --endpoint http://127.0.0.1:11434/v1 \
  --model YOUR_TOOL_CAPABLE_MODEL --prompt "Review the pipeline and identify unresolved work"

# Host securely injects MODEL_API_KEY for this one command:
job-harness agent run --endpoint https://YOUR_PROVIDER/v1 \
  --model YOUR_MODEL --api-key-env MODEL_API_KEY --prompt "Review unfinished work"
```

The optional adapter uses the OpenAI-compatible chat/tool-call wire format, including compatible Ollama or LM Studio endpoints. Compatibility is a protocol requirement, not a guarantee that every model/provider supports tools. Other providers can be reached through an external agent or an adapter implementing the same boundary.

Remote endpoints require HTTPS; local loopback HTTP is accepted. Keys come only from a named environment variable and are never written to the workspace. Redirects are disabled. Steps, calls, time, context and response sizes are bounded. Only the prompt and selected tool results are sent; no automatic workspace upload occurs. Choose a trusted endpoint before passing private information.

Sessions default to a compile-time read-only tool set. `--allow-local-writes` permits supported local coordination, not external submission, mail sending or calendar booking. Supply external connectors through a capable host and current user scope. No model loop guesses a missing fact or retries a begun application.

## Operational request

```text
Use Job Harness with my configured private workspace. Bootstrap for this task,
read its focused skill and relevant sources, and inspect unfinished work.
Batch related reads and local updates. Reuse scoped answers and my primary CV,
check duplicates, and keep observed outcomes separate from proposed actions.
Use this host's connectors within my request and preserve actual evidence.
```

Read `data`, `error` and `next`, never parse error prose. Follow paging before claiming complete coverage. A run records intent, not permission. For a form: complete it, freeze/reserve exact artifacts, begin once, submit once externally, and finish with verified outcome evidence. [Full contract](../resources/guides/job-harness-contract.md)

## Toolkit improvement

Use the separate [harness-maintain skill](../resources/skills/harness-maintain/SKILL.md). Reproduce friction with synthetic data, improve a focused capability, test recovery/contracts, and preserve source/resource boundaries. Do not turn development logs, coaching or model output into candidate facts.
