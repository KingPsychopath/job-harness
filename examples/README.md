# Try the toolkit with synthetic data

Use `make demo` from the checkout, or run the script with an installed binary:

```sh
./scripts/demo.sh
```

The demo creates a temporary workspace outside the repository, bootstraps, reads two sources in a batch, starts and finishes a bounded local run, indexes sources, and transfers a full bundle into a fresh destination. It never submits an application or sends an email.

[Generic CV example](cv/selection.json) · [Rendered CV](cv/example.pdf) · [ACP fixture](agent/acp-fixture.rs)

[Notes and review](../docs/INTAKE.md) · [Selected document exports](../docs/DOCUMENTS.md) · [Local model smoke tests](models/README.md)

For MCP, use `job-harness connect mcp` and call bootstrap in your own client. For local or remote models, select a tool-capable model and endpoint as described in [agent wiring](../docs/AGENTS.md). No credential is supplied in an example.
