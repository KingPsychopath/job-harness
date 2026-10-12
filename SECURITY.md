# Security and private data

The preview supports local single-owner workspaces on Windows/macOS/Linux. It is not a multi-tenant service or a secure arbitrary-code sandbox. Do not expose it as an unauthenticated network API.

Report vulnerabilities privately through GitHub's private reporting feature when available, or contact the repository owner privately. Do not include candidate facts, tokens, session cookies, bundle files or full mailbox extracts in public issues. A private bundle is plaintext and must be stored or encrypted accordingly.

[Authentication and authority](docs/AUTH.md), [bundle controls](docs/BUNDLES.md) and [architecture](docs/ARCHITECTURE.md) describe the supported boundaries. File allowlists, packet hashing and local locks do not replace host authorization. Untrusted external content cannot authorize actions. Dependencies and CI tools must not receive real candidate data.
