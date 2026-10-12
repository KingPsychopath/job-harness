//! One lossless coordinator parser shared by the engine and workspace transfers.
use crate::{Error, Result, storage};
use job_harness_domain::{SubmissionLifecycle, SubmissionStatus};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

pub fn parse(root: &Path, value: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "invalid_state",
            "Malformed coordinator record; preserve and repair its exact source",
        )
    };
    if value["schema_version"] != 1
        || !value["runs"].is_object()
        || !value["attempts"].is_object()
        || !value["events"].is_array()
    {
        return Err(invalid());
    }
    for (id, run) in value["runs"].as_object().unwrap() {
        let row = run.as_object().ok_or_else(invalid)?;
        if row.get("id").and_then(Value::as_str) != Some(id.as_str())
            || !row.get("scope").is_some_and(Value::is_object)
        {
            return Err(invalid());
        }
        for key in ["operation", "status", "started_at"] {
            if !row.get(key).is_some_and(Value::is_string) {
                return Err(invalid());
            }
        }
    }
    for (id, attempt) in value["attempts"].as_object().unwrap() {
        let row = attempt.as_object().ok_or_else(invalid)?;
        if row.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return Err(invalid());
        }
        for key in [
            "run_id",
            "packet_id",
            "record",
            "company",
            "vacancy_url",
            "source",
            "reserved_at",
        ] {
            if !row.get(key).is_some_and(Value::is_string) {
                return Err(invalid());
            }
        }
        SubmissionStatus::parse(&attempt["status"])?;
    }
    for event in value["events"].as_array().unwrap() {
        let row = event.as_object().ok_or_else(invalid)?;
        for key in ["idempotency_key", "request_sha256", "tool", "at"] {
            if !row.get(key).is_some_and(Value::is_string) {
                return Err(invalid());
            }
        }
        if !row.get("result").is_some_and(Value::is_object) {
            return Err(invalid());
        }
    }
    let mut normalized = value.clone();
    if let Some(pending) = value.get("pending") {
        if pending.is_null() || pending.as_array().is_some_and(Vec::is_empty) {
            normalized.as_object_mut().unwrap().remove("pending");
        } else {
            let mut seen = BTreeSet::new();
            for change in pending.as_array().ok_or_else(invalid)? {
                let path = change
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                if !seen.insert(path) {
                    return Err(Error::new(
                        "invalid_state",
                        "Pending journal contains duplicate paths",
                    ));
                }
                journal_path(root, path)?;
                if !change.get("text").is_some_and(Value::is_string)
                    || !change
                        .get("before_sha256")
                        .is_some_and(|v| v.is_null() || v.is_string())
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(normalized)
}
pub fn journal_path(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let path = storage::safe_path(
        root,
        relative,
        &["applications", "context"],
        &[".md", ".json"],
    )?;
    storage::reject_symlink_ancestors(&path)?;
    if relative.starts_with("context/intake/") {
        return Ok(path);
    }
    if path.parent() != Some(root.canonicalize()?.join("applications").as_path())
        || path.extension().and_then(|e| e.to_str()) != Some("md")
    {
        return Err(Error::new(
            "path_denied",
            "Journal paths must be canonical vacancy records or explicit intake data",
        ));
    }
    Ok(path)
}
pub fn load(root: &Path) -> Result<Value> {
    let path = storage::safe_path(
        root,
        "applications/operations/state.json",
        &["applications"],
        &[".json"],
    )?;
    storage::reject_symlink_ancestors(&path)?;
    if !path.is_file() {
        if ["packets", "receipts", "conflicts"]
            .iter()
            .any(|dir| root.join("applications/operations").join(dir).exists())
        {
            return Err(Error::new(
                "invalid_state",
                "Coordinator state is missing while operational artifacts exist. Restore its exact state backup; never create a fresh coordinator over existing attempts.",
            ));
        }
        return Ok(json!({"schema_version":1,"runs":{},"attempts":{},"events":[]}));
    }
    parse(root, &serde_json::from_slice(&fs::read(path)?)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_pending_is_clean_without_changing_source_bytes() {
        let root = tempfile::tempdir().unwrap();
        for pending in [Value::Null, json!([])] {
            let raw = json!({"schema_version":1,"runs":{},"attempts":{},"events":[],"pending":pending,"future":{"a":1}});
            let normalized = parse(root.path(), &raw).unwrap();
            assert!(normalized.get("pending").is_none());
            assert!(raw.get("pending").is_some());
            assert_eq!(normalized["future"], raw["future"]);
        }
        assert!(
            parse(
                root.path(),
                &json!({"schema_version":1,"runs":{},"attempts":{},"events":[],"pending":{}})
            )
            .is_err()
        );
    }
    #[test]
    fn orphaned_operational_artifacts_do_not_reset_history() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("applications/operations/packets")).unwrap();
        assert_eq!(load(root.path()).unwrap_err().code, "invalid_state");
        assert!(
            !root
                .path()
                .join("applications/operations/state.json")
                .exists()
        );
    }
}
