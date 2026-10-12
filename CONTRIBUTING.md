# Contributing

Start with [architecture](docs/ARCHITECTURE.md) and the source [AGENTS.md](AGENTS.md). Develop against synthetic workspaces; never copy a real CV, inbox or application history into a fixture, issue, screenshot or test artifact.

Install the dev extra in a virtual environment. Run `make check` for behavior/docs changes and `make check-package` when resources, install commands or package boundaries change. Runtime dependencies stay minimal. Test independent guarantees: stale/duplicate rejection, no retry after begin, partial batch recovery, concurrent writes, safe bundle import and real MCP/wheel boundaries.

Schemas, envelopes and on-disk versions are compatibility contracts. Update relevant docs and changelog with behavior. Generic resources have one canonical location under job_harness/resources. Host configuration and credentials never become package content.

Report local checks, remote CI and live provider evidence separately. A passing fixture does not verify an employer submission. Use clear focused commits. Public release/registry publication and changes to repository visibility require explicit owner authorization.
