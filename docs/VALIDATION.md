# Preview validation

The [v0.2.0 private preview](https://github.com/KingPsychopath/job-harness/releases/tag/v0.2.0) contains core, optional desktop and optional local-provider archives for five targets. Every archive is bound to clean source `95a732a4e899cb2e12fa5d9c50be3d00bf3dfe37`. The reusable software and private career workspace remain separate.

**Windows is experimental:** its release executables were cross-built and inspected, not executed on Windows. Earlier native source tests passed, but a later stress test exposed an unresolved intermittent notification stall. Shared core code does not remove that platform-specific limit. See [issue 1](https://github.com/KingPsychopath/job-harness/issues/1).

## Confirmed evidence

| Check | Result |
|---|---|
| v0.2.0 source, `95a732a` | [Six Windows/Linux/macOS CI checks passed](https://github.com/KingPsychopath/job-harness/actions/runs/38206520740) |
| Native release controller, `fd0618c` | [Six CI checks passed](https://github.com/KingPsychopath/job-harness/actions/runs/38211472120) |
| Recovery release | [Both Linux builds and macOS ARM passed](https://github.com/KingPsychopath/job-harness/actions/runs/38211482260); publication canceled after additional stress evidence |
| Twelve macOS/Linux release archives | Artifact digests, archive checksums, clean source/tag identity, executable headers and software-only contents checked |
| macOS ARM release components | Copied core, provider and desktop version checks passed outside the checkout |
| Final native macOS interface | Light/dark surfaces, Notes layout, card details and minimize/reopen checked in an installed app against fictional data |
| Protocol stress on macOS | Twenty complete protocol runs passed; a further 512 synthetic workspaces exercised 1,536 canceled streams and 512 native change/teardown cycles, all under external deadlines |
| Windows x64 release executables | All three components and the alias cross-built with Rust 1.97.1, cargo-xwin 0.23.1, LLVM/LLD 23.1.3 and Microsoft SDK 10.0.26100; PE32+/AMD64 headers and hashes checked; **not executed** |
| Final local development gates | Core, optional models, desktop, schemas/resources and copied-package checks passed on macOS; the SDK-dependent model comparison remains explicitly ignored without its selected runtime |

The first Linux x86 release attempt exhausted runner disk. The controller now removes unused hosted SDKs and restored target-triple/incremental artifacts, verifies the native host architecture and reuses native output paths. The corrected Linux x86 job passed. The existing v0.2.0 source tag was preserved.

## Windows notification investigation

One first-release Windows run stalled in the board-move test for roughly 40 minutes before cancellation. Ordinary Windows CI and the recovery release's core tests passed, so those successes alone do not establish freedom from intermittent stalls.

A [bounded diagnostic](https://github.com/KingPsychopath/job-harness/actions/runs/38215490577) completed eighteen iterations, then exceeded its 20-second deadline on iteration nineteen. All board phases and the other eighteen protocol tests completed; the unfinished test was `notifications_require_tab_nonce_and_stream_only_current_hints`.

The diagnostic now records phase-only checkpoints around watcher construction, stream updates, close and teardown, and captures child process IDs, names and creation times before termination. It never dumps environment variables, command arguments or process memory. No guard, assertion or native notification behavior has been disabled.

The [next diagnostic](https://github.com/KingPsychopath/job-harness/actions/runs/38217165883) was rejected before any runner or step started. No further Actions runs were requested for this preview. Local macOS stress did not reproduce the failure and is not Windows validation. The investigation remains open in [issue 1](https://github.com/KingPsychopath/job-harness/issues/1).

## Review decisions

Restricted Claude Opus 5.5 high review and independent agents examined the generic code. A proposed blanket reentrant-lock guard was rejected because no nested acquisition was demonstrated and it would break legitimate shared readers; the reviewer withdrew it. A later claim that the release workflow changed `TMP`/`TEMP` was unsupported by the actual workflow and was rejected. Unbounded optional Git probes are a separate candidate, not an established explanation for the notification stall.

The next Windows step is to execute the instrumented diagnostic, identify the stalled phase, correct the demonstrated cause and repeat the bounded test. Production changes require fresh platform checks before a new source tag. Native Windows/Linux GUI acceptance and platform signing remain unverified; local inference measurements are recorded separately in [Models](MODELS.md).

Release assets include `SHA256SUMS` and `PLATFORM_VALIDATION.json`, which distinguish native builds, cross-builds and actual execution. None contains career data, credentials, model weights or proprietary runtime downloads. Current documentation and screenshots can advance independently of the immutable v0.2.0 source tag.

## Source changes after the published preview

Current source uses clearer email/evidence wording and an agent-first setup path.
It also fixes explicit selection of an imported PDF as the primary CV, without
widening general source access or relaxing submission recovery. Local core,
desktop and copied-package checks passed on macOS, along with an independent
18-check setup/import/backup journey. New source was not tested on Windows or
Linux. Published v0.2.0 archives retain their original bytes; follow the
[older-preview CV-path step](INSTALL.md#v020-cv-path-compatibility) when using them.

CI and releases are now manual-only to avoid automatic Actions spending. Workflow
syntax, option combinations and release preflight were checked locally; they have
not been executed on GitHub. Default manual CI uses one Linux core job; platform,
desktop, provider and package work are opt-in. [Verification profiles](DEVELOPMENT.md#choose-a-manual-ci-profile)
describe the available checks. Existing regression tests remain in the local suite.

## Daily review and recruiting communications

Current source adds read-only daily checks, retained correspondence and reviewed
mail cleanup. This is separate from the immutable v0.2.0 packages.

| Local check on macOS | Evidence |
|---|---|
| Engine, CLI, domain and protocol | 199 tests, formatting, Clippy, repository and generated-contract checks passed |
| Optional desktop | Five native boundary tests and the native release build passed |
| Copied CLI outside the checkout | Official MCP client checked the focused 12-tool and full 43-tool profiles, embedded skills, source handling and exact private transfer |
| Daily command rehearsal | Ten synthetic check/update/search/brief steps passed; no archive approval or provider access |
| Release-binary mail lifecycle rehearsal | Nine synthetic steps covered withdrawal, the separate one-shot boundary, replay, unknown recovery and hashed readback with disclosed label drift |
| Live Daily review screen | Not verified: the local browser client refused the isolated loopback preview before loading it; no screenshot or click-through result is claimed |

Restricted, tool-free Claude Opus 5.5 high review identified out-of-order backfill,
queued-withdrawal and label-drift recovery gaps. Regression checks now cover them.
Approval attribution remains a trusted-host attestation; a GUI-origin string would
not authenticate a human and would exclude legitimate CLI/MCP hosts. Partial or
missing archive targets stay unknown rather than gaining another attempt.

No real mailbox, employer or calendar was used for these tests. No GitHub Actions
run was dispatched, and no Windows/Linux native execution is claimed for these
source changes. The host still owns live mail access and modification authority.
