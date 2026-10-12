use harness_core::{contracts, workspace};
use job_harness_protocol::{FOCUSED_TOOLS, mcp::McpServer};
use serde_json::{Value, json};

#[test]
fn focused_profile_defers_specialized_tools_with_actionable_next_step() {
    let temp = tempfile::tempdir().unwrap();
    workspace::initialize(temp.path()).unwrap();
    let specs = Value::Object(contracts::specs().clone());
    let server = McpServer::new(temp.path(), "focused", &specs).unwrap();
    assert_eq!(FOCUSED_TOOLS.len(), 11);
    assert!(contracts::specs().len() >= 30);
    assert_eq!(server.invoke("bootstrap", &json!({}))["ok"], true);
    let deferred = server.invoke("find_application", &json!({}));
    assert_eq!(deferred["error"]["code"], "deferred_tool");
    assert_eq!(deferred["next"][0]["tool"], "inspect_tools");
    assert_eq!(
        server.invoke("unknown", &json!({}))["error"]["code"],
        "unknown_tool"
    );
    let full = McpServer::new(temp.path(), "full", &specs).unwrap();
    assert_eq!(full.invoke("find_application", &json!({}))["ok"], true);
    let restricted = McpServer::with_policy(temp.path(), "full", &specs, true).unwrap();
    assert!(rmcp::ServerHandler::get_tool(&restricted, "begin_submission").is_none());
    assert!(rmcp::ServerHandler::get_tool(&restricted, "accept_note").is_none());
    assert!(rmcp::ServerHandler::get_tool(&restricted, "query_batch").is_some());
    assert_eq!(
        restricted.invoke("record_event", &json!({"tracking":{"stage":"saved"}}))["error"]["code"],
        "tool_denied"
    );
    for tool in [
        "begin_submission",
        "finish_submission",
        "update_application",
        "accept_note",
        "archive_note",
    ] {
        assert_eq!(
            restricted.invoke(tool, &json!({}))["error"]["code"],
            "tool_denied"
        );
        assert_eq!(
            restricted.invoke(
                "execute_batch",
                &json!({"steps":[{"id":"hidden","tool":tool,"arguments":{}}]})
            )["error"]["code"],
            "tool_denied"
        );
    }
}
