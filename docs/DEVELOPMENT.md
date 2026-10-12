# Develop and verify

Use the pinned Rust 1.97.1 toolchain and a platform C compiler. Core minimum Rust is 1.88; the optional desktop has its own minimum. Ordinary checks need no model weights, vendor SDKs or real candidate data.

**GitHub Actions are manual-only.** Pushes, pull requests and tags start no jobs. This preserves the owner's limited Actions minutes; it does not remove regression tests or turn local checks into remote CI proof. Run the relevant checks locally first, and request a remote platform check only when it resolves a concrete gap.

## Start with the change you made

| Change | Local check |
|---|---|
| Prose, links or generic resource files | `cargo xtask repository` |
| Tool schemas or generated declarations | `cargo xtask contracts --check` plus tests for the changed contract |
| A focused core behavior | `cargo test -p job-harness-core --locked TEST_FILTER` |
| Pure domain behavior | `cargo test -p job-harness-domain --locked` |
| MCP, ACP, model-host or dashboard adapter | `cargo test -p job-harness-protocol --locked TEST_FILTER` |
| CLI behavior | `cargo test -p job-harness --locked TEST_FILTER` |
| Installed resources or packaging | Build the binary and run `cargo xtask package --binary PATH` |

`TEST_FILTER` is a test-name/module substring; select it from the changed code. For example, `cargo test -p job-harness-core --locked state::tests` checks canonical-state validation. Run connected failure/recovery tests when changing state or external-action coordination. A resource-only edit still needs package verification before claiming the installed binary contains it.

Once the change is ready, the normal local core gate is:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo xtask repository
cargo xtask contracts --check
```

These commands work on Windows, macOS and Linux. `make check` is a convenience alias for this full local gate. `make check-parity` runs the retained native compatibility/invariant suites; it invokes no historical runtime. Avoid repeating passed checks unless a new change or failure justifies it.

The repository gate checks source inventory, links, skills and data boundaries. It rejects Python source, private files, runtime libraries and model weights. Tool declarations are generated from authoritative JSON schemas with `cargo xtask contracts`; use `--check` to verify without rewriting them.

## Check a copied package only when needed

Source checks share the debug build; use it for a quick installed-interface check without recompiling an optimized profile:

```sh
cargo build --bins --locked
cargo xtask package --binary target/debug/job-harness
```

For an actual optimized core artifact:

```sh
cargo build --release --bins --locked
cargo xtask package --binary target/release/job-harness
```

Append `.exe` to the binary path on Windows. `make check-package` uses the optimized build. The package verifier copies the executable outside the checkout with isolated child configuration, then checks CLI behavior, embedded guides, supplied files/documents, full backup/restore and an official SDK MCP client in focused/full modes. It uses fictional data and does not inspect a real career workspace.

Tests retain one-submit replay, perpetual unknown blocking, concurrent reservations, journal conflicts/recovery, changed-source guards, partial batches, bundle corruption/traversal, derived indexes, selective note review and adapter boundaries. The migration was checked against 49 historical cross-language traces; native vectors now retain that compatibility without a retired runtime dependency.

SVGs in `docs/assets/` are editable sources. `make assets` optionally previews them with `rsvg-convert`. `jh demo --workspace PATH` creates a fictional searchable workspace; it performs no external actions and leaves the saved workspace selection unchanged.

## Optional component checks

Desktop and provider have separate manifests and locks:

```sh
cargo fmt --manifest-path providers/local/native/Cargo.toml -- --check
cargo clippy --manifest-path providers/local/native/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path providers/local/native/Cargo.toml --locked

cargo fmt --manifest-path apps/desktop/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path apps/desktop/Cargo.toml --locked
```

`make check-models` and `make check-desktop` are aliases. Read [desktop requirements](../apps/desktop/README.md) before its build. For a copied optional binary, build that manifest and use `cargo xtask package --component local|desktop --binary PATH` with the appropriate component name and its actual output path.

Provider contract tests do not download or run models. [Model lifecycle and real probes](MODELS.md) cover that separate opt-in work. A desktop compile does not prove native window behavior on another OS.

## Choose a manual CI profile

The **Manual verification** workflow runs only through `workflow_dispatch`. Its defaults are `platforms=linux` and every boolean below `false`: one Linux root-workspace format/lint/test/resource/contract job, with no desktop, provider or package build.

| Input | Additional work |
|---|---|
| `platforms=linux`, `macos`, `windows` or `all` | One selected native OS, or all three. `all` repeats the core suite on three runners. |
| `desktop=true` | A separate desktop lint/test job for each selected OS, including Linux webview dependencies. |
| `models=true` | Provider lint/tests inside each selected core job; no weights or SDK downloads. |
| `packaging=true` | Build and verify copied **debug** core binaries, plus provider/desktop binaries when those components are selected. |
| `extended=true` | WASM domain and Rust 1.88 workspace checks on Linux, if Linux is selected. |
| `source_install=true` | Windows source installer and installed aliases, if Windows is selected. This explicitly adds an optimized rebuild. |

A default dispatch creates one job. `platforms=all` creates three core jobs; adding desktop makes six. The other flags add steps and compilation time to those jobs. Caches reduce repeated downloads/builds, but cold builds still cost time. Timeouts cap individual jobs; they are not estimates of consumed minutes.

Select the smallest profile that answers the question: Linux for ordinary core changes, a selected OS for an OS-specific fix, or all platforms and relevant components for a release acceptance pass. CI has read-only repository permissions and pinned actions. No automatic workflow checks appear on a PR; report which local or explicitly requested remote checks actually ran. An owner may still require a manual platform check before accepting a change.

## Release one source identity explicitly

The **Private native release** workflow is a separate, deliberately heavy manual action. It accepts an existing `tag` and requires `confirm_full_release=true`; its default `false` starts no build or publication jobs. Pushing a tag alone does nothing.

Before requesting it, commit a clean source tree, verify the relevant local/manual checks and create a version tag matching package versions. An authenticated read-only preflight verifies private scope and refuses a tag that already has a release before starting the five native builds; authentication, network or response errors also stop it. The workflow resolves that tag once, builds the same commit on five native targets, tests and verifies all three components, and stages **15 archives**. It has a source-resolution job, five build jobs and one publish job. This full release path is not the normal source-check profile.

The build checks that the runner's native Rust target matches the intended target. `release-stage` checks CPU/OS headers, package version and clean source metadata; staging excludes private bundles, profiles, credentials, model weights and SDK runtimes. The publish job checks that the repository is still private and the tag still resolves to the selected commit, verifies checksums, then creates a private prerelease. It neither moves a tag nor overwrites an existing release. Only publication receives repository write permission.

For local software-only staging:

```sh
cargo xtask release-stage --component core \
  --tag v0.2.0 --target aarch64-apple-darwin \
  --binary target/release/job-harness --output dist
```

Replace the example version/target with the clean source's package version and native target. Local experiments require `--allow-dirty` and are marked accordingly; they are not release proof. Public registries, repository visibility changes, signing and credential changes remain separate owner decisions.

When Actions cannot run, an explicitly approved private preview can combine previously verified native artifacts with clean-source local builds. Preserve the tag, verify each archive's identity and checksum, and publish target-specific validation alongside them. Cross-built binaries must state that they were not executed; known defects stay visible. This does not replace native platform checks for a stable release. [v0.2.0 validation](VALIDATION.md)

Keep local tests, remote CI, copied/installed executable checks and live provider/UI behavior distinct in the handoff.
