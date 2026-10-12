use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub fn all() -> Value {
    serde_json::from_str(include_str!("../../models.json")).expect("bundled model catalog JSON")
}
pub fn runtimes() -> Value {
    serde_json::from_str(include_str!("../../runtimes.json")).expect("bundled runtime catalog JSON")
}
pub fn model(id: &str) -> Result<Value, String> {
    all()["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
        .ok_or_else(|| "unknown_model: select an explicit known model".into())
}
pub fn runtime(id: &str) -> Result<Value, String> {
    runtimes()["runtimes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
        .ok_or_else(|| "unsupported_runtime: select an explicit native runtime".into())
}
pub fn platform() -> String {
    let os = std::env::consts::OS;
    format!("{os}-{}", std::env::consts::ARCH)
}
pub fn selected_runtime(sdk: &str) -> Result<Value, String> {
    let platform = platform();
    let suffix = platform.replace("macos-aarch64", "macos-arm64");
    runtimes()["runtimes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["sdk"] == sdk && p["id"].as_str().is_some_and(|id| id.ends_with(&suffix)))
        .cloned()
        .ok_or_else(|| "unsupported_runtime: no reviewed native library for this platform".into())
}
pub fn token(spec: &Value) -> String {
    let mut scope = serde_json::Map::new();
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
            scope.insert(key.into(), v.clone());
        }
    }
    let data = serde_json::to_vec(&Value::Object(scope)).unwrap();
    hex::encode(Sha256::digest(data))[..16].into()
}
pub fn dir(cache: &Path, spec: &Value) -> PathBuf {
    cache.join(spec["id"].as_str().unwrap()).join(
        spec.get("revision")
            .and_then(Value::as_str)
            .unwrap_or(spec["version"].as_str().unwrap_or("unknown")),
    )
}
pub fn default_cache() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .unwrap_or_default(),
            )
            .join(".cache")
        });
    base.join("job-harness/models")
}
pub fn info(mut spec: Value, cache: &Path) -> Value {
    spec["approval_token"] = json!(token(&spec));
    spec["installed"] = json!(dir(cache, &spec).join("manifest.json").is_file());
    spec["automatic_downloads"] = json!(false);
    spec["local_hardware"] = json!({"platform":platform(),"logical_cpus":std::thread::available_parallelism().ok().map(|n|n.get()),"physical_ram_bytes":physical_ram()});
    if let Some(id) = spec["id"].as_str() {
        let sdk = match id {
            "embeddinggemma2-text" => Some("litert-lm"),
            "laya-s256" => Some("mediapipe"),
            "kev-0.8b" => Some("kev-metal"),
            _ => None,
        };
        if let Some(sdk) = sdk {
            spec["native_runtime_plan"] = selected_runtime(sdk).ok().unwrap_or(Value::Null);
        }
    }
    spec
}
pub fn summary() -> Value {
    let profiles=all()["profiles"].as_array().unwrap().iter().map(|p|{let mut selected=serde_json::Map::new();for key in ["id","status","capabilities","download_bytes","working_memory_bytes","hardware","runtime","revision","runtime_code_revision","model_card","license","license_basis","unknowns","native_components","measured"]{if let Some(v)=p.get(key){selected.insert(key.into(),v.clone());}}if let Some(components)=p["components"].as_array(){selected.insert("components".into(),json!(components.iter().map(|c|json!({"repository":c["repository"],"revision":c["revision"],"role":c["role"],"license":c["license"]})).collect::<Vec<_>>()));}Value::Object(selected)}).collect::<Vec<_>>();
    let runtimes=runtimes()["runtimes"].as_array().unwrap().iter().map(|r|json!({"id":r["id"],"sdk":r["sdk"],"version":r["version"],"download_bytes":r["download_bytes"],"license":r["license"],"execution_verified":r["id"].as_str().unwrap().ends_with("macos-arm64")})).collect::<Vec<_>>();
    json!({"schema_version":1,"required":false,"automatic_downloads":false,"native":true,"profiles":profiles,"runtimes":runtimes})
}
pub fn physical_ram() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut bytes = 0u64;
        let mut size = std::mem::size_of::<u64>();
        if unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&mut bytes as *mut u64).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } == 0
        {
            return Some(bytes);
        }
    }
    #[cfg(target_os = "linux")]
    {
        let memory = std::fs::read_to_string("/proc/meminfo").ok()?;
        let total = memory
            .lines()
            .find(|line| line.starts_with("MemTotal:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()?;
        return total.checked_mul(1024);
    }
    #[allow(unreachable_code)]
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_pins_and_totals_are_complete() {
        let catalog = all();
        let ids = catalog["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|profile| profile["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["embeddinggemma2-text", "laya-s256", "kev-0.8b"]);
        for id in ["embeddinggemma2-text", "laya-s256", "kev-0.8b"] {
            let spec = model(id).unwrap();
            assert!(spec["download_bytes"].as_u64().unwrap() > 0);
            assert!(!spec["revision"].as_str().unwrap().is_empty());
            if let Some(components) = spec["components"].as_array() {
                let total = components
                    .iter()
                    .flat_map(|c| c["files"].as_array().unwrap())
                    .map(|f| f["bytes"].as_u64().unwrap())
                    .sum::<u64>();
                assert_eq!(total, spec["download_bytes"].as_u64().unwrap());
                assert!(
                    components
                        .iter()
                        .flat_map(|c| c["files"].as_array().unwrap())
                        .all(|f| f["sha256"].as_str().is_some_and(|s| s.len() == 64))
                );
            }
        }
    }
    #[test]
    fn compact_summary_matches_canonical_catalog() {
        let view: Value = serde_json::from_str(include_str!("../../summary.json")).unwrap();
        assert_eq!(view, summary());
        assert!(
            view["profiles"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p.get("files").is_none())
        );
    }
}
