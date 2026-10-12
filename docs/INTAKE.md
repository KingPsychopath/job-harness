# Notes, transcripts and related context

Start with whatever you have: rough work notes, an interview summary, an existing transcript or an approved CV. There is no required career quiz, microphone permission or model download.

```sh
job-harness notes import --input supplied-notes.md --kind personal_notes \
  --link-project context/projects/example.md --key capture-example
job-harness notes list
job-harness notes review NOTE_ID --include-original
```

The project/application link must already exist. Omit a link when it is unknown. The returned `note_id` identifies the exact source. Types remain distinct: `personal_notes`, `transcript`, `provider_summary`, `generated_summary`.

Each intake has an immutable original, attributed metadata, separate proposals and separately accepted selections. Added details cite exact source hashes and line ranges. The engine checks their anchors, not whether a human interpretation is true. Selective acceptance preserves unselected hypotheses as unapproved. It never edits the CV or promotes a claim into career evidence.

| Date | Meaning |
|---|---|
| `occurred_at` | Supplied exact occurrence date/time |
| `occurred_period` | Supplied approximate period, retained as text |
| `recorded_at` | When the harness captured the material |
| `valid_from` / `valid_to` | Supplied validity metadata, without inferred applicability |
| `supersedes` | Link to an earlier intake, which remains retrievable |

Missing dates/speakers stay absent. “Technical round next week” cannot reconstruct a conversation or establish a calendar date.

## Propose and review

An external agent can produce the `propose_note` arguments directly. An optional compatible generative model can draft a cited enrichment:

```sh
job-harness context enrich --note-id NOTE_ID \
  --endpoint http://127.0.0.1:11434/v1 --model GENERATIVE_MODEL --revision MODEL_REVISION \
  --source context/projects/example.md > draft.json
jq '.data.propose_arguments' draft.json | job-harness call propose_note
job-harness notes review NOTE_ID --proposal-id PROPOSAL_ID --include-original
```

Generation requires a capable chat model; Gemma embeddings and Laya/Kev decisions are different jobs. Enrichment validates proposed source anchors before returning; it writes nothing. `accept_note` requires the current proposal hash, selected item IDs and attributed approval. The optional dashboard shows original, proposals, citations and selected acceptance on the same review page. `gui --allow-local-writes` enables those named local routes.

Archive changes review visibility and keeps originals, proposals, accepted views and evidence. Search excludes archived intake by default; direct reads and full bundles retain it.

## Retrieve related history

```sh
job-harness call context_pack <<'JSON'
{"task":"Prepare a project review","project":"context/projects/example.md","query":"autonomy","limit":8}
JSON
```

Exact search is the default. The derived SQLite index materializes source relationships. Context packs preserve supplied dates, line anchors, a reproducible pack hash and explicit coverage gaps. Optional hybrid retrieval helps when wording differs; [model adapters](MODELS.md) return passages rather than inferred qualifications.

## Optional companions and audio

[Google Foresight](https://developers.googleblog.com/google-ai-edge-with-embeddinggemma-2/) and [Granola](https://help.granola.ai/article/granola-mcp) can supply notes upstream. Paste/import the output; their recording, permissions, retention and plans stay outside the harness. A provider-generated summary remains labelled as one. No dependency on an upstream app's internal storage is required.

For an existing audio file, `context transcribe --audio FILE` uses an explicitly chosen compatible transcription endpoint. It returns draft text and audio identity, never live capture or automatic acceptance. This adapter has synthetic HTTP contract tests; no claim is made that a particular transcription model or live meeting capture was verified.

[Playbooks](../resources/guides/workflows.md) · [Selected documents](DOCUMENTS.md) · [Full private transfers](BUNDLES.md)
