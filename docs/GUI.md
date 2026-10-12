# One workspace, several drivers

The GUI, CLI and MCP clients use the same engine and canonical state. Opening the desktop app creates no second tracker. A reviewed GUI write becomes visible to the next CLI/MCP read. Native filesystem notifications signal background CLI/agent changes; the GUI then reads a fresh engine snapshot. Notifications contain only revision/mode hints, never source text or a competing record.

## Work surfaces

Applications open as a board. Drag a card to another stage; click it for details and reviewed changes. A table remains available. Applications and Notes are the primary surfaces; attention, evidence search, activity and connections open as secondary inspectors. Attention gathers unfinished runs, uncertain outcomes, pending note review and supplied recruiting observations that still need action. It does not scan a mailbox itself. Notes keep originals alongside selectively accepted cited additions. Connections explain host/model boundaries; they are not a credential vault.

Updates use an authenticated local stream, with no recurring directory scans or content hashing. Bursts coalesce before the view rereads the engine. While a review, draft or drag is active, a small update notice offers a safe refresh instead of replacing the interaction. The Refresh icon explicitly rereads the current view; it never retries a submission or runs a model. Hidden windows disconnect the stream and reconnect on return. Filesystem notifications are change hints, not a guaranteed history: unsupported/network filesystems and watcher errors leave manual Refresh available.

The desktop app enables named, reviewed local edits for its human controls. **Workspace → Read only** locks or enables those actions; the native host owns the switch. Use `--read-only` for a restricted desktop session. The browser dashboard defaults to read-only and uses an explicit `--allow-local-writes` launch option. Each write checks current source identities and selected items. No dashboard action submits an employer form, sends mail, books a calendar slot or runs an arbitrary command. Changing a tracker status records a reviewed observation; it does not perform the external action.

Connections checks fixed local tools, bounded cache manifests and the optional provider's fixed loopback health endpoint when opened or refreshed. A matching manifest means files are present; verification hashes the exact artifacts only on request. The native app can review and confirm a fixed Gemma/Laya runtime and model download plan. A changed plan is refused; partial downloads stay explicit. It does not start a model, connect an agent automatically or read credentials. The large metadata-only model candidates are removed; Kev remains a separate advanced worker.

## Chat is a host choice

You can use the GUI while an agent works through CLI/MCP. Durable state, run results and events are the common continuation. A GUI action does not automatically inject a message into an unrelated agent conversation. The host can read or subscribe to its own supported signals, then query the current result.

The current optional model loop is an explicit CLI session, and ACP launches a separately selected agent. There is no compulsory built-in chat account or fake transcript. A future session panel should show actual streamed output and tool results from one identified host/model session, preserve cancellation/recovery state, and disclose selected context sent to a provider.

Keep chat separate from the source of truth: conversation text may suggest a change; a reviewed engine command commits it. Browser, mail and calendar sessions stay in the user's host.

## Native window

The optional [desktop package](../apps/desktop/README.md) opens the fixed loopback UI with OS window decoration, system appearance and normal text/clipboard controls. Launch it normally: it uses your configured workspace or asks you to choose an existing one. Remembering that folder is optional. The CLI remains independently installable. Native window acceptance and binary signing are platform-specific checks.
