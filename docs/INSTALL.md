# Install and leave Job Harness

Start with the **core release** and your existing agent. No compiler, model download or new AI account is needed. Your private career workspace is a separate folder containing your CV, answers, notes and application history; the toolkit repository contains reusable software.

| You want to… | Start here |
|---|---|
| Use an existing agent | Download the core archive, set up a private workspace, then connect MCP or use the CLI |
| Use a window as well | Add the separate desktop app; it opens an existing initialized workspace |
| Open the repo in an agent project | Clone the toolkit and follow [the project setup path](AGENTS.md#open-the-checkout-in-your-agent) |

Models, their runtimes and Typst are optional. [Connect your agent](AGENTS.md) · [Add your first sources](../resources/guides/job-harness-onboarding.md) · [Leave or switch tools](#leave-or-switch-tools)

## Download the core: no compiler required

Get your archive from [GitHub Releases](https://github.com/machineyearningengineer/job-harness/releases). This repository and its releases are private; use your existing GitHub access. Choose the **`job-harness-vVERSION-TARGET`** archive for the core. Archives prefixed `job-harness-desktop` and `job-harness-local` are separate optional components.

| System | Target |
|---|---|
| Apple Silicon macOS | `aarch64-apple-darwin` |
| Intel macOS | `x86_64-apple-darwin` |
| Linux x86-64 | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |
| Windows x86-64 | `x86_64-pc-windows-msvc` |

The [v0.2.0 preview](https://github.com/machineyearningengineer/job-harness/releases/tag/v0.2.0) includes all five targets. **Windows packages are experimental cross-builds and have not been executed on Windows.** An intermittent notification-test stall is tracked in [issue 1](https://github.com/machineyearningengineer/job-harness/issues/1). See [platform validation](VALIDATION.md).

1. Download the matching archive and `SHA256SUMS`.
2. Compare its exact entry with `shasum -a 256 ARCHIVE` on macOS, `sha256sum ARCHIVE` on Linux, or `Get-FileHash ARCHIVE -Algorithm SHA256` in PowerShell. SHA256 checks whether bytes changed; it does not replace trust in the publisher or signing.
3. Extract it. Put `job-harness` and `jh` in your chosen user bin directory and add that directory to PATH. On Windows both names end in `.exe`.
4. Run `jh --version`. Keep the archive's `BUILD.json` if you need its source commit and target later.

These preview archives are **unsigned**. Review Gatekeeper/SmartScreen prompts using your platform's normal explicit approval; do not disable global security controls. Public package registries, signed installers and auto-update are not enabled.

Windows executables require the Microsoft Visual C++ x64 runtime; the desktop also requires WebView2. Use Microsoft's [supported redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist) and [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). Archives do not bundle them.

## Set up one private folder

On macOS/Linux:

```sh
jh setup --workspace "$HOME/Career/workspace"
jh --root "$HOME/Career/workspace" status
jh --root "$HOME/Career/workspace" connect mcp
```

In PowerShell:

```powershell
jh setup --workspace (Join-Path $HOME "Career/workspace")
jh --root (Join-Path $HOME "Career/workspace") status
jh --root (Join-Path $HOME "Career/workspace") connect mcp
```

Setup creates a new/empty folder or selects an existing valid workspace. It saves that location as a host-local default; it does not reset career records or previous submissions. `--root` explicitly selects the data folder for each later command. Use paths outside the toolkit checkout.

Terminal setup asks which optional capabilities you want. An agent or script can run without prompts:

```sh
jh setup --workspace "$HOME/Career/workspace" --capability mcp --json
jh setup --workspace "$HOME/Career/workspace" --capability retrieval --plan --json
```

`--plan` creates no workspace and saves no settings. Capability selection describes next steps; it never downloads a model, connects an account or grants action permission. Without selections, noninteractive setup configures only the base.

The last command in the first examples prints the executable and arguments for an MCP-capable host. Copy `data.command` and `data.args` into its server configuration, adapting the outer format to that host. It does not register itself automatically. [MCP or direct CLI connection](AGENTS.md)

For a trial, use `jh demo --workspace "$HOME/Career/demo"`, then `jh --root "$HOME/Career/demo" gui`. The demo is fictional, refuses an existing nonempty folder and leaves your default selection unchanged.

## Add your existing CV and answers

Follow [first-source onboarding](../resources/guides/job-harness-onboarding.md#add-your-first-sources). Supplied-file import preserves the original and reports its path; it does not approve facts or select your primary CV. Current source accepts a reviewed imported PDF as `primary_cv`; the unchanged v0.2.0 archive needs the compatibility step below.

Set the workspace's timezone and confirmed answer sources in `config/career-ledger.json`. Record answers with their source, date and scope. An existing agent can help organize supplied material; no questionnaire or built-in model is mandatory.

### v0.2.0 CV-path compatibility

The published v0.2.0 engine rejects `sources/imports/...` as an active `primary_cv` path. Its source-import command still preserves the original correctly. Until using the source fix, copy the reviewed PDF byte-for-byte to a **new** `output/approved-primary.pdf` inside your private workspace and set `primary_cv` to `output/approved-primary.pdf`. Keep the imported original. Do not overwrite an existing approved CV silently. This workaround does not change the published archives.

## Add only the optional pieces you want

| Goal | Additional requirement |
|---|---|
| Use your existing MCP or command-running agent | None in Job Harness; its own authentication and connectors remain in that host |
| Use a native window | Separate [desktop package](../apps/desktop/README.md) and its OS webview requirements |
| Launch an ACP agent | Separately installed/authenticated agent; [connection guide](AGENTS.md#optional-acp-handoff) |
| Use a compatible generative model directly | Your selected local runtime or remote provider; [model connection](AGENTS.md#optional-direct-model-connection) |
| Add semantic search or small local decision models | Optional provider plus explicitly selected [runtime/model downloads](MODELS.md) |
| Generate a CV with Typst | Optional compiler and fonts; [CV guide](CV.md) |

On macOS, move **Job Harness.app** from the desktop archive into Applications and open it. Windows/Linux use the separate desktop executable and required webview. The app selects an existing initialized folder; create it with the core CLI or ask your agent to do that once. Normal GUI use does not require typing commands. The core's browser dashboard is also available through `jh --root PATH gui`.

### The dashboard opened as a local file

`gui/index.html` is a renderer template, not a standalone app. A `file://` page cannot select your workspace or connect your agent. Open the desktop app, or run `jh --root PATH gui` and open the local URL it prints.

## Build from the toolkit checkout

Release users can skip this section. Source builds need [Rust](https://rustup.rs), the platform C toolchain and your GitHub access. The project pins Rust 1.97.1; core minimum Rust is 1.88. Optional desktop compilation has separate requirements.

```sh
gh repo clone machineyearningengineer/job-harness
cd job-harness
cargo build --release --bins --locked
./target/release/jh setup --workspace "$HOME/Career/workspace" --json
./target/release/jh --root "$HOME/Career/workspace" connect mcp
```

In PowerShell the executable is `.\target\release\jh.exe`; use `(Join-Path $HOME "Career/workspace")` for the workspace. You can open this checkout in your agent and use [the short setup prompt](AGENTS.md#open-the-checkout-in-your-agent).

For a persistent source installation, `./scripts/install.sh` or `./scripts/install.ps1` uses `cargo install --locked` from this checkout. The default destination is `~/.local/bin`; the shell script accepts `--prefix PATH` and PowerShell accepts `-Prefix PATH`. Add the selected bin directory to PATH. These scripts build the core; they do not initialize private data, install models or register an agent connection. Embedded guides make the installed binary independent of the checkout.

## Upgrade or change your agent

Keep the previous executable until your private backup restores and the new binary reads it correctly. Reload the agent host after changing its server command, then call `bootstrap` through that actual connection. Saved configuration does not establish that an old process restarted. The default focused MCP profile starts with 12 tools; use `inspect_tools` to discover specialized schemas. Full mode exposes the complete catalog.

The CLI selects `--root` first, then `JOB_HARNESS_WORKSPACE`, then its saved default. [Agent setup](AGENTS.md) explains fixed workspaces and access boundaries. A new agent can use the same private folder without rebuilding your history.

## Leave or switch tools

**Keep your private workspace by default.** Removing an executable or disconnecting an agent does not require deleting your career history. There is no Job Harness uninstall/delete-workspace command.

1. Pause agents, GUI edits and any host schedules that write to this workspace. Close the desktop/browser dashboard so the snapshot is stable. If work is unfinished, inspect it and preserve uncertain submission outcomes; do not clear history to make a backup pass.
2. Export a **full** private backup to a new filename outside both the workspace and toolkit checkout. A profile-only export omits application history and is unsuitable for continuing a job search.
3. Verify it by restoring to a separate new/empty directory, then read its status and check the rebuilt view:

```sh
jh --root "$HOME/Career/workspace" workspace export --output "$HOME/Career/backups/before-removal.jhb"
jh --root "$HOME/Career/restore-check" workspace import --input "$HOME/Career/backups/before-removal.jhb"
jh --root "$HOME/Career/restore-check" status --json
jh --root "$HOME/Career/restore-check" pipeline --check
```

Import checks the archive's declared files and SHA256 values; `pipeline --check` checks the generated application view, not the entire backup. Inspect important records, supplied files and unfinished work in the restored folder before choosing what to remove. An unresolved local journal can block export; use the existing recovery process instead of deleting its state. Bundles contain private plaintext data; protect or encrypt them before transferring them. [Backup format and recovery](BUNDLES.md)

4. Remove only the Job Harness server entry from your agent host and reload it. Stop its existing MCP/ACP/model-provider processes using that host or the terminal that launched them; closing a page alone may leave a host-managed server running. There is no automatic credential revocation.
5. Remove components manually only if you want to:

| Component | What to remove | What to preserve |
|---|---|---|
| Core CLI | Your installed `job-harness`/`jh` files (or `.exe`), from the bin directory you selected | Private workspace and backups |
| Desktop | The chosen app/executable; on macOS `Job Harness.app` in Applications | The same private workspace |
| Local model provider | The separately installed `job-harness-local` executable | Other agents/runtimes and data you still use |
| Optional model/runtime cache | Only chosen model/version or runtime/version folders in your selected cache | Private career data; caches needed by another tool |
| Saved default folder | Only Job Harness's `settings.json`, if you want to forget the selected location | The folder that file points to |
| Toolkit checkout | The cloned code folder, if no longer needed | All private data and backups kept outside it |

Host settings are `$XDG_CONFIG_HOME/job-harness/settings.json` when set; otherwise `%APPDATA%\job-harness\settings.json` on Windows or `~/.config/job-harness/settings.json` on macOS/Linux. Remove your own `JOB_HARNESS_WORKSPACE` host override if retiring that selection; the desktop also accepts the legacy `JOB_HARNESS_ROOT` override.

The provider cache uses `$XDG_CACHE_HOME/job-harness/models`, otherwise `%LOCALAPPDATA%\job-harness\models` on Windows or `~/.cache/job-harness/models`; an explicit provider `--cache PATH` overrides it. Stop that provider before manually removing selected cache folders. Job Harness does not uninstall Typst, your agent host or system webview/runtime dependencies.

These are owner-controlled steps, not a deletion script. Keep the original workspace and verified backup until you deliberately decide otherwise. If the restore-check copy becomes your active workspace, stop use of the original and select one canonical folder; two copies are not synchronization.
