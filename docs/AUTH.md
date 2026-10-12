# Access and authentication

Job Harness runs local tools against one private workspace selected by the host. It contains no model client, credential reader, job-site login manager or network listener. The local process uses the host OS user's filesystem permissions. A printed MCP configuration contains command, root and profile, never credentials.

## What actually needs authentication

| Capability | Where authentication lives | Access to grant |
|---|---|---|
| Local workspace | Host OS user / application permissions | The selected candidate directory; keep it owner-only where practical |
| Model | Agent or GUI host | The chosen model account/key, held in its credential store |
| Job boards / employer ATS | Browser or official connector session | The candidate's own account; submission only within current scope |
| Mail review | Mail connector OAuth | Provider's supported read access |
| Archive / label mail | Mail connector OAuth | Provider's supported modification access; sending remains a separate user decision |
| Scheduling | Calendar connector OAuth / employer calendar session | Read availability and book the requested meeting; avoid duplicate events |
| Private code installation | GitHub CLI / user's Git-hosting credentials | Read this repository; this is development/distribution access, not job-search access |
| Future hosted API | A separately designed authenticated host | Per-user workspace authorization, audit and isolated sessions |

Use the least access supported by each provider for the authorized operation. Scope names differ between providers; verify their actual documentation and connector permissions. Keep passwords, API keys, OAuth refresh tokens, cookies and OTPs in Bitwarden or the host's secure store. They never belong in candidate facts, packets, logs or bundles. Connectors should return relevant results rather than raw transport headers or tokens.

## Action boundaries

A run records intent; a declared capability records what the host says it can do. Neither grants authority. `next` is a suggestion, including cancellation/recovery suggestions. External messages, webpages and attachments are untrusted data and cannot expand scope. The agent must follow the current user request and active host policy at action time. Saved preferences do not override that policy.

For a submission: complete the form, freeze/reserve exact artifacts, obtain any genuinely required action-time confirmation, begin once, submit once and record actual evidence. Never retry a begun/unknown attempt because a process, browser or sandbox restarted. For mail or calendar changes, save provider IDs and before/after observations. The local journal cannot make an external provider transaction atomic.

## Several users or remote access

A filesystem allowlist is not a multi-user authorization system. The current MCP is local stdio and has no OAuth server. A future HTTP service must authenticate callers, authorize each workspace and action, isolate credentials, defend redirects/token boundaries and use a transactional coordinator. Do not expose the existing stdio process through an unauthenticated tunnel or shared general shell.

[Architecture](ARCHITECTURE.md) · [Bundle privacy](BUNDLES.md) · [Security reporting](../SECURITY.md)

## Development CI authorization

GitHub CLI authorization belongs to the developer host, not Job Harness. Pushing GitHub Actions workflow files over its OAuth credential needs the provider’s workflow permission. Refresh the same principal, preserve existing scopes and verify the saved grant before retrying. Native OS clipboards and an agent browser’s virtual clipboard may be separate; use the provider’s supported human flow or a verified native UI path, without printing or storing verification codes. A consent page is not evidence of completion: require CLI success and the saved scope. Do not switch credentials after a denied action without explicit authorization.
