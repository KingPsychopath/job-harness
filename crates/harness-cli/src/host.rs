//! Explicit host settings hold paths only, never candidate facts or credentials.
use job_harness_core::{Engine, HarnessError};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn user_settings_base() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config"))
}
pub fn settings_path() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(user_settings_base)
        .ok_or_else(|| {
            HarnessError::new(
                "invalid_arguments",
                "Select --root explicitly or configure a user-owned host settings directory (XDG_CONFIG_HOME, APPDATA on Windows, or HOME).",
            )
        })?;
    Ok(base.join("job-harness/settings.json"))
}

pub fn resolve(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("JOB_HARNESS_WORKSPACE") {
        return Ok(PathBuf::from(path));
    }
    let path = settings_path()?;
    if !path.is_file() {
        return Err(HarnessError::new(
            "invalid_arguments",
            "Select --root PATH or run job-harness configure --workspace PATH.",
        )
        .into());
    }
    let data: Value = serde_json::from_slice(&fs::read(&path)?)?;
    if data["schema_version"] != 1 || !data["workspace"].is_string() {
        return Err(HarnessError::new(
            "invalid_arguments",
            "Invalid host settings; supply --root or configure your workspace again.",
        )
        .into());
    }
    Ok(PathBuf::from(data["workspace"].as_str().unwrap()))
}

pub fn configure(root: &Path) -> anyhow::Result<Value> {
    let root = root.canonicalize()?;
    let ready = Engine::open(&root)?.call("bootstrap", &json!({}));
    if ready["ok"] != true {
        return Err(HarnessError::new(
            "invalid_arguments",
            "Choose an initialized readable workspace.",
        )
        .into());
    }
    let path = settings_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid settings destination."))?;
    fs::create_dir_all(parent)?;
    if fs::symlink_metadata(parent)?.file_type().is_symlink() || path.is_symlink() {
        return Err(
            HarnessError::new("invalid_arguments", "Host settings may not use symlinks.").into(),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(
        serde_json::to_string_pretty(&json!({"schema_version":1,"workspace":root}))?.as_bytes(),
    )?;
    file.as_file().sync_all()?;
    file.persist(&path)
        .map_err(|_| HarnessError::new("io_error", "Could not save host settings."))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(
        json!({"workspace":root,"settings":path,"note":"Host-local path only. Credentials stay in the host's secure store."}),
    )
}
