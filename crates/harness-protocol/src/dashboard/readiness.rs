//! Explicit, bounded device checks. A manifest is not a verified or running model.
use harness_core::{HarnessError, Result};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const MODELS: &str = include_str!("../../../../providers/local/models.json");
const RUNTIMES: &str = include_str!("../../../../providers/local/runtimes.json");
const ENDPOINT: &str = "http://127.0.0.1:18081/health";
const MODEL_IDS: &[&str] = &["embeddinggemma2-text", "laya-s256", "kev-0.8b"];
static ACTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn failure(code: &str, message: &str) -> HarnessError {
    HarnessError::new(code, message)
}
fn catalog(text: &str, key: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(text).expect("checked native artifact catalog")[key]
        .as_array()
        .expect("catalog entries")
        .clone()
}
fn profile(id: &str) -> Result<Value> {
    catalog(MODELS, "profiles")
        .into_iter()
        .find(|v| v["id"] == id)
        .ok_or_else(|| {
            failure(
                "unknown_model",
                "Choose one of the three fixed reviewed local models",
            )
        })
}
fn platform() -> String {
    format!(
        "{}-{}",
        if std::env::consts::OS == "macos" {
            "macos"
        } else {
            std::env::consts::OS
        },
        if std::env::consts::ARCH == "aarch64" && std::env::consts::OS == "macos" {
            "arm64"
        } else {
            std::env::consts::ARCH
        }
    )
}
fn runtime(model: &str) -> Option<Value> {
    let sdk = match model {
        "embeddinggemma2-text" => "litert-lm",
        "laya-s256" => "mediapipe",
        "kev-0.8b" => "kev-metal",
        _ => return None,
    };
    catalog(RUNTIMES, "runtimes")
        .into_iter()
        .find(|v| v["sdk"] == sdk && v["id"].as_str().is_some_and(|s| s.ends_with(&platform())))
}
fn scope(spec: &Value) -> String {
    let mut selected = serde_json::Map::new();
    for key in [
        "id",
        "revision",
        "artifact",
        "download_bytes",
        "sha256",
        "source",
        "model_card",
        "license",
        "hardware",
        "components",
        "member",
    ] {
        if let Some(v) = spec.get(key) {
            selected.insert(key.into(), v.clone());
        }
    }
    harness_core::storage::digest(
        &serde_json::to_vec(&Value::Object(selected)).expect("artifact scope"),
    )[..16]
        .to_owned()
}
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}
fn cache() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|p| p.join(".cache")))
        .map(|p| p.join("job-harness/models"))
}
fn search_paths() -> Vec<PathBuf> {
    let mut paths = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .filter(|p| p.is_absolute())
                .take(64)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = home() {
        paths.push(home.join(".local/bin"));
    }
    #[cfg(target_os = "macos")]
    {
        paths.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
    }
    paths
}
fn executable_in(paths: &[PathBuf], name: &str) -> Option<PathBuf> {
    let name = format!("{name}{}", if cfg!(windows) { ".exe" } else { "" });
    paths
        .iter()
        .filter_map(|p| p.join(&name).canonicalize().ok())
        .find(|p| p.is_file())
}
fn executable(name: &str) -> Option<PathBuf> {
    executable_in(&search_paths(), name)
}

async fn output(binary: &Path, args: &[&str], deadline: Duration) -> Result<Vec<u8>> {
    let mut command = Command::new(binary);
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    command
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Loader/runtime essentials only. Host credentials, proxies and configuration stay out.
    for name in ["SystemRoot", "WINDIR", "TMP", "TEMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command.spawn().map_err(|_| {
        failure(
            "device_probe_failed",
            "Could not launch the fixed native component",
        )
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| failure("device_probe_failed", "Native output pipe unavailable"))?;
    let result = timeout(deadline, async {
        let mut bytes = Vec::new();
        let read = async { stdout.take(65537).read_to_end(&mut bytes).await };
        let (_, status) = tokio::try_join!(read, child.wait())?;
        Ok::<_, std::io::Error>((bytes, status))
    })
    .await;
    match result {
        Ok(Ok((bytes, status))) if status.success() && bytes.len() <= 65536 => Ok(bytes),
        _ => {
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(2), child.wait()).await;
            Err(failure(
                "device_probe_failed",
                "The fixed native check failed, exceeded its output bound or timed out",
            ))
        }
    }
}
fn version_text(bytes: &[u8], name: &str) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.len() > 200 || !text.starts_with(&format!("{name} ")) {
        return None;
    }
    let version = text.split_whitespace().nth(1)?;
    if version.len() > 60
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
        || !version.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    Some(version.to_owned())
}
async fn tool(name: &str) -> Value {
    let Some(binary) = executable(name) else {
        return json!({"id":name,"state":"not_found","version":null});
    };
    match output(&binary, &["--version"], Duration::from_secs(2))
        .await
        .ok()
        .and_then(|b| version_text(&b, name))
    {
        Some(version) => {
            let compatible = !["job-harness", "job-harness-local"].contains(&name)
                || version == env!("CARGO_PKG_VERSION");
            let mut value = json!({"id":name,"state":if compatible{"available"}else{"incompatible_version"},"version":version});
            if name == "job-harness" && compatible {
                value["command_path"] = json!(binary);
            }
            value
        }
        None => json!({"id":name,"state":"probe_failed","version":null}),
    }
}
fn manifest(cache: Option<&Path>, spec: &Value) -> Value {
    let Some(cache) = cache else {
        return json!({"state":"location_unknown","manifest_matches_pin":false,"artifacts_verified":false});
    };
    let revision = spec["revision"]
        .as_str()
        .or_else(|| spec["version"].as_str())
        .unwrap_or("unknown");
    let path = cache
        .join(spec["id"].as_str().unwrap())
        .join(revision)
        .join("manifest.json");
    let state = match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "absent",
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 4096 => {
            if harness_core::storage::reject_symlink_ancestors(&path).is_err() {
                "unreadable"
            } else {
                match fs::read(path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                {
                    Some(value) if value["id"] == spec["id"] && value["scope"] == scope(spec) => {
                        "manifest_present"
                    }
                    Some(_) => "manifest_mismatch",
                    None => "unreadable",
                }
            }
        }
        _ => "unreadable",
    };
    json!({"state":state,"manifest_matches_pin":state=="manifest_present","artifacts_verified":false})
}
fn health_state(status: u16, value: &Value) -> Value {
    let recognized =
        value["native"] == true && value["python_runtime"] == false && value["worker"].is_object();
    json!({"state":if !recognized{"unrecognized"}else if status==200&&value["ok"]==true{"responding"}else{"worker_unavailable"},"address":"http://127.0.0.1:18081","model_identities_available":false})
}
async fn health() -> Value {
    let unreachable = || json!({"state":"unreachable","address":"http://127.0.0.1:18081","model_identities_available":false});
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return unreachable();
    };
    let Ok(mut response) = client.get(ENDPOINT).send().await else {
        return unreachable();
    };
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if bytes.len() + chunk.len() <= 8192 => bytes.extend(chunk),
            Ok(None) => break,
            _ => {
                return json!({"state":"unrecognized","address":"http://127.0.0.1:18081","model_identities_available":false});
            }
        }
    }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return json!({"state":"unrecognized","address":"http://127.0.0.1:18081","model_identities_available":false});
    };
    health_state(status, &value)
}

pub async fn inspect() -> Value {
    let (typst, provider, cli, running) = tokio::join!(
        tool("typst"),
        tool("job-harness-local"),
        tool("job-harness"),
        health()
    );
    let cache = cache();
    let models=MODEL_IDS.iter().map(|id|{let spec=profile(id).expect("fixed model ID");let sdk=runtime(id);json!({"id":id,"name":match *id{"embeddinggemma2-text"=>"Gemma passages","laya-s256"=>"Laya decisions",_=>"Kev decisions"},"download_bytes":spec["download_bytes"],"license":spec["license"],"model_card":spec["model_card"],"hardware":spec["hardware"],"cache":manifest(cache.as_deref(),&spec),"runtime":sdk.as_ref().map(|s|json!({"id":s["id"],"cache":manifest(cache.as_deref(),s)})),"platform_runtime_listed":sdk.is_some(),"gui_install_supported":*id!="kev-0.8b"&&sdk.is_some(),"running":"unknown"})}).collect::<Vec<_>>();
    json!({"schema_version":1,"checked_at":harness_core::domain::now(),"platform":platform(),"core":{"state":"running","version":env!("CARGO_PKG_VERSION")},"tools":[typst,provider,cli],"models":models,"provider":running,"checks":"User-requested fixed version/manifest/default-loopback checks; no weight hashing, scans, downloads or connection grants","custom_caches_and_endpoints_checked":false})
}

async fn provider() -> Result<PathBuf> {
    let binary = executable("job-harness-local").ok_or_else(|| {
        failure(
            "provider_missing",
            "Install the optional native provider in your user bin directory first",
        )
    })?;
    let version = output(&binary, &["--version"], Duration::from_secs(2)).await?;
    if version_text(&version, "job-harness-local").as_deref() != Some(env!("CARGO_PKG_VERSION")) {
        return Err(failure(
            "incompatible_provider",
            "Use the native provider matching this Job Harness release",
        ));
    }
    Ok(binary)
}
async fn provider_json(binary: &Path, args: &[&str], deadline: Duration) -> Result<Value> {
    let value: Value =
        serde_json::from_slice(&output(binary, args, deadline).await?).map_err(|_| {
            failure(
                "incompatible_provider",
                "Native provider did not return bounded versioned JSON",
            )
        })?;
    if value["schema_version"] != 2 || value["ok"] != true || !value["data"].is_object() {
        return Err(failure(
            "provider_action_failed",
            "Native provider action failed; retain partial cache files and review before retrying",
        ));
    }
    Ok(value["data"].clone())
}
fn installable(id: &str) -> Result<(Value, Value)> {
    let model = profile(id)?;
    if id == "kev-0.8b" {
        return Err(failure(
            "advanced_runtime_required",
            "Kev needs the separately reviewed Swift/Metal worker; GUI installation is available for Gemma and Laya",
        ));
    }
    let sdk = runtime(id).ok_or_else(|| {
        failure(
            "unsupported_runtime",
            "This platform has no pinned native runtime for the selected model",
        )
    })?;
    Ok((model, sdk))
}
fn item(kind: &str, spec: &Value) -> Value {
    let mut value = json!({"kind":kind,"id":spec["id"],"approval_token":scope(spec)});
    for key in [
        "revision",
        "version",
        "download_bytes",
        "source",
        "sha256",
        "license",
        "hardware",
        "member",
        "native_sha256",
        "model_card",
        "license_basis",
    ] {
        if let Some(field) = spec.get(key) {
            value[key] = field.clone();
        }
    }
    value
}
fn fixed_plan(id: &str) -> Result<Value> {
    let (model, sdk) = installable(id)?;
    let mut value = json!({"schema_version":1,"model_id":id,"platform":platform(),"provider_version":env!("CARGO_PKG_VERSION"),"items":[item("runtime",&sdk),item("model",&model)],"download_bytes":model["download_bytes"].as_u64().unwrap()+sdk["download_bytes"].as_u64().unwrap(),"hardware":model["hardware"],"automatic_downloads":false,"auto_start":false,"credential_access":false});
    value["plan_sha256"] = json!(harness_core::storage::digest(
        &harness_core::storage::encoded(&value)
    ));
    Ok(value)
}
async fn reviewed_context(id: &str) -> Result<(Value, PathBuf, PathBuf)> {
    let value = fixed_plan(id)?;
    let binary = provider().await?;
    let cache = cache().ok_or_else(|| {
        failure(
            "cache_unavailable",
            "A user-owned default model cache could not be selected",
        )
    })?;
    let cache_text = cache.to_str().ok_or_else(|| {
        failure(
            "cache_unavailable",
            "Model cache path cannot be passed to the native provider",
        )
    })?;
    let info = provider_json(
        &binary,
        &["info", "--model", id, "--cache", cache_text],
        Duration::from_secs(3),
    )
    .await?;
    let model = profile(id)?;
    if info["id"] != id
        || info["approval_token"] != scope(&model)
        || info["native_runtime_plan"]["id"] != value["items"][0]["id"]
    {
        return Err(failure(
            "incompatible_provider",
            "Native provider pins differ from the reviewed model/runtime plan",
        ));
    }
    let sdk = runtime(id).expect("fixed plan selected a platform runtime");
    let sdk_info = provider_json(
        &binary,
        &[
            "info",
            "--runtime",
            sdk["id"].as_str().unwrap(),
            "--cache",
            cache_text,
        ],
        Duration::from_secs(3),
    )
    .await?;
    if sdk_info["approval_token"] != scope(&sdk)
        || sdk
            .as_object()
            .unwrap()
            .iter()
            .any(|(key, value)| sdk_info.get(key) != Some(value))
    {
        return Err(failure(
            "incompatible_provider",
            "Native runtime sources, digests or approval scope differ from the reviewed SDK pin",
        ));
    }
    Ok((value, binary, cache))
}
pub async fn plan(id: &str) -> Result<Value> {
    Ok(reviewed_context(id).await?.0)
}
fn ensure_reviewed_plan(plan: &Value, expected: &str) -> Result<()> {
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || plan["plan_sha256"] != expected
    {
        return Err(failure(
            "stale_install_plan",
            "The reviewed plan changed; review current sources, licenses and download size before confirming again",
        ));
    }
    Ok(())
}
pub async fn verify(id: &str) -> Result<Value> {
    let _guard = ACTION.try_lock().map_err(|_| {
        failure(
            "device_action_in_progress",
            "Another local artifact action is running; wait for its result",
        )
    })?;
    let (value, binary, cache) = reviewed_context(id).await?;
    let cache_text = cache
        .to_str()
        .ok_or_else(|| failure("cache_unavailable", "Default cache path unsupported"))?;
    for item in value["items"].as_array().unwrap() {
        let flag = if item["kind"] == "runtime" {
            "--runtime"
        } else {
            "--model"
        };
        let result = provider_json(
            &binary,
            &[
                "verify",
                flag,
                item["id"].as_str().unwrap(),
                "--cache",
                cache_text,
            ],
            Duration::from_secs(90),
        )
        .await?;
        if result["verified"] != true {
            return Err(failure(
                "model_integrity_error",
                "Pinned artifact verification did not confirm exact bytes",
            ));
        }
    }
    Ok(
        json!({"model_id":id,"artifacts_verified":true,"verified_at":harness_core::domain::now(),"running":"unknown","plan_sha256":value["plan_sha256"]}),
    )
}
pub async fn install(id: &str, expected_plan_sha256: &str) -> Result<Value> {
    let _guard = ACTION.try_lock().map_err(|_| {
        failure(
            "device_action_in_progress",
            "Another local artifact action is running; wait for its result",
        )
    })?;
    // Reject a changed/malformed reviewed scope before any executable or network action.
    ensure_reviewed_plan(&fixed_plan(id)?, expected_plan_sha256)?;
    let (value, binary, cache) = reviewed_context(id).await?;
    ensure_reviewed_plan(&value, expected_plan_sha256)?;
    let cache_text = cache
        .to_str()
        .ok_or_else(|| failure("cache_unavailable", "Default cache path unsupported"))?;
    let mut completed = Vec::new();
    for item in value["items"].as_array().unwrap() {
        let flag = if item["kind"] == "runtime" {
            "--runtime"
        } else {
            "--model"
        };
        let result = provider_json(
            &binary,
            &[
                "download",
                flag,
                item["id"].as_str().unwrap(),
                "--approve",
                item["approval_token"].as_str().unwrap(),
                "--cache",
                cache_text,
            ],
            Duration::from_secs(1200),
        )
        .await
        .and_then(|result| {
            if result["installed"] == true && result["id"] == item["id"] {
                Ok(result)
            } else {
                Err(failure(
                    "incompatible_provider",
                    "Native provider did not confirm this exact installed artifact",
                ))
            }
        });
        if let Err(error) = result {
            return Ok(
                json!({"model_id":id,"status":"partial_or_failed","completed":completed,"failed_item":item["id"],"error_code":error.code,"artifacts_verified":false,"running":"unknown","plan_sha256":value["plan_sha256"],"next_action":"Keep existing cache files. Refresh readiness and verify completed artifacts before explicitly retrying."}),
            );
        }
        completed.push(json!({"id":item["id"],"kind":item["kind"],"verified":true}));
    }
    Ok(
        json!({"model_id":id,"status":"installed_verified","completed":completed,"artifacts_verified":true,"verified_at":harness_core::domain::now(),"running":"unknown","plan_sha256":value["plan_sha256"],"auto_started":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifests_cannot_claim_verified_running_or_a_different_pin() {
        let temp = tempfile::tempdir().unwrap();
        let spec = profile("laya-s256").unwrap();
        let path = temp
            .path()
            .join("laya-s256")
            .join(spec["revision"].as_str().unwrap());
        fs::create_dir_all(&path).unwrap();
        assert_eq!(manifest(Some(temp.path()), &spec)["state"], "absent");
        fs::write(
            path.join("manifest.json"),
            json!({"id":"laya-s256","scope":scope(&spec)}).to_string(),
        )
        .unwrap();
        let found = manifest(Some(temp.path()), &spec);
        assert_eq!(found["state"], "manifest_present");
        assert_eq!(found["artifacts_verified"], false);
        fs::write(path.join("manifest.json"), "{\"id\":\"wrong\"}").unwrap();
        assert_eq!(
            manifest(Some(temp.path()), &spec)["state"],
            "manifest_mismatch"
        );
        fs::write(path.join("manifest.json"), vec![b' '; 4097]).unwrap();
        assert_eq!(manifest(Some(temp.path()), &spec)["state"], "unreadable");
    }
    #[test]
    fn health_is_provider_evidence_not_per_model_evidence() {
        let healthy = health_state(
            200,
            &json!({"ok":true,"native":true,"python_runtime":false,"worker":{"load_ms":1}}),
        );
        assert_eq!(healthy["state"], "responding");
        assert_eq!(healthy["model_identities_available"], false);
        assert_eq!(
            health_state(200, &json!({"ok":true}))["state"],
            "unrecognized"
        );
        assert_eq!(
            health_state(
                503,
                &json!({"native":true,"python_runtime":false,"worker":{}})
            )["state"],
            "worker_unavailable"
        );
    }
    #[test]
    fn versions_reject_arbitrary_output_and_plans_bind_exact_pins() {
        assert_eq!(
            version_text(b"typst 0.14.0 (public-build)\n", "typst"),
            Some("0.14.0".into())
        );
        assert!(version_text(b"secret stuff", "typst").is_none());
        assert!(version_text(b"typst /private/path", "typst").is_none());
        assert!(fixed_plan("unknown").is_err());
        assert!(fixed_plan("kev-0.8b").is_err());
        if let Ok(mut plan) = fixed_plan("laya-s256") {
            let expected = plan["plan_sha256"].as_str().unwrap().to_owned();
            assert!(ensure_reviewed_plan(&plan, &expected).is_ok());
            assert!(ensure_reviewed_plan(&plan, &"g".repeat(64)).is_err());
            assert!(ensure_reviewed_plan(&plan, &"0".repeat(64)).is_err());
            let digest = plan.as_object_mut().unwrap().remove("plan_sha256").unwrap();
            assert_eq!(
                digest,
                harness_core::storage::digest(&harness_core::storage::encoded(&plan))
            );
            assert_eq!(plan["auto_start"], false);
            assert!(
                plan["items"][0]["sha256"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64)
            );
        }
    }
    #[test]
    fn fixed_lookup_never_accepts_an_arbitrary_request_name() {
        let tmp = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "job-harness-local.exe"
        } else {
            "job-harness-local"
        };
        fs::write(tmp.path().join(name), b"fixture").unwrap();
        assert!(executable_in(&[tmp.path().to_owned()], "job-harness-local").is_some());
        assert!(executable_in(&[tmp.path().to_owned()], "typst").is_none());
    }
}
