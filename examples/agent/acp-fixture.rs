//! Synthetic ACP peer for protocol smoke tests; no model, files or actions.
//! cargo build -p job-harness --example acp-fixture
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    run_fixture();
}
pub fn run_fixture() {
    let mut prompt_id = Value::Null;
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = &request["id"];
        if request["method"].is_null() && request["id"] == 100 {
            assert_eq!(request["result"]["outcome"]["outcome"], "cancelled");
            println!(
                "{}",
                json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"synthetic-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ACP handoff, workspace MCP and cancelled permission verified."}}}})
            );
            println!(
                "{}",
                json!({"jsonrpc":"2.0","id":prompt_id,"result":{"stopReason":"end_turn"}})
            );
            continue;
        }
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => {
                json!({"protocolVersion":1,"agentCapabilities":{},"agentInfo":{"name":"synthetic-protocol-fixture","version":"1"}})
            }
            "session/new" => {
                let server = &request["params"]["mcpServers"][0];
                assert_eq!(server["name"], "job-harness");
                assert!(
                    std::path::Path::new(request["params"]["cwd"].as_str().unwrap()).is_absolute()
                );
                assert!(
                    server["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a == "--deny-external-actions")
                );
                json!({"sessionId":"synthetic-session"})
            }
            "session/prompt" => {
                if std::env::args().any(|arg| arg == "--timeout-peer") {
                    let directory = std::env::args().next_back().unwrap();
                    std::fs::write(
                        std::path::Path::new(&directory).join("peer.pid"),
                        std::process::id().to_string(),
                    )
                    .unwrap();
                    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                        .arg("--lingering-child")
                        .arg(&directory)
                        .spawn()
                        .unwrap();
                    std::thread::sleep(std::time::Duration::from_secs(100));
                    let _ = child.wait();
                    continue;
                }
                prompt_id = id.clone();
                println!(
                    "{}",
                    json!({"jsonrpc":"2.0","id":100,"method":"session/request_permission","params":{"sessionId":"synthetic-session","toolCall":{"toolCallId":"synthetic-call","title":"Synthetic action"},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"}]}})
                );
                continue;
            }
            _ => continue,
        };
        if !id.is_null() {
            println!("{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
        }
    }
}
