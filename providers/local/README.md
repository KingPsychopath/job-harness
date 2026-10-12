# Optional native local models

This separate Rust binary runs real text retrieval and typed decisions through published native runtimes. The core Job Harness binary does not depend on it, Python, model weights or GPU libraries.

```sh
cargo build --release --locked --manifest-path providers/local/native/Cargo.toml
providers/local/native/target/release/job-harness-local --help
```

Start with `list`, then `info --model ID` and `info --runtime ID`. Every download requires the approval token from the reviewed metadata. `serve` only loads verified cached files and never downloads.

The selectable profiles are EmbeddingGemma2 text (157 MiB weights), Laya s256 (647 MiB), and optional Kev 0.8B (1.71 GiB complete weights, with a separate Apple Silicon scorer). Native runtimes add their own downloads and hardware requirements, shown by `info` before installation.

The canonical weight catalog is [models.json](models.json); standalone native library archives are [runtimes.json](runtimes.json). [summary.json](summary.json) is a generated compact view for the core CLI and desktop. Regenerate it with `job-harness-local summary --output providers/local/summary.json`; Rust tests check it matches the catalogs.

Google engines use direct version-pinned C bindings. The optional Kev scorer is a separately compiled Swift/MLX executable; the standard Rust provider archive does **not** include that executable or its Metal kernels. Neither path executes Python. Runtime archives happen to use the upstream SDKs' wheel ZIP format; the installer extracts only the named native library/kernel, never Python modules, pip or an interpreter.

Full requirements, direct model sources, licenses, explicit installation, offline cache transfer, measured limitations and the separate Kev build are in [docs/MODELS.md](../../docs/MODELS.md).

Checks need no SDKs or downloads:

```sh
cargo test --locked --manifest-path providers/local/native/Cargo.toml
cargo clippy --locked --manifest-path providers/local/native/Cargo.toml --all-targets -- -D warnings
```
