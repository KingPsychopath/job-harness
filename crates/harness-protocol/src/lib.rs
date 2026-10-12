//! Thin transports. Domain rules and workspace authority remain in harness-core.
pub mod acp;
#[cfg(windows)]
#[doc(hidden)]
pub mod acp_windows;
pub mod context_host;
pub mod dashboard;
pub mod mcp;
pub mod provider;

use serde_json::{Value, json};
use std::path::Path;

pub const FOCUSED_TOOLS: &[&str] = &[
    "bootstrap",
    "inspect_tools",
    "read_record",
    "query_batch",
    "execute_batch",
    "get_run",
    "prepare_application",
    "begin_submission",
    "begin_mail_archive",
    "finish_submission",
    "finish_run",
    "rebuild_pipeline",
];

/// Restricted hosts coordinate local work; they cannot mint or resolve submit permits.
pub fn coordination_call(name: &str, arguments: &Value) -> bool {
    if matches!(
        name,
        "begin_submission"
            | "begin_mail_archive"
            | "finish_submission"
            | "update_application"
            | "accept_note"
            | "archive_note"
            | "daily_update"
    ) || arguments.get("correction_reason").is_some()
        || (name == "record_event" && arguments.get("tracking").is_some())
    {
        return false;
    }
    !matches!(name, "query_batch" | "execute_batch")
        || arguments["steps"].as_array().is_some_and(|steps| {
            steps.iter().all(|step| {
                step["tool"].as_str().is_some_and(|tool| {
                    !matches!(tool, "query_batch" | "execute_batch")
                        && coordination_call(tool, &step["arguments"])
                })
            })
        })
}

pub fn denied() -> Value {
    json!({"schema_version":2,"ok":false,"data":null,"error":{"code":"tool_denied","message":"This host permits local coordination only. Submission boundaries and tracking corrections need an explicitly authorized external-capable host.","retryable":false,"recovery":"Continue with local coordination; review this action with the user and the appropriate host."},"next":[]})
}

/// Host-selected command only. No tool can change the selected workspace.
pub fn mcp_connection(executable: &Path, root: &Path, profile: &str) -> Value {
    json!({"command": executable, "args": ["--root", root.to_string_lossy().as_ref(), "mcp", "--tool-profile", profile]})
}

pub fn deferred(tool: &str) -> Value {
    json!({"schema_version":2,"ok":false,"data":null,"error":{
        "code":"deferred_tool","message":"Load this specialized schema and use a bounded batch.",
        "retryable":false,"recovery":"Inspect the schema, then use query_batch or execute_batch."
    },"next":[{"tool":"inspect_tools","arguments":{"names":[tool],"include_schemas":true},
        "reason":"Load the specialized schema before batching it."}]})
}

#[cfg(test)]
mod authority_tests {
    use super::*;

    #[test]
    fn restricted_hosts_cannot_hide_mail_mutations_in_batches() {
        for action in [
            "approve_archive",
            "begin_archive",
            "record_review",
            "record_archive_outcome",
            "reconcile_archive",
            "withdraw_archive",
        ] {
            let args = json!({"action":action});
            assert!(!coordination_call("daily_update", &args));
            assert!(!coordination_call(
                "execute_batch",
                &json!({"steps":[{
                    "id":"hidden", "tool":"daily_update", "arguments":args
                }]})
            ));
        }
        assert!(coordination_call("daily_check", &json!({})));
    }
}
