# Install and onboard

The native preview targets Windows, macOS and Linux. Exact-head CI is the platform evidence; optional native model SDK availability is a separate boundary. Public Cargo, npm and Homebrew publication is not configured. Native release archives can be produced without including a personal workspace.

## Source installation

Install Rust from its official distribution, authenticate GitHub for this private repository, then:

```sh
gh repo clone KingPsychopath/job-harness
cd job-harness
./scripts/install.sh
# Or: cargo install --locked --path crates/harness-cli --root "$HOME/.local"
```

On Windows PowerShell, use `./scripts/install.ps1` (Rust with the MSVC C build tools), then add `$HOME/.local/bin` to PATH. On macOS/Linux, use the shell installer. Resources and templates are embedded in the binary; it runs outside the checkout without Python or a source-tree working directory. Typst is optional and required only for compiling CVs.

## Your private workspace

```sh
job-harness --root "$HOME/Career/workspace" init
job-harness configure --workspace "$HOME/Career/workspace"
job-harness call bootstrap <<'JSON'
{"operation":"reconcile","capabilities":[]}
JSON
job-harness connect mcp
```

Initialization refuses a nonempty destination and creates no personal facts. Add your approved PDF, set relative `primary_cv` and answer-source paths, and choose an IANA timezone in `config/career-ledger.json`. Record facts and preferences with date, attribution, scope and limits. [Onboarding details](../resources/guides/job-harness-onboarding.md)

Root precedence: explicit `--root`, then `JOB_HARNESS_WORKSPACE`, then host-local `$XDG_CONFIG_HOME/job-harness/settings.json` (otherwise `~/.config/job-harness/settings.json`). Host settings hold a path; candidate bundles never include them. Use a fixed absolute root in MCP registration.

## Connect and verify

Use `connect mcp` output in any MCP client. Focused advertises 11 tools; full advertises the complete catalog. Optional skill installation links `resources/skills/` into your host's skill directory. Clients can retrieve embedded skills with `read_record`, so installation is not required.

Reload a client after changing its server command, then verify bootstrap through that actual connection. Configuration is not proof a previously running process switched versions. ACP and local/remote model commands are described in [agent wiring](AGENTS.md).

## Upgrade or move

Export a full private bundle before upgrading. Keep the old installed binary until recovery and compatibility checks pass. Unknown attempts must survive; never clear the coordinator to fix a migration error. Restore into a new directory and inspect unfinished work before taking external actions.

Use [workspace transfers](BUNDLES.md) for personal data, release archives for software, and the host's secure credential store for access. A GUI consumes the same boundaries.

## Windows PowerShell

```powershell
gh repo clone KingPsychopath/job-harness
Set-Location job-harness
./scripts/install.ps1
$env:Path = (Join-Path $HOME ".local/bin") + [IO.Path]::PathSeparator + $env:Path
job-harness --root (Join-Path $HOME "Career/workspace") init
job-harness configure --workspace (Join-Path $HOME "Career/workspace")
job-harness status
job-harness connect mcp
```

The native Windows release uses `.exe` files in a ZIP; macOS/Linux use native binaries in tar archives. Native release installation needs no Rust or Python. Source builds need the selected platform's C compiler for bundled SQLite/cryptography. Host settings use XDG/HOME where present and APPDATA on Windows otherwise. Use `--input` with UTF-8 JSON files for portable Unicode tool arguments.

The same core/protocol contracts apply across operating systems. The pure domain is also WASM-checkable; browser/mobile embedding needs host storage and connector adapters. Other Rust targets need platform-specific build verification rather than an unsupported universal execution claim.
