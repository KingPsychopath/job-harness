# Install what you need

Start with the native toolkit. Exact search, notes, application coordination, private backups, CLI/MCP and the local dashboard are included. Models, their runtimes, Typst and the optional desktop application install separately.

## Native release: no compiler required

Get your platform archive from [GitHub Releases](https://github.com/KingPsychopath/job-harness/releases). This repository and its releases remain private; authenticate with your existing GitHub access. Choose your actual CPU:

| System | Archive target |
|---|---|
| Apple Silicon macOS | `aarch64-apple-darwin` |
| Intel macOS | `x86_64-apple-darwin` |
| Linux x86-64 | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |
| Windows x86-64 | `x86_64-pc-windows-msvc` |

Download the matching archive and `SHA256SUMS`. On macOS, compare `shasum -a 256 ARCHIVE`; on Linux, `sha256sum ARCHIVE`; on Windows, `Get-FileHash ARCHIVE -Algorithm SHA256`. The digest must match the archive's exact entry. SHA256 detects changed bytes; it does not replace trust in the repository or signing.

Extract the archive and put both `job-harness` and `jh` in your user bin directory, then add that directory to PATH. Windows executables have `.exe`. `BUILD.json` records the source commit, target and build toolchain. `jh --version` confirms which installation you are using.

The optional desktop archive is separate: on macOS, open or move **Job Harness.app** into Applications; on Windows/Linux, launch its native executable after installing the platform webview requirements. It opens your configured workspace or offers an existing-folder chooser. You do not need to enter commands in the GUI. The local provider archive is another optional component; its runtime/weight lifecycle is described below.

These preview archives are **unsigned**. macOS Gatekeeper and Windows SmartScreen may warn about them. Review provenance and your platform's normal explicit application approval; do not disable global security controls. Public package registries, signed installers and auto-update are not enabled.

## Set up your private workspace

```sh
jh setup --workspace "$HOME/Career/workspace"
jh status
jh connect mcp
```

On a terminal, setup asks which capabilities you want. All optional software stays uninstalled until you choose its separate lifecycle. To automate without prompts:

```sh
jh setup --workspace "$HOME/Career/workspace" --capability mcp --json
jh setup --workspace "$HOME/Career/workspace" --capability retrieval --plan --json
jh capabilities --json
```

`--plan` changes nothing. Without a capability selection, noninteractive setup configures only the base. Repeated selections are deduplicated in the plan. Setup initializes a new/empty directory or selects an existing valid workspace; it never resets its coordinator or career evidence. Credentials are neither requested nor stored.

Use `jh demo --workspace "$HOME/Career/demo"` for fictional data. It refuses an existing nonempty destination and makes no employer, mail or model requests. `jh --root "$HOME/Career/demo" gui` displays that separate workspace without changing your default.

### Bring what you already have

`jh source import --input approved-cv.pdf` preserves the supplied file and records its identity. Select that retained relative path as `primary_cv` in `config/career-ledger.json`; import does not automatically approve content. Choose an IANA timezone and source paths in that config. Add direct answers with source, date, scope and unresolved limits. Original notes can be imported without reconstructing missing facts. [Detailed onboarding](../resources/guides/job-harness-onboarding.md)

Root precedence is explicit `--root`, then `JOB_HARNESS_WORKSPACE`, then host-local settings. Settings use `$XDG_CONFIG_HOME/job-harness/settings.json`, otherwise APPDATA on Windows or `~/.config/job-harness/settings.json`. They hold a path, not candidate facts or keys, and are excluded from private exports.

## Add a capability deliberately

| Goal | Next step | Additional download |
|---|---|---|
| Give an existing agent the tools | `jh connect mcp`; paste the returned config into your host | None in Job Harness |
| Launch a separately installed agent | `jh acp --help` | Your selected agent |
| Use a compatible local/remote model | `jh agent run --help` | Your selected runtime/model, or provider access |
| Semantic retrieval or typed suggestions | `jh context models`, then the [native model lifecycle](MODELS.md) | Listed native runtime and pinned model bytes |
| Compile an evidence-backed CV | [CV guide](CV.md) | Optional Typst and chosen fonts |
| Use a native desktop window | [Desktop package](../apps/desktop/README.md) | Separate application; OS webview requirements |

Selecting a capability in setup is a plan, not an installation or permission grant. Model info reports immutable sources, size, hardware and unknown memory requirements before explicit installation. The local provider never downloads on serve. Remote model endpoints send only selected prompt/context; choose them intentionally. Browser, mail and calendar sessions remain host-owned. [Access boundaries](AUTH.md)

## Build from source

Install Rust from [rustup](https://rustup.rs), the platform C toolchain, and authenticate GitHub for this private repository:

```sh
gh repo clone KingPsychopath/job-harness
cd job-harness
./scripts/install.sh
```

The pinned core build uses Rust 1.97.1; its minimum Rust version is 1.88. Source builds use bundled SQLite/cryptography and need a C compiler. Embedded resources make the installed binary independent of the checkout. Optional desktop compilation has its own minimum and platform dependencies.

On Windows PowerShell with MSVC build tools:

```powershell
gh repo clone KingPsychopath/job-harness
Set-Location job-harness
./scripts/install.ps1
$env:Path = (Join-Path $HOME ".local/bin") + [IO.Path]::PathSeparator + $env:Path
jh setup --workspace (Join-Path $HOME "Career/workspace") --json
jh status
jh connect mcp
```

Use UTF-8 JSON files with `--input` for portable structured arguments. Use native Windows paths in host launch configuration; a copied Unix config does not select the equivalent Windows directory automatically.

## Connect, upgrade and move

The MCP focused profile advertises 11 tools; full exposes all 40. Specialized schemas and generic guides are embedded and retrievable without installing a skill folder. Reload the actual client after changing its server executable, then run bootstrap through that connection. A saved config alone does not prove an old process restarted.

Export a full private bundle before upgrading. Keep the previous binary until compatibility/recovery checks pass. Restore into a new directory and inspect unfinished work. Unknown attempts must survive upgrades. Release archives contain software; [workspace bundles](BUNDLES.md) contain private data; the secure host credential store contains access.
