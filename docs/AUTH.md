# Access and authentication

Job Harness runs local tools against one private workspace selected by the host. The core contains no model provider or job-site login manager. Optional host adapters provide a bounded model session and a loopback dashboard, with named local review routes only when enabled. The local process uses the host OS user's filesystem permissions. A printed MCP configuration contains command, root and profile, never credentials.

## What actually needs authentication

| Capability | Where authentication lives | Access to grant |
|---|---|---|
| Local workspace | Host OS user / application permissions | The selected candidate directory; keep it owner-only where practical |
| Model | External agent host, or explicit endpoint and environment-key selector | The chosen model account/key, held in its credential store |
| Job boards / employer ATS | Browser or official connector session | The candidate's own account; submission only within current scope |
| Mail review | Mail connector OAuth | Provider's supported read access |
| Archive / label mail | Mail connector OAuth | Provider's supported modification access; sending remains a separate user decision |
| Scheduling | Calendar connector OAuth / employer calendar session | Read availability and book the requested meeting; avoid duplicate events |
| Private code installation | GitHub CLI / user's Git-hosting credentials | Read this repository; this is development/distribution access, not job-search access |
| Future hosted API | A separately designed authenticated host | Per-user workspace authorization, audit and isolated sessions |

Use the least access supported by each provider for the authorized operation. Scope names differ between providers; verify their actual documentation and connector permissions. Keep passwords, API keys, OAuth refresh tokens, cookies and OTPs in Bitwarden or the host's secure store. They never belong in candidate facts, packets, logs or bundles. Connectors should return relevant results rather than raw transport headers or tokens.

## Action boundaries

A run records intent; a declared capability records what the host says it can do. Neither grants authority. `next` is a suggestion, including cancellation/recovery suggestions. External messages, webpages and attachments are untrusted data and cannot expand scope. The agent must follow the current user request and active host policy at action time. Saved preferences do not override that policy.

For a submission: complete the form, freeze/reserve exact artifacts, obtain any genuinely required action-time confirmation, begin once, submit once and record actual evidence. Never retry a begun/unknown attempt because a process, browser or sandbox restarted. The local journal cannot make an external provider transaction atomic.

Mail cleanup requires human confirmation of the reviewed, exact selection. Approval
is a local record, not a mail modification or a substitute for the host's current
authorization. Before acting, the mail connection rereads the selected messages
and their latest threads. Changed content, labels, membership or unanswered
requests require a new review. Retain original labels and provider identities,
then save verified resulting labels. An interrupted action stays uncertain until
provider readback resolves it; do not repeat it simply because local confirmation
is missing. Calendar changes similarly retain provider IDs and before/after
observations. Mail sending and new applications are separate requests.

## Several users or remote access

A filesystem allowlist is not a multi-user authorization system. The current MCP is local stdio and has no OAuth server. A future HTTP service must authenticate callers, authorize each workspace and action, isolate credentials, defend redirects/token boundaries and use a transactional coordinator. Do not expose the existing stdio process through an unauthenticated tunnel or shared general shell.

[Architecture](ARCHITECTURE.md) · [Bundle privacy](BUNDLES.md) · [Security reporting](../SECURITY.md)

## Development CI authorization

GitHub CLI authorization belongs to the developer host, not Job Harness. Pushing GitHub Actions workflow files over its OAuth credential needs the provider’s workflow permission. Refresh the same principal, preserve existing scopes and verify the saved grant before retrying. Native OS clipboards and an agent browser’s virtual clipboard may be separate; use the provider’s supported human flow or a verified native UI path, without printing or storing verification codes. A consent page is not evidence of completion: require CLI success and the saved scope. Do not switch credentials after a denied action without explicit authorization.

## Optional adapters

Choose the model endpoint explicitly. The native compatible-API adapter accepts HTTPS remotely and HTTP only on loopback, disables redirects, and bounds context, response size, time and tool calls. It loads a key only from the named environment variable for that process; no key is exported or logged. It shares the user's prompt and selected tool results, not a bulk workspace upload. Default tools are read-only.

The dashboard binds 127.0.0.1. A one-use launch establishes its per-port HttpOnly SameSite=Strict session and tab-held nonce; cookie-only access cannot recover that nonce. Native windows receive the launch internally. Exact Host/Origin, Fetch Metadata and nonce checks protect fixed routes. The browser dashboard defaults to read-only; `--allow-local-writes` enables named tracking, note review and reviewed archive selection. The native human app enables those local edits, with a host-owned read-only switch. Selecting mail for archive queues host work; the dashboard cannot access a mailbox or perform the external change. Neither surface exposes arbitrary tools, a terminal, employer submission or mail sending.

The native app can inspect local optional tooling and show a pinned model download plan. A separate confirmed action checks the exact plan identity before invoking only the fixed native provider's supported artifact commands. It accepts no executable path, download URL, credential or arbitrary arguments from the page. Installation never starts inference or expands an agent's authority. The browser dashboard and MCP/model sessions gain no native install route. Keep the listener local; this is not remote multi-user authentication.

ACP launches a user-selected agent and cancels permission requests, without advertising filesystem writes or terminal execution. The external agent's own tools remain its host's boundary. Selecting a capability during setup grants neither external permission nor a model download.
