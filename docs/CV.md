# Evidence-linked CVs

The optional generator combines typed JSON content with an embedded Typst template. Existing approved PDFs and custom Typst projects remain usable as primary CVs; no automatic conversion or wording change occurs.

See [synthetic content](../examples/cv/selection.json) and its [one-page PDF](../examples/cv/example.pdf). Personal examples belong in your external workspace.

```sh
job-harness cv validate --input examples/cv/selection.json
# Copy the example structure into your workspace and replace it with attributed facts.
job-harness cv prepare --content content/cv.json --output output/cv.pdf
job-harness cv build --content content/cv.json --output output/cv.pdf
```

Preparation validates the content, approval and evidence references and creates derived staging artifacts. Build additionally requires the host's `typst` executable. Theme controls include accent, font, margins and text sizes. The one-page default is checked after compilation. Candidate text is passed as JSON, never interpolated into Typst source.

Use deterministic timestamps and supplied fonts for reproducible output. The compile root contains only staged template/data/fonts. Never regenerate an already frozen packet or overwrite the configured primary CV. Select a reviewed new PDF explicitly in workspace configuration.

Approval says the candidate has authorized wording; it does not independently verify a claim. Read the cited evidence and inspect the rendered PDF before an application uses it.
