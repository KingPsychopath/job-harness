# See the workflow

Every example uses fictional data. None contacts an employer, mailbox or calendar.

## First useful result

```sh
jh demo --workspace "$HOME/Career/demo"
jh --root "$HOME/Career/demo" workspace search database
jh --root "$HOME/Career/demo" notes list
jh --root "$HOME/Career/demo" gui
```

The native demo creates four vacancy records, one project story and one linked original interview note. Search retrieves the project containing a database query investigation. The note retains the missing technical-round date as unknown. An existing nonempty destination is refused; your default workspace is unchanged.

The [README screenshot](../docs/assets/dashboard.png) shows fictional applications. The optional [desktop application](../apps/desktop/README.md) opens the same fixed routes in a native window.

## Let an agent pick up the work

```sh
jh --root "$HOME/Career/demo" connect mcp
```

Give the returned server configuration to your supporting client and ask it to bootstrap, read the relevant skill and source, then prepare an interview story from `context/projects/dashboard.md`. CLI-capable agents can use the same `call` commands. [MCP/ACP examples](agent/README.md)

## Batch and resume

`./scripts/demo.sh` is a disposable shell walkthrough: initialize, bootstrap, query two sources in a batch, chain a run's result into its completion, index, export and restore. It cleans its temporary directory. `make demo` uses the checkout's native binary. [Full batching/recovery contract](../resources/guides/job-harness-contract.md)

## Add depth only when needed

| Task | Example |
|---|---|
| Preserve rough notes and review cited additions | [Notes and intake](../docs/INTAKE.md) |
| Retrieve related passages or review typed model suggestions | [Pinned local model examples and probes](models/README.md) |
| Create a selected one-page CV | [CV selection](cv/selection.json), [PDF](cv/example.pdf), [guide](cv/README.md) |
| Export a cover letter, interview story or period review | [Selected documents](../docs/DOCUMENTS.md) |
| Inspect unfinished work | `jh --root PATH status` and `jh --root PATH call review_queue` |

Model examples state which runtime actually ran, its sources and measured limits. No real model is needed for the normal demo, and no model output is a qualification.
