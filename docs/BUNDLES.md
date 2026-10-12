# Private workspace bundles

The code repository and installable package contain only generic tooling/resources. Each person owns a separate workspace. A `.jhb` bundle transfers that workspace explicitly, outside Git.

```sh
job-harness workspace export --output "$HOME/Career/backup.jhb"
job-harness --root "$HOME/Career/restored" workspace import --input "$HOME/Career/backup.jhb"
```

## Two scopes

`workspace` is the default: attributed context, content, workspace configuration, applications, coordinator state, saved application materials and evidence, outputs and original source material. It retains unknown/begun attempts and therefore their retry blocks. Generic linked guides/skills, Git history, scratch/build caches and host settings are excluded. Common credential-like filenames are filtered; this is not a secret-content scanner. Review evidence and original source material before transferring it. Credentials and browser sessions must never be stored in the workspace.

`profile` is for fresh onboarding: context, content, configured canonical answer sources and primary CV. It omits vacancy/coordinator history. Export is refused while any reserved, submitting or unknown attempt, or unfinished mail archive action, exists; use a full bundle to retain retry guards. This is not a continuation of an existing search; use a full bundle when duplicate prevention must retain earlier applications. Credential sessions and provider accounts must be connected separately in either case.

## Format and validation

A bundle is a **plaintext ZIP** with one `manifest.json`: format=job-harness.workspace, schema_version=1, kind, created_at and a list of path/size/SHA-256 entries. There is no automatic upload or password handling. Owner-only archive permissions reduce local exposure; encrypt using an established external tool before sharing. The software license does not authorize sharing candidate data.

Import accepts only allowlisted private paths and declared files, verifies digests, refuses symlinks, duplicate central-directory names and NFC/full-casefold collisions, traversal and expansion limits, and rejects an interrupted coordinator journal. It stages a new workspace and moves it into place only after validation. Existing nonempty destinations are never merged or replaced. Limits are 10,000 files, 32 MiB per file and 256 MiB expanded total; a manifest has a 2 MiB limit. Unsupported versions, encrypted entries, multi-disk and Zip64 layouts fail explicitly. Standard ZIP v1 archives produced by the historical exporter remain compatible.

Exports hold the coordinator's shared lock, so they wait for harness writes and reject a pending journal. Human edits outside that lock must be paused during a snapshot. Import rebuilds generated views from canonical records and fsyncs staging before publication. Derived SQLite files are never transferred. A profile import initializes fresh coordinator state; a full import preserves all submission/action history. Workspaces and bundle destinations inside the code checkout are refused.

A full restore preserves exact source and coordinator bytes, including unfinished
mail actions. It creates a new host-local mail review generation, excluded from
exports. Previous archive selections need human confirmation again before a new
begin; started or unknown changes still need mailbox readback. Restoring an older
backup cannot prove that an external action did not happen afterward.

A copied workspace is not synchronization: independent machines can independently act. A backup is passive. For a transfer, stop writers on the source before using the destination and designate one canonical workspace. Never fork an authenticated VM image containing a live coordinator or browser cookies into several active workers. [Architecture](ARCHITECTURE.md).
