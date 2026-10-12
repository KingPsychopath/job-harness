use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn native_stdio_handshake_catalog_and_call_are_protocol_only() {
    let temp = tempfile::tempdir().unwrap();
    job_harness_core::workspace::initialize(temp.path()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .arg("--root")
        .arg(temp.path())
        .args(["mcp", "--tool-profile", "focused"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    fn request(
        input: &mut impl Write,
        output: &mut impl BufRead,
        id: u64,
        method: &str,
        params: Value,
    ) -> Value {
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(output.read_line(&mut line).unwrap() > 0);
            let value: Value = serde_json::from_str(&line).expect("stdout contains only JSON-RPC");
            if value["id"] == id {
                return value;
            }
        }
    }
    let init = request(
        &mut input,
        &mut output,
        1,
        "initialize",
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"independent-native-test","version":"1"}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "job-harness");
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    input.flush().unwrap();
    let list = request(&mut input, &mut output, 2, "tools/list", json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 12);
    assert!(
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "begin_mail_archive")
    );
    let call = request(
        &mut input,
        &mut output,
        3,
        "tools/call",
        json!({"name":"bootstrap","arguments":{}}),
    );
    assert_eq!(call["result"]["structuredContent"]["schema_version"], 2);
    assert_eq!(call["result"]["structuredContent"]["ok"], true);
    drop(input);
    let _ = child.kill();
    assert!(child.wait().is_ok());
}
