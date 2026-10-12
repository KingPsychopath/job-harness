# Job Harness desktop

An optional native window for an existing private workspace. It uses the same
Rust engine, fixed loopback routes and frontend as `job-harness gui`; no second
tracker or cloud account is created.

The app keeps the operating system's window decoration and follows its theme.
Applications open on the board, with an optional table. Click a card to open its side inspector, or drag to move between eligible stages. A legal move updates the shared record immediately; submission and stale-source guards still apply. Attention, source search, activity and connection metadata are secondary panels. Note review
compares the immutable original with individually selected, cited proposals.

## Build and run

The main CLI does not need this package. This directory is a separate Cargo
workspace with its own lockfile. Desktop builds require Rust **1.90 or newer**
(the pinned project toolchain is 1.97.1), a platform C/C++ toolchain, and the
platform webview described in [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/):

- macOS: Xcode command-line tools and the system WebKit.
- Windows: the Visual Studio C++ build tools and Microsoft WebView2.
- Linux: the distro's GTK3, WebKitGTK 4.1 and Tauri build dependencies.

```sh
cargo build --release --manifest-path apps/desktop/Cargo.toml --locked
apps/desktop/target/release/job-harness-desktop --root /absolute/private/workspace
```

On Windows, use `apps\desktop\target\release\job-harness-desktop.exe`.
Open the app normally. It uses an explicit `--root`, then
`JOB_HARNESS_WORKSPACE` (or the legacy `JOB_HARNESS_ROOT`), then the CLI's
private host setting. If no ready workspace is selected, a native folder chooser
lets you select an existing one. It never initializes or resets that folder.
Remembering the location is optional and stores only a path in owner-only host
settings, shared with the CLI.

The native app enables its named, reviewed local edits like a document app.
Choose **Workspace → Read only** to lock them, or launch with `--read-only`.
The menu is owned by Rust; the page cannot change this policy. The legacy
`--allow-local-writes` flag remains accepted. Browser dashboards and agent hosts
retain their separate read-only defaults.

Review mode enables preserving supplied notes, drafting sourced proposals,
accepting individually selected items, archiving retained notes and reviewed
tracker updates. It grants no employer submission, email, calendar, model
download or general tool-execution route. Board moves to Applied or Materials
ready require their recorded submission or current prepared materials.

## Local access

One listener binds **127.0.0.1** on an OS-assigned port. The webview uses that
exact origin. A one-use launch capability establishes a private cookie and a
tab-scoped request nonce; the capability is immediately removed from the page's
URL. Cookie-only access cannot obtain that nonce. Host, Origin, Fetch Metadata
and nonce checks protect every data route, with additional checks on writes. Closing the app closes its in-process server. The webview
uses an ephemeral private data store.

There are no Tauri invoke commands, plugins or frontend IPC capabilities.
Off-origin navigation and new webview windows are denied; explicit external
links open in the user's browser when the platform's new-window handler fires.
A **Copy link** control remains available. Downloads and permission requests
(camera, microphone, geolocation and others) are denied. The app retains the
standard Edit menu so normal text selection and clipboard shortcuts work.
See [Tauri capabilities](https://v2.tauri.app/security/capabilities/) and the
[webview handlers](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html).

## Checks and packaging

```sh
cargo fmt --manifest-path apps/desktop/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked
```

The source build, native read-only and opted-in review flows were checked on macOS using fictional records. Windows and
Linux native-window runtime acceptance remains a platform check; a CLI's
cross-platform CI result alone is not proof of the platform webview. This
version provides the standalone native binary and browser mode. Release archives
may include an unsigned macOS app. Signed/notarized apps, MSI and distro installers
are separate packaging work; this source package does not claim signed installers.

Compiled executable size and download requirements depend on target and build
profile. Model weights and provider runtimes are separate, opt-in capabilities;
they are not installed with the desktop package.
