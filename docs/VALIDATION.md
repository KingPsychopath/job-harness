# Preview validation

The reusable software and private career workspace remain separate. The preview release is held; a Git tag and successful build artifacts are not a published release.

## Confirmed evidence

| Check | Result |
|---|---|
| v0.2.0 source, `95a732a` | [Six Windows/Linux/macOS CI checks passed](https://github.com/KingPsychopath/job-harness/actions/runs/38206520740) |
| Native release controller, `fd0618c` | [Six CI checks passed](https://github.com/KingPsychopath/job-harness/actions/runs/38211472120) |
| Recovery release | [Both Linux builds and macOS ARM passed](https://github.com/KingPsychopath/job-harness/actions/runs/38211482260); publication canceled after additional stress evidence |
| Uploaded macOS ARM components | Independent checksum, source/tag identity, copied version and archive-content checks passed |
| Final surface and Notes layout | Browser rendering checked in light and dark; final native visual acceptance remains separate |
| Protocol stress on macOS | Twenty complete runs passed with an external process deadline |

The first Linux x86 release attempt exhausted runner disk. The controller now removes unused hosted SDKs and restored target-triple/incremental artifacts, verifies the native host architecture and reuses native output paths. The corrected Linux x86 job passed. The existing v0.2.0 source tag was preserved.

## Windows notification investigation

One first-release Windows run stalled in the board-move test for roughly 40 minutes before cancellation. Ordinary Windows CI and the recovery release's core tests passed, so those successes alone do not establish freedom from intermittent stalls.

A [bounded diagnostic](https://github.com/KingPsychopath/job-harness/actions/runs/38215490577) completed eighteen iterations, then exceeded its 20-second deadline on iteration nineteen. All board phases and the other eighteen protocol tests completed; the unfinished test was `notifications_require_tab_nonce_and_stream_only_current_hints`.

The diagnostic now records phase-only checkpoints around watcher construction, stream updates, close and teardown, and captures child process IDs, names and creation times before termination. It never dumps environment variables, command arguments or process memory. No guard, assertion or native notification behavior has been disabled.

The [next diagnostic](https://github.com/KingPsychopath/job-harness/actions/runs/38217165883) was rejected before any runner or step started. GitHub Actions execution requires account-owner action before further Windows validation. The investigation is tracked in [issue 1](https://github.com/KingPsychopath/job-harness/issues/1); account details stay outside this reusable repository.

## Review decisions

Restricted Claude Opus 5.5 high review and independent agents examined the generic code. A proposed blanket reentrant-lock guard was rejected because no nested acquisition was demonstrated and it would break legitimate shared readers; the reviewer withdrew it. A later claim that the release workflow changed `TMP`/`TEMP` was unsupported by the actual workflow and was rejected. Unbounded optional Git probes are a separate candidate, not an established explanation for the notification stall.

The next step is to execute the instrumented Windows diagnostic, identify the stalled phase, correct the demonstrated cause and repeat the bounded test. Production changes require fresh platform checks before a new source tag and private release. Native Windows/Linux GUI acceptance, signing and live local inference remain distinct from compilation and fixture evidence.
