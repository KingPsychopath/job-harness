# Optional model examples

All texts and records here are fictional. No candidate workspace, CV or employer correspondence was used.

[Set up and review model metadata first](../../docs/MODELS.md). `smoke.py` starts the real local provider with already installed SDKs and already approved cached weights, exercises retrieval and typed decisions, rejects generation and stops the provider. It never downloads or installs models.

The dated JSON files preserve local measurements and wrong model predictions as well as successful contracts. The native provider benchmark tests embeddings and typed primitives; the Rust integration separately proves indexing/search source provenance and mixed-request visibility. Neither is a general quality or performance claim.

`triage-fixture.json` freezes six fictional cases and question wording. `typed_smoke.py` sends them through an explicitly selected running Laya/Kev service, records original/reversed options and raw outputs separately from guarded triage, and optionally invokes a supplied Rust binary. The per-model dated results are feasibility evidence, not a model ranking.
