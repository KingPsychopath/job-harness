# Contributing

Read [architecture](docs/ARCHITECTURE.md) and [AGENTS.md](AGENTS.md). Develop against synthetic workspaces; real CVs, inboxes, credentials and application history never belong in fixtures or screenshots.

Use the pinned Rust toolchain. Run `make check`; use `make check-parity` for contract, normalization, coordinator and transfer changes, and `make check-package` for embedded resources or installation changes. Test behavior and recovery, not a copy of the implementation.

Keep the pure domain independent of providers. Share engine rules across transports. Add a focused capability instead of arbitrary SQL, shell or expressions. Markdown evidence, operational coordinator state, derived views, typed config and host credentials have distinct owners.

Schemas and on-disk versions are compatibility contracts. Preserve unknown historical fields, exact artifact identities and unresolved attempts. Document deliberate semantic changes. Generic resources are canonical under `resources/`.

Use clear commits and report local, CI and live evidence separately. Public publication, visibility changes and paid deployment require owner authorization. [Security](SECURITY.md)
