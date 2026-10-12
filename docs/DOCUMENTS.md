# Import once, present selected evidence

The agent supplies interpretation and wording. The harness preserves sources, validates structural selections and renders portable documents. There is no inferred identity profile or mandatory STAR questionnaire.

## Start with an existing CV or source

```sh
job-harness source import --input approved-cv.pdf
```

The native importer retains exact PDF/Markdown/text/JSON bytes under `sources/imports/`, with content identity and capture provenance. Repeating an identical import verifies the saved bytes. It does not parse career facts, approve claims, replace the primary CV or upload anything. The agent can read supplied text or use its host's document parser, then present sourced changes. Without a CV, start with one rough note using [intake](INTAKE.md).

## Draft a selected document

```sh
job-harness document template --kind cover_letter > selection.json
# Have your agent fill the selection with supported wording and source citations.
job-harness document render --input selection.json --output output/cover-letter.md --format markdown
job-harness document render --input selection.json --output output/cover-letter.json --format json
```

Templates cover cover letters, application answers, interview stories, portfolio case studies and period reviews. Selections contain title/status and sections with source hash/line citations. Empty placeholder text is refused. Approved status requires explicit attributed approval; it is not an independent truth certification. Changed sources block rendering until reconciled.

Markdown and JSON exports contain the selected wording and citation identities. They do not export all private history. Read the output before sharing: a selected subset can still contain private information. Outputs inside the workspace belong under `output/`; explicit external destinations are allowed. Paths refuse overwrite and protect the configured primary CV. Full backups preserve exact original artifacts; [profile/full bundles](BUNDLES.md) are separate.

For rendered CV PDFs, use the optional [Typst generator](CV.md), which supplies layout/theme controls and approval-linked evidence. Existing custom CVs remain supported. Other document formats can be produced by your agent's host from the selected Markdown/JSON; no document service is required by the engine.

## Observe progress without a platform score

```sh
job-harness workspace outcomes
```

The view reports vacancy counts, a separately identified recorded-submission denominator, pending cases and explicit unknowns. Closed/outcome alone does not imply a submission; recruiting progression is recorded local state rather than live ATS confirmation. Discovery channel and ATS host remain separate attributed fields. A closed role is not automatically a rejection, no-response is not “ghosted,” and small samples never become automatic platform recommendations. The engine reports what the records support; the agent discusses what it means.
