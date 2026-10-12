# Develop and verify

Use the pinned Rust toolchain. The declared Rust 1.88 minimum is checked separately; native development/release uses 1.97.1. Core runtime commands are native; Python is used by development checks, graphics generation, the frozen compatibility oracle and a separate optional provider for released model SDKs.

```sh
make check          # formatting, Clippy, Rust behavior/protocol/CLI tests and repository gate
make check-parity   # historical Python reference against Rust, synthetic data only
make check-package  # release binary copied outside checkout, CLI/MCP/bundle smoke checks
make assets         # original SVG graphics
```

Crates divide pure vocabulary, native workspace operations, host/protocol adapters and CLI. Engine capabilities are focused modules. Generic resources have one source at `resources/`; include macros embed them. Schema/envelope/on-disk versions are compatibility contracts. [Architecture](ARCHITECTURE.md)

Tests exercise one-submit replay, perpetual unknown blocking, caps, concurrent reservations, journal recovery/conflicts, source changes, typed errors, partial batches, bundle corruption/traversal, derived search, actual MCP frames, ACP handoff and fake model HTTP. They use disposable synthetic workspaces, never real job sites or mailboxes.

Historical Python code under `tests/oracle/` is a frozen test reference, excluded from installation and release artifacts. It is not invoked by the Rust runtime. Keep migration vectors for Unicode identities, numeric encoding, dates and old transfer formats.

CI has read-only repository permissions and pinned actions. Native checks run on Windows/Linux/macOS, plus a pure-domain WASM build. Private release automation produces checksummed archives; it does not publish Cargo/npm packages or change repository visibility. Review exact-head CI results before claiming remote success.

Local checks, remote CI, installed-client execution and live provider actions are separate evidence. The local dashboard defaults to read-only; named local review routes require an explicit host flag. Remote tenancy, Windows behavior and live model/provider correctness require their own verification.
