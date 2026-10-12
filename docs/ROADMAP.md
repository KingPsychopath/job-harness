# Roadmap

The private preview includes a native Rust engine, CLI/MCP, ACP client, optional bounded compatible model host, local dashboard with optional named review routes, full/profile transfers, derived SQLite search, source-preserving intake, selected document exports, actual optional local model adapters and evidence-linked Typst CV generation.

The next product boundary is a richer GUI over the same contracts. A desktop host can provide secure credentials and connector bindings; no provider SDK needs to enter the core. Browser/mail/calendar drivers should return observations with provider identities, leaving submission coordination in one engine.

Multi-host operation needs a deliberate transactional coordinator/storage version and per-user authorization. Flexible Markdown evidence can remain alongside relational operational records. Do not assume local file locks synchronize machines or use the search index to authorize actions.

Public Cargo/npm/Homebrew distribution, auto-update, remote hosting and repository visibility changes remain explicit owner decisions. Benchmarks and actual completed workflows should determine future optimization; event counts are not model-round-trip metrics.
