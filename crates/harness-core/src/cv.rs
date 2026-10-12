//! Evidence-linked CV selection with an explicit optional Typst build adapter.
use crate::{HarnessError, Result};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;
const SCHEMA: &str = include_str!("../../../resources/cv/schema.json");
const TYPST_VERSION: &str = "0.15.1";
const TEMPLATE: &str = include_str!("../../../resources/cv/resume.typ");
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn failure(message: impl Into<String>) -> HarnessError {
    HarnessError::validation(message)
}
fn evidence(value: &Value, paths: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if (key == "evidence" || key == "sources") && value.is_array() {
                    for source in value.as_array().unwrap() {
                        if let Some(source) = source.as_str() {
                            paths.insert(source.into());
                        }
                    }
                } else {
                    evidence(value, paths);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                evidence(item, paths)
            }
        }
        _ => {}
    }
}

pub fn validate(content: &Value) -> Result<Value> {
    let schema: Value = serde_json::from_str(SCHEMA)?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| failure(format!("Bundled CV schema is invalid: {e}")))?;
    if let Some(error) = validator.iter_errors(content).next() {
        return Err(failure(format!(
            "CV selection does not match schema: {error}"
        )));
    }
    let mut paths = BTreeSet::new();
    evidence(content, &mut paths);
    for path in &paths {
        crate::workspace::validate_relative_data(
            path,
            &["context", "applications", "sources"],
            None,
        )?;
        if !crate::bundle::allowed_path(path) {
            return Err(failure(
                "CV evidence must not reference credential, bundle or cache files",
            ));
        }
    }
    if chrono::NaiveDate::parse_from_str(
        content["provenance"]["approved_at"].as_str().unwrap(),
        "%Y-%m-%d",
    )
    .is_err()
    {
        return Err(failure("CV approval date is invalid"));
    }
    Ok(
        json!({"valid":true,"schema_version":1,"evidence_paths":paths,"note":"Schema and provenance references are checked; the candidate must verify the truth of claims."}),
    )
}

pub fn validate_file(root: &Path, content: &str) -> Result<Value> {
    let (selection, _) = load_selection(root, content)?;
    let result = validate(&selection)?;
    validate_evidence(root, &selection)?;
    Ok(result)
}
fn load_selection(root: &Path, content: &str) -> Result<(Value, Vec<u8>)> {
    crate::workspace::validate_relative_data(content, &["content"], Some("json"))?;
    let path = crate::workspace::safe_path(root, content)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(failure("CV selection exceeds 1 MiB"));
    }
    let selection: Value = serde_json::from_slice(&bytes)?;
    validate(&selection)?;
    Ok((selection, bytes))
}
fn validate_evidence(root: &Path, selection: &Value) -> Result<Vec<Value>> {
    let mut paths = BTreeSet::new();
    evidence(selection, &mut paths);
    let mut sources = Vec::new();
    for relative in paths {
        let path = crate::workspace::safe_path(root, &relative)?;
        if !path.is_file() {
            return Err(failure(format!("CV evidence file is missing: {relative}")));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(failure("CV evidence source exceeds size limit"));
        }
        sources.push(json!({"path":relative,"sha256":digest(&bytes)}));
    }
    Ok(sources)
}
fn output_paths(root: &Path, output: &str) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    crate::workspace::validate_relative_data(output, &["output"], Some("pdf"))?;
    let config = crate::workspace::settings(root)?;
    if config["primary_cv"].as_str().is_some_and(|primary| {
        primary.nfc().case_fold().collect::<String>()
            == output.nfc().case_fold().collect::<String>()
    }) {
        return Err(failure(
            "Generate a separate preview; the configured primary CV is preserved",
        ));
    }
    let pdf = crate::workspace::safe_path(root, output)?;
    let stem = pdf
        .file_stem()
        .and_then(|x| x.to_str())
        .ok_or_else(|| failure("Invalid CV output filename"))?;
    let parent = pdf.parent().unwrap();
    Ok((
        pdf.clone(),
        parent.join(format!("{stem}.harness.typ")),
        parent.join(format!("{stem}.harness.json")),
        parent.join(format!("{stem}.provenance.json")),
    ))
}
fn prepare_inner(root: &Path, content: &str, output: &str) -> Result<Value> {
    let (selection, bytes) = load_selection(root, content)?;
    let sources = validate_evidence(root, &selection)?;
    let (pdf, typ, data, provenance) = output_paths(root, output)?;
    fs::create_dir_all(pdf.parent().unwrap())?;
    let mut manifest = json!({"schema_version":1,"source":{"path":content,"sha256":digest(&bytes)},"template_sha256":digest(TEMPLATE.as_bytes()),"evidence":sources,"approved_by":selection["provenance"]["approved_by"],"approved_at":selection["provenance"]["approved_at"],"note":"Derived selection; source references do not themselves prove the truth of claims."});
    let output_relative = |path: &Path| crate::storage::portable_relative(root, path);
    manifest["artifacts"] = json!({"typst":output_relative(&typ)?,"content":output_relative(&data)?,"pdf":output_relative(&pdf)?});
    crate::storage::atomic_write(&typ, TEMPLATE.as_bytes())?;
    crate::storage::atomic_write(&data, &serde_json::to_vec_pretty(&selection)?)?;
    crate::storage::atomic_write(&provenance, &serde_json::to_vec_pretty(&manifest)?)?;
    Ok(
        json!({"prepared":true,"source":content,"output":output,"typst":output_relative(&typ)?,"content":output_relative(&data)?,"provenance":output_relative(&provenance)?,"built":false,"next":["Run the explicit cv build adapter or compile the emitted template with your host.","Inspect the one-page PDF and verify selected claims before choosing it as primary."]}),
    )
}
pub fn prepare(root: &Path, content: &str, output: &str) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let _guard = crate::storage::WorkspaceLock::acquire(&root, true)?;
    prepare_inner(&root, content, output)
}
/// Optional native compiler adapter. It never reads outside its source-only stage.
pub fn build(root: &Path, content: &str, output: &str) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let _guard = crate::storage::WorkspaceLock::acquire(&root, true)?;
    let version=Command::new("typst").arg("--version").output().map_err(|error|HarnessError::new("compiler_unavailable",format!("Install Typst {TYPST_VERSION} for cv build, or use cv prepare with a host compiler: {error}")))?;
    let version = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    if version.split_whitespace().nth(1) != Some(TYPST_VERSION) {
        return Err(HarnessError::new(
            "compiler_version",
            format!(
                "Reproducible cv build requires Typst {TYPST_VERSION}; found {version}. cv prepare supports a separate host compiler."
            ),
        ));
    }
    let mut result = prepare_inner(&root, content, output)?;
    let (pdf, _, _, provenance) = output_paths(&root, output)?;
    let (selection, _) = load_selection(&root, content)?;
    let date = chrono::NaiveDate::parse_from_str(
        selection["provenance"]["approved_at"]
            .as_str()
            .expect("validated approval date"),
        "%Y-%m-%d",
    )
    .map_err(|_| failure("Invalid approval date"))?;
    let timestamp = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp();
    let stage = tempfile::Builder::new()
        .prefix(".job-harness-cv-")
        .tempdir_in(pdf.parent().unwrap())?;
    let typ = stage.path().join("resume.typ");
    let data = stage.path().join("resume.content.json");
    let compiled = stage.path().join("preview.pdf");
    crate::storage::atomic_write(&typ, TEMPLATE.as_bytes())?;
    crate::storage::atomic_write(&data, &serde_json::to_vec_pretty(&selection)?)?;
    let fonts = stage.path().join("fonts");
    fs::create_dir(&fonts)?;
    let mut font_sources = Vec::new();
    let mut font_bytes = 0u64;
    let source_fonts = root.join("fonts");
    if source_fonts.exists() {
        if fs::symlink_metadata(&source_fonts)?
            .file_type()
            .is_symlink()
        {
            return Err(failure("CV fonts must not be symlinked"));
        }
        for entry in walkdir::WalkDir::new(&source_fonts)
            .follow_links(false)
            .sort_by_file_name()
        {
            let entry = entry.map_err(|e| failure(e.to_string()))?;
            if entry.file_type().is_symlink() {
                return Err(failure("CV fonts must not follow symlinks"));
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = crate::storage::portable_relative(&root, entry.path())?;
            if !["ttf", "otf", "ttc", "otc"].contains(
                &entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or(""),
            ) {
                continue;
            }
            let path = crate::workspace::safe_path(&root, &relative)?;
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            font_bytes += bytes.len() as u64;
            if font_sources.len() >= 100
                || font_bytes > 32 * 1024 * 1024
                || bytes.len() > 16 * 1024 * 1024
            {
                return Err(failure("CV fonts exceed the documented size limits"));
            }
            let local = entry
                .path()
                .strip_prefix(&source_fonts)
                .map_err(|_| failure("Invalid font path"))?;
            let target = fonts.join(local);
            crate::storage::atomic_write(&target, &bytes)?;
            font_sources.push(json!({"path":relative,"sha256":digest(&bytes)}));
        }
    }
    let compile = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(stage.path())
        .arg("--ignore-system-fonts")
        .arg("--font-path")
        .arg(&fonts)
        .arg("--creation-timestamp")
        .arg(timestamp.to_string())
        .arg(&typ)
        .arg(&compiled)
        .output()?;
    if !compile.status.success() {
        return Err(HarnessError::new(
            "compiler_failed",
            format!(
                "Typst could not compile the prepared selection: {}",
                String::from_utf8_lossy(&compile.stderr)
                    .chars()
                    .take(1200)
                    .collect::<String>()
            ),
        ));
    }
    let pages = Command::new("typst")
        .arg("eval")
        .arg("--root")
        .arg(stage.path())
        .arg("--ignore-system-fonts")
        .arg("--font-path")
        .arg(&fonts)
        .arg("--in")
        .arg(&typ)
        .arg("query(<harness-pages>).map(it => it.value)")
        .output()?;
    if !pages.status.success() {
        return Err(HarnessError::new(
            "compiler_failed",
            "Typst page-count verification failed; the PDF was not published",
        ));
    }
    let count: Value = serde_json::from_slice(&pages.stdout)?;
    if count != json!([1]) {
        return Err(HarnessError::new(
            "cv_overflow",
            "The selection exceeds one page; shorten the content or adjust the documented layout knobs",
        ));
    }
    let bytes = fs::read(compiled)?;
    crate::storage::atomic_write(&pdf, &bytes)?;
    let mut manifest: Value = serde_json::from_slice(&fs::read(&provenance)?)?;
    manifest["build"] = json!({"compiler":version,"creation_timestamp":timestamp,"fonts":font_sources,"system_fonts":false,"pages":1,"pdf_sha256":digest(&bytes)});
    crate::storage::atomic_write(&provenance, &serde_json::to_vec_pretty(&manifest)?)?;
    result["built"] = json!(true);
    result["pages"] = json!(1);
    result["compiler"] = json!(version);
    result["pdf_sha256"] = json!(digest(&bytes));
    result["next"] = json!([
        "Inspect the PDF and selected claims before choosing it as primary. The current primary CV was preserved."
    ]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        serde_json::from_str(include_str!("../../../examples/cv/selection.json")).unwrap()
    }
    fn workspace() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        crate::workspace::populate(t.path()).unwrap();
        fs::write(t.path().join("context/profile/example.md"),"# Synthetic example\nAll facts in this fixture are fictional and approved only as a demonstration.").unwrap();
        fs::write(
            t.path().join("content/selection.json"),
            serde_json::to_vec(&sample()).unwrap(),
        )
        .unwrap();
        t
    }
    #[test]
    fn synthetic_selection_validates() {
        assert_eq!(validate(&sample()).unwrap()["valid"], true);
    }
    #[test]
    fn no_missing_provenance_or_traversal() {
        let mut value = sample();
        value["summary"]["evidence"] = json!([]);
        assert!(validate(&value).is_err());
        let mut value = sample();
        value["summary"]["evidence"] = json!(["context/../../secret"]);
        assert!(validate(&value).is_err());
        let mut value = sample();
        value["summary"]["evidence"] = json!(["context/credentials.json"]);
        assert!(validate(&value).is_err());
    }
    #[test]
    fn primary_cv_cannot_be_replaced() {
        let t = workspace();
        let mut config = crate::workspace::settings(t.path()).unwrap();
        config["primary_cv"] = json!("output/primary.pdf");
        fs::write(
            t.path().join("config/career-ledger.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        assert!(prepare(t.path(), "content/selection.json", "output/primary.pdf").is_err());
    }
    #[test]
    fn prepare_checks_sources_and_emits_only_derived_artifacts() {
        let t = workspace();
        prepare(t.path(), "content/selection.json", "output/preview.pdf").unwrap();
        assert!(t.path().join("output/preview.harness.typ").is_file());
        assert!(!t.path().join("output/preview.pdf").exists());
        fs::remove_file(t.path().join("context/profile/example.md")).unwrap();
        assert!(prepare(t.path(), "content/selection.json", "output/second.pdf").is_err());
    }
    #[test]
    fn nested_preview_and_provenance_paths_are_portable() {
        let t = workspace();
        let prepared = prepare(
            t.path(),
            "content/selection.json",
            "output/previews/screening.pdf",
        )
        .unwrap();
        assert_eq!(prepared["typst"], "output/previews/screening.harness.typ");
        assert_eq!(
            prepared["content"],
            "output/previews/screening.harness.json"
        );
        assert_eq!(
            prepared["provenance"],
            "output/previews/screening.provenance.json"
        );
        let provenance: Value = serde_json::from_slice(
            &fs::read(t.path().join("output/previews/screening.provenance.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            provenance["artifacts"]["pdf"],
            "output/previews/screening.pdf"
        );
        for artifact in provenance["artifacts"].as_object().unwrap().values() {
            assert!(crate::workspace::portable_relative_path(
                artifact.as_str().unwrap()
            ));
        }
    }
}
