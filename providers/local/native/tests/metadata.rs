use std::process::Command;
#[test]
fn metadata_needs_no_sdk_or_network() {
    let output = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
        .args(["list"])
        .env("HF_HUB_OFFLINE", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let data = &data["data"];
    assert_eq!(data["automatic_downloads"], false);
    let ids = data["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["embeddinggemma2-text", "laya-s256", "kev-0.8b"]);
    assert_eq!(data["profiles"][0]["download_bytes"], 164626432);
    assert!(
        data["runtimes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "mediapipe-1.1.0-windows-aarch64")
    );
}
#[test]
fn actual_worker_has_fresh_environment_and_no_sdk() {
    let output = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
        .args(["doctor"])
        .env("SYNTHETIC_API_TOKEN", "fixture-only-not-a-secret")
        .env("AWS_SESSION_TOKEN", "fixture-only-not-a-secret")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let data = &data["data"];
    assert_eq!(data["sdk_loads"], 0);
    assert_eq!(data["native_worker"]["native"], true);
    assert_eq!(data["native_worker"]["python_runtime"], false);
    assert_eq!(data["native_worker"]["host_credentials_inherited"], false);
    #[cfg(unix)]
    assert_eq!(data["native_worker"]["core_dump_limit_bytes"], 0);
    #[cfg(windows)]
    assert_eq!(
        data["native_worker"]["core_dump_limit_bytes"],
        serde_json::Value::Null
    );
}
#[test]
fn unapproved_download_rejected_without_fetch() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
        .args([
            "download",
            "--model",
            "embeddinggemma2-text",
            "--approve",
            "unapproved",
            "--cache",
            temp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("approval_required"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[test]
fn removed_large_models_are_unknown_without_fetch() {
    let temp = tempfile::tempdir().unwrap();
    for model in ["kev-4b", "clef-flash"] {
        let output = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
            .args([
                "download",
                "--model",
                model,
                "--approve",
                "anything",
                "--cache",
                temp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "unknown_model");
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
#[test]
fn help_and_error_recovery_are_readable() {
    let help = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Serve never downloads"));
    let error = Command::new(env!("CARGO_BIN_EXE_job-harness-local"))
        .args(["info", "--unknown", "fixture"])
        .output()
        .unwrap();
    assert!(!error.status.success());
    let response: serde_json::Value = serde_json::from_slice(&error.stderr).unwrap();
    assert_eq!(response["schema_version"], 2);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "invalid_arguments");
    assert_eq!(response["error"]["message"], "unknown flag");
    assert!(!response["next"].as_array().unwrap().is_empty());
}
