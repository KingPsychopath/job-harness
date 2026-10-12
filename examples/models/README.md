# Synthetic native model probes

These fictional messages and project excerpts contain no candidate facts, contacts or private ledger data. Probes call an explicitly selected, already-running native provider; they do not install or download anything and cannot archive, send, apply or change canonical records.

```sh
job-harness-local probe --model embeddinggemma2-text \
  --endpoint http://127.0.0.1:18081/v1 --output gemma-report.json \
  --harness /absolute/path/to/job-harness
job-harness-local probe --model laya-s256 \
  --endpoint http://127.0.0.1:18081/v1 --output laya-report.json \
  --harness /absolute/path/to/job-harness
job-harness-local probe --model kev-0.8b \
  --endpoint http://127.0.0.1:18082/v1 --output kev-report.json \
  --harness /absolute/path/to/job-harness
job-harness-local encoding-check --cache /absolute/model/cache --output encoding-report.json
```

The embedding probe checks32×768-dimensional vectors, five warm queries, one synthetic retrieval ranking and real Rust CLI index/search source anchors. Typed probes use the frozen [triage-fixture.json](triage-fixture.json): six cases, original and reversed choice order, two repeated requests, all four typed primitives, raw probabilities, independent consistency rules and real Rust CLI triage. `encoding-check` compares token IDs and pointer positions with [kev-reference-token-ids.json](kev-reference-token-ids.json), recorded from the pinned author SDK on the same fictional inputs.

`native-*-2026-10-12.json` reports are the current native measurements. Older files without `native-` document development runs of the original upstream SDK wrappers. Their Python/MLX startup and latency values are **not native-provider measurements**. The old Laya prototype sorted schema-cache keys, so its reversed requests reused original choice order; its report is annotated. The native provider respects caller order and shows two urgency flips, alongside wrong labels and abstentions. No report establishes general accuracy, calibration or a model ranking.

The runnable catalog contains only EmbeddingGemma2 text, Laya s256 and Kev 0.8B. Larger candidates inspected as metadata during development were not downloaded or tested and are no longer setup options. Historical SDK reports are retained as evaluation evidence; they do not add installable profiles.

See [docs/MODELS.md](../../docs/MODELS.md) for actual native limitations and exact upstream sources. No weights, SDKs or generated kernels are tracked here.
