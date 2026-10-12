# Rehearse a daily review locally

These two messages are fictional: one resolved application confirmation and one recruiter conversation needing a reply, with no known application. Dates, labels and provider IDs are fixtures. They are not a fresh scan or live mail connection.

Use the current source-built core; the unchanged v0.2.0 preview predates the daily tools. From the toolkit checkout, choose a **new folder outside the repo**. On macOS/Linux:

```sh
jh --root "$HOME/Career/daily-rehearsal" init
cp examples/daily-review/observations.json "$HOME/Career/daily-rehearsal/applications/evidence/daily-demo.json"
jh --root "$HOME/Career/daily-rehearsal" daily check --observations applications/evidence/daily-demo.json
jh --root "$HOME/Career/daily-rehearsal" call execute_batch --input examples/daily-review/record-review.json
jh --root "$HOME/Career/daily-rehearsal" daily check
```

In PowerShell, use `(Join-Path $HOME "Career/daily-rehearsal")` for `--root`, and copy the fixture with:

```powershell
Copy-Item examples/daily-review/observations.json (Join-Path $HOME "Career/daily-rehearsal/applications/evidence/daily-demo.json")
```

The first check is read-only and marks its input as supplied observations. The batch starts a local review run, checks the snapshot, records `daily_update` with `action=record_review`, and finishes the run. Exact `$ref` values carry the run ID and observed source hash between steps. The last check reads saved evidence and reports that no live mailbox read occurred.

Expect two recruiting communications, one reply request and one archive candidate. No application is created and nothing is archived, sent or booked. A candidate is only a proposal: this rehearsal deliberately does not provide human approval or a one-shot host action. The fixed historical timestamp would also fail the fresh-read requirement for beginning an archive today.

Rerunning the unchanged batch recognizes its saved action keys. Do not reuse its keys for changed input. For real work, read the [daily guide](../../resources/guides/daily-review.md), inspect current schemas and use your host's actual observations and human-reviewed selection. `begin_mail_archive` is a separate call and is forbidden in a batch.
