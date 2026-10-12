# Architecture review and evidence

On 12 October 2026, two restricted, tools-disabled Claude Opus 5.5 high reviews assessed the generic architecture and transfer/packaging implementation. Review input contained generic code and selected historical workflow lessons. No candidate facts, credentials or raw production data were supplied. The reviewer inspected pasted source; it did not execute tests or certify the system.

The review supported separating durable workspace state, execution lifetime and security isolation. Persistent VMs can serve appropriate workloads; ephemeral isolation helps with untrusted code. Existing authenticated connectors suit this preview. A future compute service must still provide identity, permissions, limits, artifact transfer and verified outcomes. The model/runtime is not the source of career facts.

Concrete fixes included refusing bundle destinations inside the workspace, retaining configured CV/answer files, validating profile shape, preventing full-import onboarding drift, safe MCP module search, common credential-filename filtering and canonical wheel resources. Redundant source export was removed. Follow-up fixes use filesystem identity for alternate-case paths, refuse final-component symlink swaps and keep distribution checks active under Python optimization. Common filename filters are not a content secret scanner.

Local verification includes 72 behavioral tests, independent focused/full MCP SDK/schema checks, source/skill/link/SVG checks, exact wheel/sdist resource parity and a fresh wheel-only CLI/MCP/bundle round trip outside the checkout. Original vector graphics were rendered and visually inspected. The existing private workspace was compared separately by hashes; application data was not used as a CI fixture.

CI runs the same checks on Linux/Python 3.11 and macOS/Python 3.14. The exact commit's [CI run](https://github.com/KingPsychopath/job-harness/actions/workflows/ci.yml) is the source for remote evidence. Live job-provider actions, a GUI, public registries, shared cloud coordination and Windows locking are outside this verification.

The retained workflow lessons are ownership of mutable state, bounded batches, focused context retrieval, client-neutral AGENTS.md/skills and constrained tools. Historical cost observations were not remeasured; ledger event counts must not be used as model-round-trip metrics.

The first remote matrix passed on Linux but exposed an exact-string interpreter-path assumption in the macOS wheel check. Launch metadata now preserves the active virtual environment, and verification checks the actual environment and installed module location rather than path spelling. The fresh-wheel check also passed under a separate macOS framework Python locally. Check the latest head for final remote results.
