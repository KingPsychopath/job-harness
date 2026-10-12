# A CV you can trace

![Synthetic one-page CV](example.png)

[Download the example PDF](example.pdf).

This example is entirely fictional. It demonstrates a one-page selection, not a career claim.

The JSON separates selected wording, layout knobs and evidence paths. Every summary, role,
skill and bullet cites workspace evidence. The harness validates shape and source existence;
it cannot certify that a claim is true. Review and approval still belong to the candidate.

```sh
job-harness --root /path/to/private/workspace cv validate --content content/selection.json
job-harness --root /path/to/private/workspace cv prepare --content content/selection.json --output output/preview.pdf
job-harness --root /path/to/private/workspace cv build --content content/selection.json --output output/preview.pdf
```

`prepare` emits JSON, the Typst template and a provenance manifest. `build` additionally calls
the installed `typst` executable and rejects a result longer than one page. It never changes
configuration or replaces the configured primary CV. Use `prepare` if your host supplies its own
compiler. The optional Typst adapter is the only external process in this feature.

Knobs: accent/ink colors, font, font size, page margin and section spacing. Prefer shorter,
clearer content to smaller type. Change selection JSON; keep facts in canonical evidence.

The native build adapter pins Typst 0.15.1. Preparation needs no Typst install.
Builds use a source-only staging directory, embedded fonts or explicit workspace `fonts/`,
and the approved date as a fixed creation timestamp. System fonts are ignored. The provenance
manifest records compiler, fonts, evidence hashes and final PDF hash. The same approved selection,
font files and compiler produce the same document. Font limits are 100 files, 16 MiB per file and
32 MiB total. Profile bundles carry the primary PDF; a full bundle also carries custom fonts.
