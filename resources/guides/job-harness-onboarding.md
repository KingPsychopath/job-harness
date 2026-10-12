# Start with your own career workspace

Job Harness keeps supplied career information and application history in one private folder. People and agents use the same tools to retrieve sources, reuse confirmed answers, check duplicates and organize applications. Your agent host supplies live browser, mail and calendar access; the toolkit does not connect those accounts by itself.

## Choose a starting point

| You have… | Start with… |
|---|---|
| An existing agent | The [native core release](../../docs/INSTALL.md), then MCP or direct CLI |
| A preference for a window | The separate desktop app, opening an initialized workspace |
| The toolkit repo open in an agent | [Build the core and use a separate private folder](../../docs/AGENTS.md#open-the-checkout-in-your-agent) |

No built-in model, questionnaire or CV generator is required. Optional local models, Typst and desktop installation can come later.

## Set up a separate private folder

On macOS/Linux, after installing the core:

```sh
jh setup --workspace "$HOME/Career/workspace" --json
jh --root "$HOME/Career/workspace" status
jh --root "$HOME/Career/workspace" connect mcp
```

In PowerShell, use `(Join-Path $HOME "Career/workspace")` for the workspace argument. Setup creates a new/empty folder or selects an existing valid workspace without resetting it. It saves a host-local default location. `--plan --json` previews setup without changing files or settings. Optional `--capability` choices describe requirements; they do not download models or grant access.

The toolkit checkout is reusable software. Keep private data outside it. One person's agents use the same private workspace; copies on different machines have independent locks and do not synchronize automatically.

For a fictional trial, run `jh demo --workspace PATH` with a new folder, then `jh --root PATH gui`. The demo is clearly fictional and does not change your saved workspace selection. Open the desktop app or the local URL printed by `gui`; `gui/index.html` in the code checkout is not a standalone application.

## Add your first sources

With an existing approved CV, retain it first:

```sh
jh --root "$HOME/Career/workspace" source import --input /path/to/approved-cv.pdf
```

The result's `data.path` is the retained workspace-relative path and `data.sha256` identifies its exact bytes. Import preserves the original and a record of where it came from; it does not approve claims, extract facts or change the primary CV. After reviewing the PDF, set `primary_cv` in the private workspace's `config/career-ledger.json` to that returned path. Current source supports an imported PDF here; the unchanged v0.2.0 preview requires the [CV-path compatibility step](../../docs/INSTALL.md#v020-cv-path-compatibility).

Set `timezone` to your actual IANA timezone, such as `Europe/London` if appropriate. Paths in this configuration are relative to the **private workspace**, not the toolkit repository. Inspect the supported shape with `jh config-schema ledger`; keep credentials and model endpoints in the host instead.

Without a CV, start with a supplied note:

```sh
jh --root "$HOME/Career/workspace" notes import \
  --input /path/to/interview-notes.md --kind personal_notes --key first-interview-note
```

The key identifies this operation so a repeated request can be recognized. Use a different key for different supplied content. Original notes remain intact. Dates, ownership and outcomes that you have not supplied stay unknown. An agent can propose interpretations with source references; reviewing and accepting selected items is a separate step. Notes do not automatically rewrite your CV or mark an application submitted.

Add confirmed answers and preferences to the configured answer records, recording who supplied them, when, where they apply and any limits. New workspaces include blank `applications/approved-answers.md` and `context/profile/application-decisions.md`. Salary, contact details and consent are personal records, not reusable toolkit defaults. Your agent should read existing answers before asking the same question again.

## Give an agent its first task

Connect the command and arguments returned by `connect mcp` to your chosen host, or let a command-running agent use the CLI with an explicit `--root`. [Connection instructions](../../docs/AGENTS.md)

```text
Use Job Harness with /absolute/private/career-workspace. Bootstrap, read the
focused skill and inspect unfinished work. Help organize the CV, notes and answers
I supply. Keep originals, preserve unknowns and propose interpretations with
source references. Ask only for missing facts. Do not change my approved CV or
send messages or applications beyond my request.
```

Through MCP, start with `bootstrap` for the requested operation. Through the CLI, on macOS/Linux:

```sh
jh --root "$HOME/Career/workspace" call bootstrap <<'JSON'
{"operation":"reconcile","capabilities":[]}
JSON
```

The same JSON works on any platform through `call bootstrap --input bootstrap.json`. Read `ok`, `data`, `error` and `next`. Bootstrap points to the focused skill; `read_record` can read it from the binary, even without the checkout. Generic guides and skills remain in the toolkit. A private file at an allowed matching path is an explicit owner override.

Related source reads can use `query_batch`; related local changes can use `execute_batch`. Specialized tool shapes are available through `inspect_tools`. Follow paging and reported gaps before claiming a complete review. [Contracts and examples](job-harness-contract.md)

## Keep the four boundaries clear

| Where | What belongs there |
|---|---|
| Toolkit repo or native release | Reusable code, schemas, skills, guides and fictional examples |
| Private workspace `config/` | Timezone and paths selecting your CV and answer sources |
| Private workspace records and retained files | Career evidence, notes, preferences, vacancies, application history and saved submission proof |
| Agent or GUI host | Credentials, model access, connector sessions, permissions and schedules |

Configuration selects sources; it is not evidence for a career claim. A stored run describes intent, not permission. A submission with an unknown outcome must be investigated before retrying. Saved submission proof and application-confirmation emails are evidence to inspect, not permission to guess a new status.

Choose a [short workflow](workflows.md) for a note, a review, an application, an interview or a new role. `context_pack` retrieves bounded source passages and reports missing or uncertain history; the agent handles interpretation and writing.

## Back up, transfer or leave

Use a full private workspace bundle to carry your history forward. A profile-only bundle omits application history. Restore to a new/empty separate folder and inspect it before removing anything. Bundles contain private plaintext data; protect them when sharing. Credentials and connector sessions stay in the host.

[Backup format](../../docs/BUNDLES.md) · [Verify a backup and leave or switch tools](../../docs/INSTALL.md#leave-or-switch-tools)
