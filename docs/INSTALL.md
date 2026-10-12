# Install and connect

This private preview uses standard Python packaging. No package has been published to PyPI, npm or Homebrew, and no desktop application is claimed.

## Source install

```sh
gh repo clone KingPsychopath/job-harness
cd job-harness
python3 -m venv .venv
.venv/bin/pip install .
.venv/bin/job-harness --version
```

A Python wheel contains the CLI, local MCP and generic skills/guides. After installing it, commands work outside the source checkout. For a PATH-managed install, `pipx install .` or `uv tool install .` can manage the isolated Python environment. Homebrew can install Python/pipx; a dedicated formula can be added once release artifacts and access rules are stable. An npm launcher could wrap this engine later, but it would add a second runtime/install surface now.

## One-time private workspace setup

```sh
job-harness --root "$HOME/Career/workspace" init
job-harness configure --workspace "$HOME/Career/workspace"
job-harness connect mcp
```

The host-local default lives under `$XDG_CONFIG_HOME/job-harness/settings.json` or `~/.config/job-harness/settings.json`. It holds only the workspace path. `JOB_HARNESS_WORKSPACE` takes precedence, and explicit `--root` takes precedence over both. Root selection is trusted host configuration, not an MCP tool argument. Initialization needs an explicit root; it never silently creates a workspace from a typo.

Add the approved PDF and attributed answers described by [onboarding](../job_harness/resources/guides/job-harness-onboarding.md). The focus profile's 11 tools support discovery and batching; full exposes 29 direct tools. Use the connection's command/args, not a guessed executable. Reload the client after changing an installed package; registration is not execution proof.

## Skill discovery

The source `.agents/skills` symlink points to the canonical resources. Optional client installation may link each resource directory into a client's skill directory. MCP clients do not need installation: bootstrap returns a logical skill path and read_record retrieves the bundled source. Keep the same candidate root across chats. [Agent workflow](AGENTS.md).
