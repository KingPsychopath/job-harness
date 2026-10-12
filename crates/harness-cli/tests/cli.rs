use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .arg("--root")
        .arg(root)
        .args(args)
        .env(
            "XDG_CONFIG_HOME",
            root.parent().unwrap().join("host-settings"),
        )
        .output()
        .unwrap()
}
fn output(out: Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("Invalid stdout: {}", String::from_utf8_lossy(&out.stdout)))
}

#[test]
fn native_cli_keeps_compact_contract_and_workspace_outside_code() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("career");
    assert_eq!(output(run(&root, &["init"]))["ok"], true);
    let bootstrap = output(run(&root, &["call", "bootstrap"]));
    assert_eq!(bootstrap["schema_version"], 2);
    assert_eq!(bootstrap["ok"], true);
    let catalog = output(run(&root, &["schemas"]));
    assert_eq!(
        catalog.as_object().unwrap().len(),
        job_harness_core::contracts::specs().len()
    );
    let config = output(run(&root, &["connect", "mcp"]));
    assert_eq!(config["ok"], true);
    assert_eq!(
        config["data"]["args"],
        json!([
            "--root",
            root.canonicalize().unwrap(),
            "mcp",
            "--tool-profile",
            "focused"
        ])
    );
    assert!(
        Path::new(config["data"]["command"].as_str().unwrap())
            .file_stem()
            .is_some_and(|stem| stem == "job-harness")
    );
    let failed = run(&root, &["call", "unknown_tool"]);
    assert!(!failed.status.success());
    assert_eq!(output(failed)["error"]["code"], "unknown_tool");
}

#[test]
fn host_default_roundtrip_and_invalid_input_are_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    assert_eq!(output(run(&root, &["init"]))["ok"], true);
    assert_eq!(
        output(run(
            &root,
            &["configure", "--workspace", root.to_str().unwrap()]
        ))["ok"],
        true
    );
    let default = Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .args(["call", "bootstrap"])
        .env("XDG_CONFIG_HOME", temp.path().join("host-settings"))
        .env_remove("JOB_HARNESS_WORKSPACE")
        .output()
        .unwrap();
    assert_eq!(output(default)["ok"], true);
    let bad = temp.path().join("bad.json");
    std::fs::write(&bad, "[]").unwrap();
    let result = run(
        &root,
        &["call", "bootstrap", "--input", bad.to_str().unwrap()],
    );
    assert!(!result.status.success());
    assert_eq!(output(result)["error"]["code"], "invalid_arguments");
}

#[test]
fn notes_cli_preserves_original_and_pipeline_check_has_no_events() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    assert_eq!(output(run(&root, &["init"]))["ok"], true);
    let original = temp.path().join("supplied.md");
    let text = "# Supplied notes\nA candidate supplied an example; dates are unknown.\n";
    std::fs::write(&original, text).unwrap();
    let imported = output(run(
        &root,
        &[
            "notes",
            "import",
            "--input",
            original.to_str().unwrap(),
            "--key",
            "notes-cli-fixture",
        ],
    ));
    assert_eq!(imported["ok"], true);
    let id = imported["data"]["note_id"].as_str().unwrap();
    let review = output(run(&root, &["notes", "review", id, "--include-original"]));
    assert_eq!(review["data"]["original"]["text"], text);
    assert_eq!(output(run(&root, &["pipeline"]))["ok"], true);
    let before = std::fs::read(root.join("applications/operations/state.json")).unwrap();
    assert_eq!(output(run(&root, &["pipeline", "--check"]))["ok"], true);
    assert_eq!(
        std::fs::read(root.join("applications/operations/state.json")).unwrap(),
        before
    );
    let models = Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .args(["context", "models"])
        .env_remove("JOB_HARNESS_WORKSPACE")
        .env("HOME", temp.path())
        .env("XDG_CONFIG_HOME", temp.path().join("unconfigured-host"))
        .output()
        .unwrap();
    assert_eq!(output(models)["ok"], true);
}

#[test]
fn presentation_cli_preserves_supplied_sources_and_keeps_templates_fact_free() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    assert_eq!(output(run(&root, &["init"]))["ok"], true);
    let source = temp.path().join("supplied.txt");
    std::fs::write(&source, "A supplied example.\nUnselected detail.\n").unwrap();
    let imported = output(run(
        &root,
        &["source", "import", "--input", source.to_str().unwrap()],
    ));
    assert_eq!(imported["ok"], true);
    let template = output(run(
        &temp.path().join("not-initialized"),
        &["document", "template", "--kind", "interview_story"],
    ));
    assert_eq!(template["status"], "draft");
    assert_eq!(template["sections"][0]["text"], "");
    assert!(!temp.path().join("not-initialized").exists());
    let selection = json!({"schema_version":1,"title":"Supplied example","status":"draft","sections":[{"heading":"Story","text":"A supplied example.","citations":[{"path":imported["data"]["path"],"sha256":imported["data"]["sha256"],"start_line":1,"end_line":1}]}]});
    let input = temp.path().join("selection.json");
    std::fs::write(&input, selection.to_string()).unwrap();
    let rendered = output(run(
        &root,
        &[
            "document",
            "render",
            "--input",
            input.to_str().unwrap(),
            "--output",
            "output/story.md",
        ],
    ));
    assert_eq!(rendered["ok"], true);
    assert_eq!(rendered["data"]["citations_checked"], 1);
    let bytes = std::fs::read_to_string(root.join("output/story.md")).unwrap();
    assert!(bytes.contains("A supplied example."));
    assert!(!bytes.contains("Unselected detail."));
    assert_eq!(
        output(run(&root, &["workspace", "outcomes"]))["data"]["counts"]["total"],
        0
    );
    let human = run(&root, &["status"]);
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Job Harness · Workspace")
    );
    assert_eq!(
        output(run(&root, &["status", "--json"]))["data"]["health"]["records"],
        0
    );
}

#[cfg(windows)]
#[test]
fn windows_host_settings_use_appdata_and_preserve_drive_paths_with_spaces() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("career workspace with spaces");
    assert_eq!(output(run(&root, &["init"]))["ok"], true);
    let appdata = temp.path().join("Host Settings");
    let configured = Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .args(["configure", "--workspace"])
        .arg(&root)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .env("APPDATA", &appdata)
        .output()
        .unwrap();
    assert_eq!(output(configured)["ok"], true);
    assert!(appdata.join("job-harness/settings.json").is_file());
    let selected = Command::new(env!("CARGO_BIN_EXE_job-harness"))
        .args(["call", "bootstrap"])
        .env_remove("JOB_HARNESS_WORKSPACE")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .env("APPDATA", &appdata)
        .output()
        .unwrap();
    assert_eq!(
        output(selected)["data"]["ledger"],
        serde_json::json!(root.canonicalize().unwrap())
    );
}
