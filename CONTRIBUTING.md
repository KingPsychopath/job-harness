# Contributing to Job Harness

Help an agent complete useful career work with less friction and better evidence. Prefer a small, dependable capability over a larger set of features. Read [AGENTS.md](AGENTS.md), [architecture](docs/ARCHITECTURE.md) and [the tool contract](resources/guides/job-harness-contract.md) before changing behavior.

## Start here

1. Install the pinned Rust toolchain and your platform C build tools. No model or account access is needed for the normal test suite.
2. Run `cargo test --workspace --locked`, then `cargo run -p job-harness-dev -- repository`.
3. Use `jh demo --workspace /new/temporary/path` or a test fixture. Never use a real candidate, inbox or credential as development data.
4. Reproduce one actual workflow problem. State the desired outcome, evidence and interruption behavior before editing.

Use [development commands](docs/DEVELOPMENT.md) for lint, generated contracts, native package checks and optional components. Desktop/model checks are separate from the slim core build.

## Choose the right layer

| Change | Home |
|---|---|
| Shared states, identities or canonical encoding | `crates/harness-domain/` |
| Durable workspace capability, schema, provenance or recovery | `crates/harness-core/` + `resources/schemas/` |
| MCP, ACP, model endpoint or dashboard transport | `crates/harness-protocol/` |
| Command UX and host workspace selection | `crates/harness-cli/` |
| Guidance for agents | One focused skill under `resources/skills/` |
| Optional desktop or native model runtime | `apps/desktop/` or `providers/local/` |
| Development gates and release packaging | `xtask/` + `.github/workflows/` |

A transport calls the same engine; it does not create another tracker or rewrite submission rules. Avoid arbitrary SQL, shell execution, expression languages or provider secrets in the engine. Source/config selection, career evidence, operational state, derived indexes and host access have different owners.

## Add a capability well

Define strict input/output schemas and stable error codes. Keep output bounded and paginated where needed. Explain `next` as guidance, not authorization. Use existing batch references instead of asking a model to shuttle identifiers manually.

For writes, test repeated keys, changed sources, partial failure and interrupted work. A begun or unknown attempt must never become another submit permit. Preserve opaque historical fields, artifact bytes and unresolved state in imports. New storage or contract versions need explicit migration and compatibility fixtures.

Keep original sources distinct from summaries, coaching and reviewed additions. A cited model result is still a suggestion. Add the capability to discovery and its focused skill so an agent can find and use it without loading the entire workspace.

## Make the change reviewable

Run the checks affected by the change, then the normal gate before proposing a release. Regenerate TypeScript contracts with `cargo run -p job-harness-dev -- contracts`; do not hand-edit generated schemas/types. For embedded resources or packaging, verify a copied binary outside the checkout.

Use clear focused commits. A pull request should describe the concrete problem, resulting behavior, relevant tests and known limits. Separate local checks, exact-head CI and actual provider execution. A passing synthetic fixture is not proof an employer form or production service works.

No real candidate data, private bundles, model weights or compiled runtime libraries belong in Git. The repository guard enforces this boundary. Native components acquire their own license/digest metadata before opt-in installation.

## Release and access

The owner controls repository visibility and publication. Private native releases contain software and checksums only; private data exports are separate. Public registries, paid deployments and credential/config writes require explicit scope. Follow [security guidance](SECURITY.md); report issues using synthetic reproductions without secrets or career data.
