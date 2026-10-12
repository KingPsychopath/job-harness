//! Official Rust MCP SDK over local stdio, with compact version 2 envelopes.
use crate::{FOCUSED_TOOLS, deferred};
use harness_core::Engine;
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

const INSTRUCTIONS: &str = "First call bootstrap and read its focused skill. Batch related local work; load specialized schemas with inspect_tools. Canonical vacancy records and sourced answers control facts. A run or capability does not grant authority. Host connectors own browser, email, calendar and credentials. Freeze/reserve, begin once, submit externally once, finish with verified evidence. Never repeat a submitting or outcome_unknown attempt. These tools only mutate local records; they cannot submit forms, send mail or book meetings.";

#[derive(Clone)]
pub struct McpServer {
    engine: Arc<Engine>,
    tools: Arc<Vec<Tool>>,
    catalog: Arc<Vec<String>>,
    focused: bool,
    coordination_only: bool,
}

impl McpServer {
    pub fn new(root: &Path, profile: &str, specs: &Value) -> anyhow::Result<Self> {
        Self::with_policy(root, profile, specs, false)
    }
    pub fn with_policy(
        root: &Path,
        profile: &str,
        specs: &Value,
        coordination_only: bool,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            matches!(profile, "focused" | "full"),
            "Use focused or full tool profile."
        );
        let mut tools = Vec::new();
        let mut catalog = Vec::new();
        for (name, spec) in specs
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Tool catalog must be an object."))?
        {
            catalog.push(name.clone());
            if profile == "focused" && !FOCUSED_TOOLS.contains(&name.as_str()) {
                continue;
            }
            // Advertise only callable tools; batch child arguments are checked again at invocation.
            if coordination_only
                && !matches!(name.as_str(), "query_batch" | "execute_batch")
                && !crate::coordination_call(name, &json!({}))
            {
                continue;
            }
            let mut entry = spec.clone();
            entry["name"] = json!(name);
            tools.push(serde_json::from_value(entry)?);
        }
        Ok(Self {
            engine: Arc::new(Engine::open(root)?),
            tools: Arc::new(tools),
            catalog: Arc::new(catalog),
            focused: profile == "focused",
            coordination_only,
        })
    }

    pub fn invoke(&self, name: &str, arguments: &Value) -> Value {
        if self.coordination_only && !crate::coordination_call(name, arguments) {
            return crate::denied();
        }
        if self.focused && self.catalog.iter().any(|n| n == name) && !FOCUSED_TOOLS.contains(&name)
        {
            deferred(name)
        } else {
            self.engine.call(name, arguments)
        }
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "job-harness",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: self.tools.as_ref().clone(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| json!({}));
        let server = self.clone();
        let envelope =
            tokio::task::spawn_blocking(move || server.invoke(&request.name, &arguments))
                .await
                .map_err(|_| {
                    ErrorData::internal_error(
                        "Local tool task stopped; inspect unfinished work before retrying.",
                        None,
                    )
                })?;
        Ok(if envelope["ok"] == true {
            CallToolResult::structured(envelope)
        } else {
            CallToolResult::structured_error(envelope)
        }
        .into())
    }
}

pub async fn serve(
    root: &Path,
    profile: &str,
    specs: &Value,
    coordination_only: bool,
) -> anyhow::Result<()> {
    let service = McpServer::with_policy(root, profile, specs, coordination_only)?
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
