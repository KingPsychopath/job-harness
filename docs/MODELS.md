# Optional native local models

The core Rust binary needs no models, Python, SDKs or GPU libraries. Local retrieval and typed decisions are opt-in capabilities supplied by the separate `job-harness-local` native provider. Writing stays with the user's agent or an explicitly selected compatible generative endpoint. No model proposal authorizes sending, archiving or changing career facts.

The standard Rust provider archive contains the gateway and metadata. It contains **no weights, Google libraries, Kev Swift scorer, Metal kernels or Python**. A full installation means explicitly adding the capabilities needed on that machine, with their requirements visible first.

## Direct sources and download scope

The sole weight catalog is [models.json](../providers/local/models.json); native library distributions are [runtimes.json](../providers/local/runtimes.json). [summary.json](../providers/local/summary.json) is a generated compact view. Weights come directly from pinned public publisher URLs. We do not host another copy or publish weights to GitHub. SDK archives use the publishers' wheel ZIP format; the native installer extracts only the named library or kernel and never runs pip or an interpreter.

| Profile | Reviewed weight download | Runtime | Native model execution verified |
|---|---:|---|---|
| EmbeddingGemma2 text 270M |164,626,432 bytes (157 MiB) |LiteRT-LM 0.18 C EmbeddingEngine, CPU |macOS arm64 |
| Laya s256 |678,477,844 bytes (647 MiB) |MediaPipe 1.1 DecisionMaker C ABI, CPU |macOS arm64 |
| Kev 0.8B |1,835,370,519 bytes (1.71 GiB), complete base/LoRA/head/tokenizer |separate native Swift/MLX scorer |Apple Silicon macOS 26+ for the exact verified kernel profile |

These are the only selectable model profiles. Larger candidates inspected during development were removed from setup and the catalog because their resource requirements exceeded the requested scope; they were never downloaded or run. The retained profiles still require the weight and runtime downloads shown above.

Google vendors publish native archives for macOS arm64, Linux x86_64/aarch64 and Windows x86_64. MediaPipe also publishes Windows arm64. Linux needs glibc 2.27 for LiteRT-LM and 2.28 for MediaPipe. Portable Rust contracts need none of these libraries; actual model execution on Linux/Windows remains unverified. There is no reviewed Gemma archive for macOS x86_64 or Windows arm64. The core's compatible HTTP endpoints remain available across its supported platforms.

Runtime downloads are separate from weights. The macOS LiteRT-LM archive is 21,430,649 bytes; MediaPipe is 42,559,246 bytes. Other platform sizes/hashes appear in `info --runtime ID`. Kev's exact verified macOS 26 Metal archive is 67,661,845 bytes. The separately compiled Swift scorer needs Swift 6.2+ and source dependencies fetched during an explicit build; their source/compile size is not a promised fixed download size. Older MLX targets exist upstream but require separately reviewed kernels.

RAM depends on hardware and input length. `working_memory_bytes:null` means unknown, not zero. `info` reports CPU/platform and physical RAM where available. The local trials ran on a 64 GiB Mac. Published model-card memory/latency claims are separate from local measurements.

Exact source and license evidence:

- **Gemma:** [converted model card](https://huggingface.co/litert-community/embeddinggemma-2-text-270m-litert-lm), revision `9be6e8b90982095dc05c2bd162e4b954ee4dbac7`; [exact artifact](https://huggingface.co/litert-community/embeddinggemma-2-text-270m-litert-lm/resolve/9be6e8b90982095dc05c2bd162e4b954ee4dbac7/embeddinggemma-2-text-270m.litertlm). Apache 2.0. Text only, 768 dimensions, 2,048 tokens. Uses the public [LiteRT-LM embedding C API](https://github.com/google-ai-edge/LiteRT-LM/blob/v0.18.0/c/embedding_engine.h).
- **Laya:** [official guide](https://ai.google.dev/edge/mediapipe/solutions/decision_maker), [exact GCS generation 1791231584156797](https://storage.googleapis.com/mediapipe-models/decision_maker/laya/float32/laya_s256/latest/laya_s256.task?generation=1791231584156797), [linked card](https://huggingface.co/litert-community/laya-LiteRT). The linked card declares Apache 2.0; GCS supplies no separate artifact license metadata, so that exact mapping remains an explicit uncertainty. The guide's 149M claim differs from the card's 421M/322M checkpoints; the artifact's parameter count is unverified. The [official 1.1 SDK](https://pypi.org/project/mediapipe/1.1.0/) contains the standalone library. [Official binding layouts](https://github.com/google-ai-edge/mediapipe/blob/master/mediapipe/tasks/python/decision/decision_maker.py) expose the actual typed C functions. No separate published DecisionMaker C header was located: this adapter therefore pins the exact archive/library hash and ABI sizes/offsets as an experimental interface.
- **Kev:** [0.8B card](https://huggingface.co/jaredpalmer/kev-0.8b), revision `bf75a6a8848ea6960ff2ed108d9ed44c2941174f`; [Qwen 3.5 base](https://huggingface.co/Qwen/Qwen3.5-0.8B-Base), revision `dc7cdfe2ee4154fa7e30f5b51ca41bfa40174e68`; both Apache 2.0. The [pinned author source](https://github.com/jaredpalmer/kev/tree/2ecafc49fe1e8945cd600443e6c04c1cf0b3a78f) defines branch encoding and pointer semantics. Native architecture reuse comes from MIT-licensed [mlx-swift-lm 3.32.3](https://github.com/ml-explore/mlx-swift-lm/tree/3b339ad6e3b3f44c8121ecff5131c7fd55e075e6) and [mlx-swift 0.32.3](https://github.com/ml-explore/mlx-swift/tree/19601207e9a0de51e03ee6ec0c3c5f3784275075). The complete catalog pins every component's size and SHA256.

## Explicit installation

Build or install the separate Rust provider, then inspect the selected model and its native runtime:

```sh
cargo build --release --locked --manifest-path providers/local/native/Cargo.toml
providers/local/native/target/release/job-harness-local --help
job-harness-local list
job-harness-local info --model laya-s256
job-harness-local info --runtime mediapipe-1.1.0-macos-arm64
```

CLI results use the core's version 2 `schema_version/ok/data/error/next` envelope. `info` supplies an approval token for the exact pinned scope. Review license, bytes, RAM unknowns and platform requirements, then substitute that token:

```sh
job-harness-local download --model laya-s256 --approve TOKEN_FROM_MODEL_INFO
job-harness-local download --runtime mediapipe-1.1.0-macos-arm64 --approve TOKEN_FROM_RUNTIME_INFO
job-harness-local verify --model laya-s256
job-harness-local verify --runtime mediapipe-1.1.0-macos-arm64
job-harness-local serve --decision-model laya-s256
```

The dashboard displays resource metadata and setup instructions. A profile's verification status describes the recorded development run; installation and endpoint availability on the current machine are separate checks. After starting the provider, select its endpoint, model and pinned revision explicitly for a real CLI request:

```sh
jh context triage --input selected-thread.txt \
  --endpoint http://127.0.0.1:18081/v1 --model laya-s256 \
  --revision gcs-1791231584156797
```

This returns a reviewable suggestion for the supplied text. It grants no archive or send permission.

For retrieval, repeat `info/download/verify` for `embeddinggemma2-text` and the matching `litert-lm-0.18.0-PLATFORM` runtime; add `--embedding-model embeddinggemma2-text` to `serve`. Select the platform's runtime ID from `list`. Only selected models load. Serving never downloads or installs anything.

The default cache is `$XDG_CACHE_HOME/job-harness/models` (normally `~/.cache/job-harness/models`), or the local application-data cache on Windows. `--cache /absolute/path` overrides it. Files verify before publication; a complete manifest is written last. Repeating an approved download retains complete verified files, restarts partial individual transfers and repairs interrupted manifests. Verification rejects missing, corrupt or unlisted model files. SDK verification rechecks the canonical archive hash and the extracted library against that archive before loading.

For an offline machine, copy complete model/version and native-runtime/version directories, including archives/manifests, from an already verified cache. Run `verify` on the receiving machine before `serve --cache ...`. Native libraries must match its OS/CPU; weights can be reused where a corresponding runtime exists. Copy only public model/runtime caches, never a private workspace or credentials. No hosted mirror is needed.

## Separate native Kev scorer

The standard Rust provider archive does **not** include this executable. Build it explicitly on Apple Silicon:

```sh
swift build --package-path providers/local/kev-native --build-system native -c release
job-harness-local info --model kev-0.8b
job-harness-local info --runtime kev-metal-0.32.3-macos-arm64
job-harness-local download --model kev-0.8b --approve TOKEN_FROM_MODEL_INFO
job-harness-local download --runtime kev-metal-0.32.3-macos-arm64 --approve TOKEN_FROM_RUNTIME_INFO
job-harness-local serve --decision-model kev-0.8b \
  --kev-sidecar /absolute/path/to/providers/local/kev-native/.build/release/kev-native \
  --kev-metallib /absolute/model/cache/kev-metal-0.32.3-macos-arm64/0.32.3/mlx.metallib
```

`Package.resolved` pins source dependencies. The tested compiler is Swift 6.4; the SDK declares a 6.2 floor. The legacy SwiftPM build mode avoids the default engine's request for a separately installed Metal compiler: the reviewed standalone kernel archive works without installing a global Metal toolchain. That build mode is deprecated upstream and may need a future toolchain update. The exact kernel profile requires macOS 26+. Record and verify the built scorer checksum when packaging/transferring it. Installing the Rust provider alone does not install Swift or build the scorer.

Native Kev reuses the published Qwen architecture and public post-final-norm hidden-state API. It preserves control-marker escaping, causal branches, CPU FP32 LoRA merge rounded once to BF16, calibrated FP32 pointer head, confidence formulas, rubric scale and nullable thresholds. The exact `head.pt` hash fixes four raw storage/name/shape mappings; only those checked ZIP entries are extracted locally. Pickle metadata is never parsed/executed, and Torch is not a dependency. Full causal rows are batched within 16,384 tokens; state is limited to 8,192 tokens, with no truncation. A generic chat model does not replace this head.

## Contracts, isolation and evidence

The serial gateway binds only `127.0.0.1`, requires the exact Host and rejects Origin. It exposes OpenAI-compatible `/v1/embeddings`, typed `/v1/decisions`, and fixed triage-only `/v1/chat/completions`. It rejects free-form generation, streaming, tools and audio. Embeddings are finite normalized 768-float vectors with explicit caller prefixes and native overflow errors. Typed requests accept 1–8 questions and 2–8 options/levels. Nullable booleans preserve unknown values within explicit lower/upper thresholds (default 0.2–0.8). Scores are zero-indexed rubric expectations, not universal grades; confidence is not an accuracy guarantee.

Inference runs in a fresh native process with bounded JSON IPC, a temporary directory and no inherited host API/AWS credentials. Native diagnostics are suppressed. HTTP input has an absolute read deadline; inference has a hard deadline. Failure kills the worker and requires an explicit restart; Unix group cleanup includes the Swift child. CtrlC/SIGTERM stops the selected worker. Core dumps are disabled on POSIX and Linux dumpability is disabled; Windows dump behavior remains unknown. Native SDKs still have the user's filesystem/network rights: this is crash/credential isolation, **not an OS sandbox**. Loopback is not authentication against another local process; a local caller can exhaust the request deadline and stop the non-respawning worker.

`--threads` sets Gemma's CPU API. Laya exposes no such field, so environment hints are advisory and do not enforce a thread count. The 16-schema Rust prewarm cache does not establish an audited bound for SDK internals. Unsigned native macOS execution was verified; a future hardened/notarized binary needs an independently verified library-loading policy.

Current native reports use fictional inputs and real Rust retrieval/triage calls:

| Measurement | Local result | Evidence |
|---|---:|---|
| Gemma 32-passage batch |1,277 ms |[native Gemma](../examples/models/native-gemma-2026-10-12.json) |
| Gemma five warm queries |41–46 ms |same report; SDK vectors match exactly: max difference 0/cosine 1 |
| Laya five typed questions, warm median |352 ms |[native Laya](../examples/models/native-laya-2026-10-12.json); direct SDK probabilities match exactly |
| Kev five typed questions, warm median |294 ms |[native Kev](../examples/models/native-kev-0.8b-2026-10-12.json) |
| Kev Rust host / Swift scorer peak RSS |182 MB /1,893 MB |separate processes; MLX allocation peak 2,440 MB is a different counter |

Kev's 12 token sequences and pointer positions match the author encoder. Fixture choices and boolean/nullable outcomes matched the earlier SDK reference; raw probability drift reached 0.0217 with different BF16 kernels/cache execution. The earlier Python/MLX cached 26 ms result is a historical reference, not a native measurement. Six cases do not establish accuracy or calibration. Laya retains wrong labels/urgencies and abstentions; genuine reversed order changed two urgency choices. Its old prototype sorted schema-cache keys and failed to reverse cached options, so that historical report is annotated. All proposals remain review-only, with an independent request guard and no archive permission.

Checks need no SDKs, weights, network or Python:

```sh
cargo test --locked --manifest-path providers/local/native/Cargo.toml
cargo clippy --locked --manifest-path providers/local/native/Cargo.toml --all-targets -- -D warnings
job-harness-local doctor
```

Explicit real probes require already verified runtimes/weights:

```sh
job-harness-local probe --model laya-s256 --endpoint http://127.0.0.1:18081/v1 --output report.json
job-harness-local encoding-check --cache /absolute/model/cache --output encoding.json
```

Add `--harness /absolute/path/to/job-harness` for real core integration. [Example notes](../examples/models/README.md) describe fixture coverage and historical limitations.
