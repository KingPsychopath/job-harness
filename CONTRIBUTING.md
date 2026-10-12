# Contributing to Job Harness

Contributions are welcome when they solve a reproducible problem or improve an agreed workflow. Start with the problem, then send a focused change with evidence a reviewer can check.

## Start with an issue

Search existing issues and pull requests first. Add a new reproduction or useful evidence to an existing thread when it covers the same problem.

For a bug, use the bug report form: explain the expected outcome, what happened, your version/platform and the smallest fictional reproduction. A focused fix may accompany that issue; it does not need a separate design approval.

Discuss substantial changes before implementing them: new capabilities, connectors or dependencies; changed schemas, storage or recovery rules; and broad interface redesigns. Use the change proposal form to explain the workflow or maintenance problem, current workaround and proposed scope. Wait for a maintainer to confirm the scope in the issue. Comment before beginning substantial work so others can see who is working on it.

Small typos, broken links, clear documentation corrections and narrow, well-understood fixes with a self-contained reproduction can go straight to a PR. An open proposal or an unanswered issue does not imply accepted scope.

Report sensitive security problems through the private route described in [SECURITY.md](SECURITY.md). Do not put secrets or candidate data in an issue.

## Send one reviewable PR

- Link the issue, or explain the small direct correction and include a reproduction for a code fix. State the concrete problem and resulting behavior.
- Keep the diff to one coherent change. Include relevant tests, documentation and generated contracts; leave unrelated cleanup for an agreed follow-up.
- Run the checks affected by the change and report the commands, results and platform. State what you could not verify; include CI only if it actually ran. Do not call a cross-build a native test or a mock provider a live integration.
- For visible UI changes, attach before/after screenshots using fictional data; new UI only needs the resulting screen. For an interaction such as dragging, selecting, loading or reopening a window, add a short video/GIF when practical, showing one interaction in about 30 seconds or less. Include written steps and the tested OS. One representative OS is enough for shared visual changes; identify platform-specific behavior and untested platforms.

Images and recordings must show the actual behavior under review. Label mocks and placeholders. Remove personal records, account identifiers, user filesystem paths, session tokens and credentials; use the fictional demo or a synthetic fixture. Keep text reproduction steps and error codes readable without opening an attachment.

AI assistance is allowed. Human and agent contributions have the same standard: review the entire diff, check behavior, verify claims and explain the change. Agent submissions must be authorized by and answerable through a responsible person or organization. Maintainers may ask about any part of the diff or its evidence and may close submissions that cannot be supported. Review model output before submitting it; report actual check results, not generated claims of verification.

GitHub verification is manual-only. No workflow checks run automatically on PRs.
Report relevant local results; a maintainer can request a focused remote check
when it resolves a platform gap. See [verification profiles](docs/DEVELOPMENT.md#choose-a-manual-ci-profile).

Submit only material you have the right to contribute. Contributions use the project's [MIT license](LICENSE); preserve required upstream copyright and license notices for reused material.

## No spam or unsolicited sweeps

We close duplicate, empty, promotional or mass-generated submissions that add no concrete problem, reproduction or agreed scope. Do not multiply identical reports or send unsolicited formatting sweeps, speculative rewrites, dependency churn or bulk "cleanup" changes. Split substantial work across PRs only as planned in the agreed issue. Keep comments useful: avoid repeated bumps and unsolicited generated reviews.

Focused fixes or bumps to an existing dependency for a published advisory are welcome with a concrete reason and relevant checks. A bump that requires substantial API or behavior changes needs a change proposal, as does adding or replacing a dependency. Undisclosed vulnerabilities follow [SECURITY.md](SECURITY.md); planned maintenance uses a scoped change proposal.

Invented evidence, copied private data and unreviewed patches are grounds to close a submission. Disabling a failing guard or test to obtain a green result is not a fix. Explain failures and limitations plainly.

## Preserve the project boundaries

- Candidate facts, CVs, application records, inbox extracts and private bundles stay outside this repository. Tests and screenshots use fictional sources. Credentials remain in the host; model weights and vendor runtime libraries are separate opt-in downloads.
- CLI, MCP and GUI call the same `Engine::call`. Do not introduce another tracker authority or arbitrary SQL, shell execution or expression evaluation into the engine.
- Preserve original sources and exact artifact bytes, including historical fields an importer does not recognize. Summaries, coaching and model proposals stay distinct from approved facts.
- Keep versioned schemas, bounded output, exact result references and idempotency consistent. A tool listing, run or suggested next step never grants permission to contact an employer or perform another external action. See [tool contracts](resources/guides/job-harness-contract.md).
- Preserve [journal recovery and submission rules](docs/ARCHITECTURE.md#durable-local-action-uncertain-external-action). A begun or unknown submission stays blocked until its outcome is checked. A search-cache failure cannot authorize retrying an action already committed. Storage changes need an explicit migration and compatibility fixtures.

Read [AGENTS.md](AGENTS.md), [architecture](docs/ARCHITECTURE.md) and [the tool contract](resources/guides/job-harness-contract.md) before changing behavior. The owner controls visibility, publication, paid services and host access; contributing does not authorize those actions.

## Develop and check

Install the pinned Rust toolchain and your platform C build tools. The normal suite needs no model, account or real candidate workspace. Use `jh demo --workspace PATH` with a new temporary path, or a synthetic test fixture.

| Change | Home |
|---|---|
| States, identities and canonical encoding | `crates/harness-domain/` |
| Durable capabilities, schemas, provenance and recovery | `crates/harness-core/` + `resources/schemas/` |
| MCP, ACP, model endpoint and dashboard transport | `crates/harness-protocol/` |
| CLI and host workspace selection | `crates/harness-cli/` |
| Focused agent guidance | `resources/skills/` |
| Optional desktop or native provider | `apps/desktop/` or `providers/local/` |
| Development gates and release packaging | `xtask/` + `.github/workflows/` |

Use [development commands](docs/DEVELOPMENT.md) for the relevant checks. For behavior changes, run `make check` or its portable Cargo commands. For packaging or embedded resources, run the package check against a copied binary outside the checkout. Desktop and model components have separate manifests and gates.

Regenerate TypeScript contracts with `cargo run -p job-harness-dev --locked -- contracts`; do not hand-edit generated types. Test meaningful source changes, repeated keys, partial failures and interrupted work when changing durable operations. Keep new capabilities discoverable through their schema and focused skill.

For documentation-only PRs, review the rendered text. If links or assets change, run `cargo run -p job-harness-dev --locked -- repository`. State what you checked; an unrelated runtime test suite is unnecessary for a typo correction.
