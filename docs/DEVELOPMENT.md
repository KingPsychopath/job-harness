# Develop and verify

The reusable code and development gates are native. Use pinned Rust 1.97.1 and a platform C compiler. Core MSRV is 1.88; the optional Tauri application declares its own minimum. Model runtimes and weights are not needed by the normal suite.

## Portable commands

These work without Make on Windows, macOS and Linux:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo run -p job-harness-dev --locked -- repository
cargo run -p job-harness-dev --locked -- contracts --check
cargo build --release --bins --locked
cargo run -p job-harness-dev --locked -- package --binary target/release/job-harness
```

On Windows append `.exe` to the package verifier's binary path. `make check`, `make check-package` and `make demo` are convenience aliases. `make check-parity` runs the retained native compatibility/invariant suites; it invokes no historical runtime.

The repository gate inventories Git-tracked/non-ignored source, checks data/resource/link boundaries and rejects Python source, runtime libraries, weights and private files. TypeScript declarations are generated from authoritative JSON schemas with `cargo run -p job-harness-dev -- contracts`.

SVG graphics are editable source in `docs/assets/`. `make assets` optionally previews them with `rsvg-convert`; the generated preview is not another authority. Screenshots contain fictional data. The native `jh demo --workspace PATH` reproduces a searchable workspace without any external action.

## Meaningful behavior checks

Tests cover one-submit replay, perpetual unknown blocking, caps, concurrent reservations, journal conflicts/recovery, source changes, partial batches, bundle corruption/traversal, derived indexes, source-preserving intake, selective review, documents, actual MCP frames, ACP handoff and fake model HTTP.

The completed migration was checked against 49 historical cross-language traces before retiring the old runtime source. Native golden vectors retain Unicode/numeric encoding, identity, URL and pipeline compatibility. The legacy code remains in Git history, not in the product or test dependency graph.

`xtask package` copies the binary outside the source tree with a clean environment, checks CLI, embedded resources, supplied-file/document flow, exact backup/restore and an official SDK MCP client in focused/full modes. It does not inspect a real candidate workspace.

## Optional components

The desktop and native provider use separate manifests/locks and do not enlarge the default CLI installation:

```sh
cargo test --manifest-path providers/local/native/Cargo.toml --locked
cargo test --manifest-path apps/desktop/Cargo.toml --locked
```

Use [model lifecycle and real probes](MODELS.md) and [desktop requirements](../apps/desktop/README.md). An SDK-free contract test is not proof a model ran; a compiled desktop binary is not proof of each operating system's window behavior.

## Release one source identity

Commit a clean source tree, verify exact-head CI, create a version tag matching workspace package version, then run the private native release workflow. `xtask release-stage` checks native CPU/OS headers and stages only selected software with source metadata and SHA256. Local experiments require `--allow-dirty` and are marked accordingly; they are not release proof.

```sh
cargo run -p job-harness-dev -- release-stage \
  --tag v0.2.0 --target aarch64-apple-darwin \
  --binary target/release/job-harness --output dist
```

CI actions are pinned and normal jobs have read-only repository access. Publication receives only the permission needed for private release assets. No candidate bundle, model weight, secret or private profile enters an artifact. Public Cargo/npm/Homebrew publication and repository visibility changes remain owner decisions.

An owner-approved manual preview can reuse verified native artifacts and clean-source local builds when Actions cannot run. Preserve the existing tag, verify every archive's source identity and checksum, and publish a per-target validation manifest alongside the checksums. Cross-built binaries must say that they were not executed; known runtime defects must remain visible. This does not replace native platform checks for a stable release. See the [v0.2.0 validation record](VALIDATION.md).

Keep local tests, remote CI, installed-client execution and live provider evidence distinct in the handoff.
