# Optional local retrieval and typed decisions

The Rust core, CLI, MCP and ACP work without Python, model weights or a model account. Exact-word search and inbox rules remain available offline. This separate provider is useful when a question needs related passages rather than exact words, or a selected message needs a bounded decision for review. An agent handles writing and external actions.

The implementation calls Google's released SDKs: [LiteRT-LM `EmbeddingEngine`](https://github.com/google-ai-edge/LiteRT-LM/blob/main/python/litert_lm/embedding_engine.py) for text-only EmbeddingGemma 2 retrieval, and [MediaPipe `DecisionMaker`](https://developers.google.com/edge/mediapipe/solutions/decision/decision_maker/python) for Laya choice, boolean and rubric scores. Nullable booleans abstain between explicit probability thresholds. It does not reimplement a model, generate text, read a workspace or capture audio.

## Review before downloading

Metadata was checked on 12 October 2026. `list` and `info` use the standard library and never load SDKs, contact a provider or download weights.

| Selected model | Reviewed weight file | Source identity | Working memory |
|---|---:|---|---|
| `embeddinggemma2-text` | 164,626,432 bytes / 157 MiB | [Text-only 270M card](https://huggingface.co/litert-community/embeddinggemma-2-text-270m-litert-lm), revision `9be6e8b90982095dc05c2bd162e4b954ee4dbac7`, SHA256 pinned | Unknown before loading; measure locally |
| `laya-s256` | 678,477,844 bytes / 647 MiB | Official MediaPipe bundle, GCS generation `1791231584156797`, SHA256 and MD5 pinned | Unknown before loading; measure locally |

Both linked model repositories declare Apache-2.0. Review their cards and license evidence in `info`; the Laya GCS object has no separate license metadata. The MediaPipe overview's parameter count differs from its [linked Laya conversion card](https://huggingface.co/litert-community/laya-LiteRT), so this catalog leaves the bundle's parameter count unknown. No terms or authentication are accepted automatically.

This adapter selects CPU and supports macOS arm64 and Linux with compatible published wheels. Metadata commands and the synthetic HTTP/rules tests are portable on Windows; the optional hardened native worker currently uses POSIX descriptors and reports unsupported there. The Windows Rust core can still use an independently selected compatible external model endpoint. The combined pinned SDKs require macOS 12 or a compatible Linux wheel platform (MediaPipe's Linux wheels target glibc 2.28). GPU/NPU acceleration is untested. Package downloads are additional to model weights; the optional environment contains MediaPipe's published dependencies, including unused vision/audio packages.

## Explicit lifecycle

From the repository, create a separate optional environment:

```sh
uv venv --python 3.12 providers/local/.venv
uv pip install --python providers/local/.venv/bin/python \
  -r providers/local/requirements-runtime.txt -e providers/local

providers/local/.venv/bin/job-harness-local list
providers/local/.venv/bin/job-harness-local info embeddinggemma2-text
providers/local/.venv/bin/job-harness-local info laya-s256
```

The same installation works with `python3.12 -m venv providers/local/.venv` and that environment's `python -m pip install -r ... -e ...`. Never install into global Python. Without the optional SDKs, metadata also works with `PYTHONPATH=providers/local python3 -m job_harness_local list`.

After reviewing the displayed size, source, license and hardware unknowns, pass the corresponding `approval_token` explicitly:

```sh
providers/local/.venv/bin/job-harness-local download embeddinggemma2-text --approve TOKEN_FROM_INFO
providers/local/.venv/bin/job-harness-local download laya-s256 --approve TOKEN_FROM_INFO
providers/local/.venv/bin/job-harness-local serve \
  --embedding-model embeddinggemma2-text --decision-model laya-s256 \
  --port 18081 --threads 2 --timeout 30
```

Models go to `$XDG_CACHE_HOME/job-harness/models` or `~/.cache/job-harness/models`. `--cache-dir /chosen/path` before the command selects another local cache. Download sizes and digests must match before installation completes; `serve` verifies the pinned bytes again and never fetches missing files. Ctrl-C stops the service and releases its worker; removing the selected cache directory removes those weights.

The service binds `127.0.0.1` only, rejects browser origins and mismatched Host headers, and does not log input or native diagnostic text. Requests have a one-MiB bound; inference is serialized in one separate worker with a hard deadline. The worker starts with a fresh minimal environment, a temporary working directory, isolated Python imports, bounded JSON IPC and disabled core dumps. It remains a process with the same user, filesystem and network rights; this is crash isolation, not an execution sandbox. Timeout kills that worker, returns unavailable and requires an explicit restart. No automatic retry occurs. `/health` reports SDK versions, startup time, peak worker RSS and CPU controls; `/v1/models` reports loaded revisions. Successful responses also advertise `model_revision` for compatible hosts that verify provider identity.

`--threads` sets LiteRT-LM's actual embedding CPU thread count. MediaPipe's Python DecisionMaker exposes no thread-count field: Laya receives thread environment hints. Linux additionally bounds the worker's CPU affinity; macOS has no affinity control here. **A two-thread Laya limit is not claimed on macOS.**

## Connect retrieval to the Rust harness

Choose sources explicitly. The provider embeds the supplied strings without adding another task prefix. Use the same prefixes, endpoint, revision and chunk size when indexing and searching:

```sh
job-harness context index --endpoint http://127.0.0.1:18081/v1 \
  --model embeddinggemma2-text --revision 9be6e8b90982095dc05c2bd162e4b954ee4dbac7 \
  --source context/projects/example.md --chunk-lines 20 \
  --document-prefix 'title: none | text: ' \
  --query-prefix 'task: search result | query: ' --timeout-seconds 30

job-harness context search --endpoint http://127.0.0.1:18081/v1 \
  --model embeddinggemma2-text --revision 9be6e8b90982095dc05c2bd162e4b954ee4dbac7 \
  --query 'Where did I work on database performance?' --chunk-lines 20 \
  --document-prefix 'title: none | text: ' \
  --query-prefix 'task: search result | query: ' --timeout-seconds 30
```

The [Google prompt guide](https://ai.google.dev/gemma/docs/embeddinggemma/inference-embeddinggemma-with-sentence-transformers) supplies these asymmetric retrieval prefixes. Each request accepts 1–32 text inputs and returns normalized 768-dimensional float vectors. Token overflow returns an error instead of silently truncating. The harness stores a derived cache and returns original paths, source hashes and line anchors; read the source before making a claim. Select a shorter chunk if it exceeds 2,048 tokens.

The SDK choice matters: the released MediaPipe 1.1.0 UniversalEmbedder failed locally on this text-only bundle by trying to load an absent vision encoder. The released LiteRT-LM 0.18.0 EmbeddingEngine loaded the same bundle and produced real vectors, without larger multimodal weights or Torch.

## Typed decisions and inbox triage

`POST /v1/decisions` accepts selected text and 1–8 questions:

```json
{"model":"laya-s256","state":"Please choose an interview time this week.","questions":{"intent":{"type":"choice","instructions":"Choose the requested action.","criteria":{"meeting":"Choose a time for an interview.","test":"Complete a written assessment."}},"reply":{"type":"nullable_boolean","condition":"The message asks for a written reply.","lower":0.2,"upper":0.8},"priority":{"type":"score","instructions":"Rate time sensitivity.","rubric":["No action requested.","Action this week.","Action today."]}}}
```

Choice values preserve their keys. Boolean values use a configurable threshold. Nullable boolean values are `true`, `false` or `null`; the default abstention band is 0.2–0.8. Score values are the SDK's continuous expected score on the zero-based ordered rubric, with probabilities retained. These probabilities are model estimates; this implementation makes no local calibration claim.

Laya has a compact 256-token window. The adapter uses the bundle's official tokenizer with a conservative allowance for the question prompt and refuses oversized state/question combinations. Keep excerpts and descriptions short. At most 16 choice schemas are prewarmed in the Python adapter. It does not summarize a long email thread to fit that budget.

For the harness's selected-message triage:

```sh
job-harness context triage --input /path/to/selected-message.txt \
  --endpoint http://127.0.0.1:18081/v1 \
  --model laya-s256 --revision gcs-1791231584156797
```

The compatibility route `/v1/chat/completions` implements only this fixed typed triage contract. It rejects free-form generation, streaming, tools and enrichment requests. Mixed acknowledgments/rejections with recognized action requests stay `needs_reply=true`; disagreement or abstention stays uncertain. The rationale is a fixed review instruction, not generated evidence. The Rust result includes its independent rules baseline and never permits archiving. For summaries or application wording, use an external agent or a separately selected compatible generative model.

## Local verification and practical limits

Dependency-free synthetic checks:

```sh
PYTHONPATH=providers/local python3 -m unittest discover -s providers/local/tests -v
```

An explicit real-runtime smoke requires already installed SDKs and approved cached weights; it never installs or downloads anything:

```sh
providers/local/.venv/bin/python examples/models/smoke.py \
  --cache-dir /path/to/model/cache --output /path/to/synthetic-benchmark.json \
  --harness /path/to/job-harness
```

The [recorded native smoke](../examples/models/benchmark-2026-10-12.json) on macOS arm64, 18 logical CPUs, CPU backend and requested two embedding threads measured 1.53 s worker startup (excluding artifact hash verification), about 1.88 GiB combined worker peak RSS, 1.10 s for 32 fictional embedding passages, and 32–36 ms warm query requests. Four Laya primitives took 247 ms together. These are local observations, not portable performance promises. The first expected passage ranked first in one toy retrieval fixture; this is not a relevance evaluation.

The [Rust CLI integration](../examples/models/harness-integration-2026-10-12.json) also exercised real indexing, hybrid search with source hashes/line anchors and Laya triage in a temporary synthetic workspace: two passages indexed in 161 ms and search in 59 ms as measured inside the harness. No canonical evidence changed.

**Laya quality needs calibration on approved task examples.** This smoke included a wrong boolean and a high-confidence acknowledgment label on a message also requesting an interview reply. The gateway preserved the requested action and uncertainty. It proves the actual SDK and typed contract run, not that Laya can make unattended inbox decisions. No private facts, employer messages, microphone access, remote deployment or CI model download were used.

## Kev and Clef alternatives

The [Kev author’s SDK](https://github.com/jaredpalmer/kev/tree/2ecafc49fe1e8945cd600443e6c04c1cf0b3a78f) provides a separate Apple Silicon MLX path. `kev-0.8b` is selectable through the same `/v1/decisions` and triage gateway, using its own environment. It does not add Torch to the Gemma/Laya environment or Rust core.

| Profile | Complete selected download | Runtime and evidence |
|---|---:|---|
| [Kev-0.8B](https://huggingface.co/jaredpalmer/kev-0.8b) | 1,835,370,519 bytes / 1.71 GiB | LoRA/head **and** pinned Qwen base; author MLX bf16 path verified locally |
| [Kev-4B](https://huggingface.co/jaredpalmer/kev-4b) | 9,502,461,619 bytes / 8.85 GiB | Metadata only; no download or inference lifecycle here |
| [Clef-flash 9B](https://huggingface.co/Cloudflare/clef-flash) | 19,083,367,679 bytes / 17.77 GiB | Metadata only; public full weights and joint decision head, no local trial or hosted call |

Both Kev adapters and their Qwen bases declare Apache-2.0; Clef-flash’s publisher repository includes its Apache-2.0 license. The complete immutable file plans are in [model-profiles.json](../providers/local/model-profiles.json). [summary.json](../providers/local/summary.json) is a compact generated projection for the native CLI/dashboard. Published memory figures are workload-specific: Kev-4B’s card reports **11.9 GB process footprint** on an M5/32 GB Mac for three questions and an 8,192-token state, in bf16. That is the condition behind the approximate “12 GB” claim, not a universal RAM requirement. It was not tested here.

The [independent Apple Silicon benchmark](https://github.com/tak-bro/local-jev-bench/tree/f84f5b90d69a2e59a1afa9bd8b0952b593641274) reports Kev-0.8B short-set first-call medians of 27.9–57.6 ms; its long typed-decisions set takes 152.4 ms. The abbreviated “28–58 ms” excludes that set. Its Laya path uses Ollaya, so those numbers do not measure this MediaPipe adapter. Small handwritten sets, training overlap, option-order changes and contention limit the comparisons.

For the selected Kev profile, install a **separate** tested environment on Apple Silicon macOS / Python 3.12:

```sh
uv venv --python 3.12 providers/local/.venv-kev
uv pip install --python providers/local/.venv-kev/bin/python --no-deps \
  -r providers/local/requirements-runtime-kev.txt
uv pip install --python providers/local/.venv-kev/bin/python --no-deps -e providers/local

providers/local/.venv-kev/bin/job-harness-local info kev-0.8b
providers/local/.venv-kev/bin/job-harness-local download kev-0.8b --approve TOKEN_FROM_INFO
providers/local/.venv-kev/bin/job-harness-local serve \
  --decision-model kev-0.8b --port 18082 --threads 2 --timeout 30

job-harness context triage --input /path/to/selected-message.txt \
  --endpoint http://127.0.0.1:18082/v1 \
  --model kev-0.8b --revision bf75a6a8848ea6960ff2ed108d9ed44c2941174f
```

The pinned SDK is installed without its training extras. It reuses the author’s tokenizer, encoder, LoRA merger, MLX backbone and pointer head. The head is loaded explicitly with `torch.load(weights_only=True)`; base and adapter revisions/hashes must match. HF/Transformers offline flags are set inside the fresh-env worker. The local state budget is strict at 8,192 tokens; no context is silently truncated. Torch’s head receives the selected CPU thread count, while the backbone runs on Metal. Review/abstention thresholds are conservative host rules and are uncalibrated for this workload; the SDKs define confidence differently. Quantized Kev inference is unverified.

The [frozen six-case fixture](../examples/models/triage-fixture.json) and [Kev result](../examples/models/kev-0.8b-2026-10-12.json) record original/reversed option order, raw probability outputs, separate guarded triage, and actual Rust CLI results. Warm five-question HTTP requests had a 26.24 ms median, with roughly 3.01 GiB process peak RSS (`ru_maxrss`, with Metal accounting potentially different). MLX’s separate allocation peak is reported for the worker lifetime. A first direct SDK load took 8.69 s and its first two-question inference took 2.62 s before kernel warmup; the later gateway startup was 2.79 s with existing caches. These are different startup conditions, not interchangeable “cold” timings. Raw reply decisions often abstained. [Laya on the same fixture](../examples/models/laya-2026-10-12.json) used CPU and had a 533.88 ms warm median for five primitive calls; Kev used Metal with a shared state pass. This compares the tested adapter paths, not general model quality. Both had no observed label-order flip in twelve rows; that small result proves little about unseen messages.

Reproduce against an already running selected provider; this script installs/downloads nothing:

```sh
python3 examples/models/typed_smoke.py \
  --endpoint http://127.0.0.1:18082/v1 --model kev-0.8b \
  --revision bf75a6a8848ea6960ff2ed108d9ed44c2941174f \
  --harness /path/to/job-harness --output /path/to/fictional-typed-probe.json
```

The requested Claude Opus 5.5 / high discussion supported a bounded 0.8B feasibility trial and keeping every model optional. It did not justify a default or a 4B/9B download. Larger models need a workload-specific reason and fresh metadata review. Kev-0.8B’s own card cautions against tool-call routing and consequential unattended employment decisions; the gateway only proposes typed results for review. Neither these fixtures nor a public benchmark establish hiring-triage quality or transferable calibration.
