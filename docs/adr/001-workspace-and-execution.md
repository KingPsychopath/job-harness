# 001: Keep durable workspace state separate from execution

Status: accepted for the private preview.

The initial implementation mixed reusable code, candidate facts and personal Git history. That made code distribution and concurrent agents harder to reason about.

The reusable repository now contains code, synthetic tests, generic skills/resources and project documentation. Personal configuration and evidence live in external workspaces. Local host settings select a workspace without becoming package data. Private bundles transfer data deliberately; they contain no Git history or credential sessions.

CLI, MCP and future GUIs use one typed API. Ordinary operations use local functions, while host connectors perform authorized external work. Execution can be short-lived or persistent without changing ownership of facts and outcomes. No VM provisioning is a prerequisite.

This preserves easy local use and permits later packaging/UI work. Multi-host concurrency still requires a shared transactional coordinator; ZIP copies and local filesystem locks are not a cloud synchronization protocol. Public package publication and a visibility change remain separate decisions.
