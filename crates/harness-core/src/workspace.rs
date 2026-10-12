//! Private workspace lifecycle and bundled generic instructions.
use crate::{HarnessError, Result};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const GUIDES: &[&str] = &[
    "career-operations.md",
    "job-harness-contract.md",
    "job-harness-onboarding.md",
    "host-observations.md",
    "workflows.md",
];
pub const SKILLS: &[&str] = &[
    "career-ops",
    "career-apply",
    "career-discover",
    "career-inbox-tidy",
    "career-interview-prep",
    "career-knowledge-update",
    "career-reconcile",
    "career-schedule",
    "application-email-review",
    "harness-maintain",
];
pub const DEFAULT_ANSWERS: &[&str] = &[
    "applications/approved-answers.md",
    "context/profile/application-decisions.md",
];

pub fn resource(alias: &str) -> Option<&'static str> {
    resource_text(alias)
}

pub fn resource_source_path(alias: &str) -> Option<PathBuf> {
    resource_text(alias)?;
    let relative = if let Some(name) = alias.strip_prefix("docs/") {
        format!("resources/guides/{name}")
    } else if let Some(name) = alias.strip_prefix(".agents/skills/") {
        format!("resources/skills/{name}")
    } else {
        let name = alias.strip_prefix("cv/")?;
        format!("resources/cv/{name}")
    };
    let root = PathBuf::from(std::env::var_os("JOB_HARNESS_SOURCE_ROOT")?)
        .canonicalize()
        .ok()?;
    let source = root.join(relative).canonicalize().ok()?;
    (source.starts_with(&root) && source.is_file()).then_some(source)
}

pub fn resource_text(alias: &str) -> Option<&'static str> {
    match alias {
        "docs/workflows.md" => Some(include_str!("../../../resources/guides/workflows.md")),
        "docs/host-observations.md" => Some(include_str!(
            "../../../resources/guides/host-observations.md"
        )),

        "docs/career-operations.md" => Some(include_str!(
            "../../../resources/guides/career-operations.md"
        )),
        "docs/job-harness-contract.md" => Some(include_str!(
            "../../../resources/guides/job-harness-contract.md"
        )),
        "docs/job-harness-onboarding.md" => Some(include_str!(
            "../../../resources/guides/job-harness-onboarding.md"
        )),
        ".agents/skills/career-ops/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-ops/SKILL.md"
        )),
        ".agents/skills/career-apply/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-apply/SKILL.md"
        )),
        ".agents/skills/career-discover/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-discover/SKILL.md"
        )),
        ".agents/skills/career-inbox-tidy/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-inbox-tidy/SKILL.md"
        )),
        ".agents/skills/career-interview-prep/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-interview-prep/SKILL.md"
        )),
        ".agents/skills/career-knowledge-update/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-knowledge-update/SKILL.md"
        )),
        ".agents/skills/career-reconcile/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-reconcile/SKILL.md"
        )),
        ".agents/skills/career-schedule/SKILL.md" => Some(include_str!(
            "../../../resources/skills/career-schedule/SKILL.md"
        )),
        ".agents/skills/application-email-review/SKILL.md" => Some(include_str!(
            "../../../resources/skills/application-email-review/SKILL.md"
        )),
        ".agents/skills/harness-maintain/SKILL.md" => Some(include_str!(
            "../../../resources/skills/harness-maintain/SKILL.md"
        )),
        "cv/schema.json" => Some(include_str!("../../../resources/cv/schema.json")),
        "cv/resume.typ" => Some(include_str!("../../../resources/cv/resume.typ")),
        _ => None,
    }
}

/// Names accepted by every supported native filesystem, without Win32 aliases.
/// The rule is applied on every OS so a backup remains portable when restored.
pub(crate) fn portable_component(name: &str) -> bool {
    if name.is_empty()
        || [".", ".."].contains(&name)
        || name.ends_with(['.', ' '])
        || name.len() > 255
        || name.encode_utf16().count() > 255
        || name
            .chars()
            .any(|ch| ch <= '\u{1f}' || "<>:\"/\\|?*".contains(ch))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$"].contains(&stem.as_str())
        && !["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
                    || ["¹", "²", "³"].contains(&suffix)
            })
        })
}

pub(crate) fn portable_relative_path(relative: &str) -> bool {
    !relative.is_empty()
        && relative.len() <= 4096
        && relative.split('/').count() <= 64
        && relative.split('/').all(portable_component)
        && Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

/// Windows can resolve another spelling (including an 8.3 alias) to the same
/// file. Wire paths retain one exact identity rather than silently aliasing it.
pub(crate) fn exact_component(parent: &Path, name: &std::ffi::OsStr) -> Result<()> {
    #[cfg(windows)]
    if parent.is_dir() {
        use unicode_casefold::UnicodeCaseFold;
        use unicode_normalization::UnicodeNormalization;
        let text = name
            .to_str()
            .ok_or_else(|| HarnessError::validation("Workspace paths must be UTF-8"))?;
        let key = text.nfc().case_fold().collect::<String>();
        for entry in fs::read_dir(parent)? {
            let actual = entry?.file_name();
            if actual
                .to_str()
                .is_some_and(|actual| actual.nfc().case_fold().collect::<String>() == key)
                && actual != name
            {
                return Err(HarnessError::new(
                    "path_denied",
                    "Use the exact stored case and Unicode spelling of each workspace path",
                ));
            }
        }
        let child = parent.join(name);
        if child.exists() && child.canonicalize()?.file_name() != Some(name) {
            return Err(HarnessError::new(
                "path_denied",
                "Workspace data refuses short-name or other filesystem aliases",
            ));
        }
    }
    #[cfg(not(windows))]
    let _ = (parent, name);
    Ok(())
}

/// Reject aliases, traversal and symlink components; no path can select a second workspace.
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if !portable_relative_path(relative) {
        return Err(HarnessError::validation(
            "Expected a portable workspace-relative path",
        ));
    }
    let path = Path::new(relative);
    let mut candidate = fs::canonicalize(root)?;
    for part in path.components() {
        exact_component(&candidate, part.as_os_str())?;
        candidate.push(part);
        match fs::symlink_metadata(&candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(HarnessError::validation(
                    "Workspace data must not follow symlinks",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(candidate)
}

/// Resolve a possibly new path while detecting symlink ancestors.
pub fn absolute_path(path: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    if !path.is_absolute()
        && path
            .components()
            .any(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
    {
        return Err(HarnessError::validation(
            "Use an absolute Windows path or an ordinary relative path, not a drive-relative/root-relative alias",
        ));
    }
    if path.components().any(|part| matches!(part, Component::Normal(name) if name.to_str().is_none_or(|name| !portable_component(name)))) {
        return Err(HarnessError::validation("Workspace destination uses a nonportable path component"));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if absolute
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(HarnessError::validation(
            "Use a normalized path without parent traversal",
        ));
    }
    if fs::symlink_metadata(&absolute).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(HarnessError::validation(
            "Workspace destination must not be a symlink",
        ));
    }
    let mut tail = Vec::new();
    let mut existing = absolute.as_path();
    while !existing.exists() {
        let part = existing
            .file_name()
            .ok_or_else(|| HarnessError::validation("Invalid path"))?;
        tail.push(part.to_owned());
        existing = existing
            .parent()
            .ok_or_else(|| HarnessError::validation("Path has no parent"))?;
    }
    let mut result = fs::canonicalize(existing)?;
    for part in tail.iter().rev() {
        result.push(part);
    }
    Ok(result)
}

pub fn assert_external_workspace(root: &Path) -> Result<()> {
    for parent in root.ancestors() {
        let rust_checkout = parent.join("Cargo.toml").is_file()
            && parent.join("resources/schemas/tools.json").is_file();
        let python_checkout = parent.join("pyproject.toml").is_file()
            && parent.join("job_harness/service.py").is_file();
        if rust_checkout || python_checkout {
            return Err(HarnessError::validation(
                "Private workspaces must stay outside the Job Harness code checkout",
            ));
        }
    }
    Ok(())
}

pub fn settings(root: &Path) -> Result<Value> {
    let path = safe_path(root, "config/career-ledger.json")?;
    let provided: Value = serde_json::from_slice(&fs::read(path)?)?;
    validate_settings(&provided)?;
    let mut value = json!({"schema_version":1,"timezone":"UTC","primary_cv":null,"preferred_browser":null,"answer_sources":DEFAULT_ANSWERS});
    for (key, item) in provided.as_object().expect("validated object") {
        value[key] = item.clone();
    }
    Ok(value)
}

pub fn validate_settings(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| HarnessError::validation("Workspace configuration must be an object"))?;
    if object.keys().any(|k| {
        ![
            "schema_version",
            "timezone",
            "primary_cv",
            "preferred_browser",
            "answer_sources",
        ]
        .contains(&k.as_str())
    }) || value["schema_version"].as_u64() != Some(1)
    {
        return Err(HarnessError::validation(
            "Unsupported workspace configuration",
        ));
    }
    if let Some(zone) = object.get("timezone") {
        let s = zone
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| HarnessError::validation("Configure an IANA timezone"))?;
        if s.parse::<chrono_tz::Tz>().is_err() {
            return Err(HarnessError::validation("Configure a valid IANA timezone"));
        }
    }
    for key in ["primary_cv", "preferred_browser"] {
        if let Some(v) = object.get(key)
            && !v.is_null()
            && v.as_str()
                .is_none_or(|s| s.trim().is_empty() || s.chars().count() > 20000)
        {
            return Err(HarnessError::validation(format!("Invalid {key}")));
        }
    }
    if let Some(cv) = value["primary_cv"].as_str() {
        validate_relative_data(cv, &["output", "applications", "content"], Some("pdf"))?;
    }
    if let Some(answers) = object.get("answer_sources") {
        let answers = answers
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 20)
            .ok_or_else(|| HarnessError::validation("Configure 1–20 answer sources"))?;
        for source in answers {
            validate_relative_data(
                source
                    .as_str()
                    .ok_or_else(|| HarnessError::validation("Answer source must be text path"))?,
                &["applications", "context"],
                Some("md"),
            )?;
        }
    }
    Ok(())
}

pub fn validate_relative_data(path: &str, roots: &[&str], extension: Option<&str>) -> Result<()> {
    if !portable_relative_path(path)
        || !roots.contains(&path.split('/').next().unwrap_or(""))
        || path.split('/').count() < 2
    {
        return Err(HarnessError::validation(
            "Configured data path is outside permitted portable workspace directories",
        ));
    }
    if let Some(ext) = extension
        && Path::new(path).extension().and_then(|e| e.to_str()) != Some(ext)
    {
        return Err(HarnessError::validation(
            "Configured file has an unsupported extension",
        ));
    }
    Ok(())
}

pub fn validate(root: &Path) -> Result<Value> {
    let settings = settings(root)?;
    let mut missing = Vec::new();
    for source in settings["answer_sources"].as_array().expect("validated") {
        if !safe_path(root, source.as_str().expect("validated"))?.is_file() {
            missing.push(source.clone());
        }
    }
    let cv_present = match settings["primary_cv"].as_str() {
        Some(cv) => safe_path(root, cv)?.is_file(),
        None => false,
    };
    Ok(
        json!({"settings":settings,"missing_answer_sources":missing,"primary_cv_present":cv_present,"configured":cv_present && missing.is_empty()}),
    )
}

pub(crate) fn populate(root: &Path) -> Result<()> {
    for name in [
        "applications/evidence",
        "applications/briefs",
        "applications/operations/packets",
        "applications/operations/receipts",
        "applications/operations/conflicts",
        "context/profile",
        "content",
        "output",
        "config",
        "docs",
    ] {
        fs::create_dir_all(root.join(name))?;
    }
    let files = [
        (
            "AGENTS.md",
            "# Candidate workspace\n\nRead approved answers and attributed evidence before acting. Do not invent facts. User instructions control external scope. Record verified outcomes; never replay a submitting or unknown attempt. Credentials belong in the host.\n",
        ),
        (
            "README.md",
            "# Private career workspace\n\nSet config/career-ledger.json, provide your primary CV and attributed answers, then call Job Harness bootstrap. Keep this personal workspace separate from the toolkit.\n",
        ),
        (
            "applications/approved-answers.md",
            "# Approved answers\n\nRecord direct answers with date, attribution, allowed scope and unresolved limits. No personal answer is assumed.\n",
        ),
        (
            "context/profile/application-decisions.md",
            "# Application decisions\n\nRecord preferences with attribution and limits. Chat memory is not canonical.\n",
        ),
        (
            "context/README.md",
            "# Career evidence\n\nKeep firsthand facts, feedback, coaching and hypotheses distinct. Cite source and date; preserve unknowns.\n",
        ),
        (
            "docs/project.md",
            "# Career workspace\n\nOne canonical private source of facts, applications and attributable outcomes.\n",
        ),
        (
            "docs/workflow.md",
            "# Evidence workflow\n\nFacts live under context; vacancies and outcomes under applications. CV changes need candidate authority.\n\n## After an interview\nSave and link a dated note, keeping recollection and coaching separate from facts.\n",
        ),
        (
            "docs/application-pipeline.md",
            "# Tracking and preparation\n\nEdit canonical vacancy records, then rebuild the pipeline. Keep proposed, booked, completed and unknown separate. Briefs are coaching, not career facts.\n",
        ),
        (
            "applications/briefs/template.md",
            "# Company brief\n\nBusiness, exact vacancy, sourced personal stories, questions and unresolved facts.\n",
        ),
        (
            ".gitignore",
            "applications/operations/.lock\napplications/operations/.career-tmp-*\n.scratch/\n.harness/index.sqlite*\n",
        ),
    ];
    for (relative, text) in files {
        fs::write(root.join(relative), text)?;
    }
    fs::write(
        root.join("applications/operations/state.json"),
        job_harness_domain::canonical::encoded(
            &json!({"schema_version":1,"runs":{},"attempts":{},"events":[]}),
        ),
    )?;
    fs::write(
        root.join("config/career-workflows.json"),
        include_str!("../../../resources/workflows.json"),
    )?;
    fs::write(
        root.join("config/career-ledger.json"),
        serde_json::to_vec_pretty(
            &json!({"schema_version":1,"timezone":"UTC","primary_cv":null,"preferred_browser":null,"answer_sources":DEFAULT_ANSWERS}),
        )?,
    )?;
    Ok(())
}

pub fn initialize(root: &Path) -> Result<Value> {
    let root = absolute_path(root)?;
    assert_external_workspace(&root)?;
    if fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink())
        || (root.exists() && (!root.is_dir() || fs::read_dir(&root)?.next().is_some()))
    {
        return Err(HarnessError::validation(
            "Initialize only a new or empty workspace",
        ));
    }
    let parent = root
        .parent()
        .ok_or_else(|| HarnessError::validation("Workspace needs a parent"))?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".job-harness-init-")
        .tempdir_in(parent)?;
    populate(stage.path())?;
    crate::pipeline::write(stage.path())?;
    private_tree(stage.path())?;
    publish_directory(stage.path(), &root)?;
    Ok(
        json!({"workspace":root,"ledger":root,"configured":false,"next":["Set timezone and relative primary_cv path in config/career-ledger.json.","Supply your CV and attributed facts and answers.","Connect host capabilities and call bootstrap."]}),
    )
}

pub(crate) fn private_tree(root: &Path) -> Result<()> {
    let mut paths = walkdir::WalkDir::new(root)
        .into_iter()
        .map(|e| {
            e.map(|e| e.path().to_owned())
                .map_err(|e| HarnessError::validation(e.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    // Files and permissions reach durable storage before children/parents are published.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in &paths {
            fs::set_permissions(
                path,
                fs::Permissions::from_mode(if path.is_dir() { 0o700 } else { 0o600 }),
            )?;
        }
    }
    paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in paths {
        if path.is_file() {
            // FlushFileBuffers on Windows requires a writable file handle.
            fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
        } else {
            #[cfg(unix)]
            fs::File::open(path)?.sync_all()?;
        }
    }
    Ok(())
}

pub(crate) fn publish_directory(stage: &Path, target: &Path) -> Result<()> {
    // Only rename into a missing/empty target; never merge content.
    if target.exists() {
        if !target.is_dir() || fs::read_dir(target)?.next().is_some() {
            return Err(HarnessError::validation(
                "Destination changed or is no longer empty",
            ));
        }
        fs::remove_dir(target)?;
    }
    crate::storage::publish_rename(stage, target, false)?;
    #[cfg(unix)]
    fs::File::open(target.parent().expect("parent"))?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn onboarding_has_no_candidate_defaults() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("data");
        initialize(&root).unwrap();
        assert_eq!(settings(&root).unwrap()["primary_cv"], Value::Null);
        assert!(validate(&root).unwrap()["configured"] == false);
        assert!(initialize(&root).is_err());
    }
    #[test]
    fn traversal_and_bad_config_are_refused() {
        let t = tempfile::tempdir().unwrap();
        assert!(safe_path(t.path(), "context/../secret").is_err());
        assert!(safe_path(t.path(), "/etc/passwd").is_err());
        assert!(validate_settings(&json!({"schema_version":1,"primary_cv":"../cv.pdf"})).is_err());
        assert!(validate_settings(&json!({"schema_version":1,"answer_sources":[17]})).is_err());
    }
    #[test]
    fn generic_resources_are_embedded() {
        assert!(resource_text(".agents/skills/career-apply/SKILL.md").is_some());
        assert!(resource_text("context/private.md").is_none());
    }
    #[test]
    fn portable_data_paths_reject_win32_devices_and_alias_syntax_on_every_os() {
        let root = tempfile::tempdir().unwrap();
        for path in [
            "context/CON.md",
            "context/con.tar.gz",
            "context/PRN",
            "context/AUX.json",
            "context/NUL.txt",
            "context/COM1.md",
            "context/com9.log",
            "context/LPT1.pdf",
            "context/lpt9",
            "context/COM¹.md",
            "context/LPT².json",
            "context/CONIN$.md",
            "context/trailing.md.",
            "context/trailing.md ",
            "context/inner. /note.md",
            "context/tab\t.md",
            "context/line\n.md",
            "context/file:stream.md",
            "context/name?.md",
            "context/name*.md",
            "C:/context/note.md",
            "C:context/note.md",
            r"C:\context\note.md",
            r"\\server\share\note.md",
            "//server/share/note.md",
            "context//note.md",
            "context/./note.md",
            "context/../note.md",
        ] {
            assert!(!portable_relative_path(path), "{path:?}");
            assert!(safe_path(root.path(), path).is_err(), "{path:?}");
            assert!(
                validate_relative_data(path, &["context"], None).is_err(),
                "{path:?}"
            );
            assert!(
                crate::storage::safe_path(root.path(), path, &["context"], &[".md"]).is_err(),
                "{path:?}"
            );
        }
        for path in [
            "context/console.md",
            "context/com0.md",
            "context/com10.md",
            "context/lpt10.md",
            "context/My Notes/café.md",
            "context/日本語.md",
            "context/.hidden-note.md",
        ] {
            assert!(portable_relative_path(path), "{path}");
            assert!(safe_path(root.path(), path).is_ok(), "{path}");
        }
    }
    #[test]
    fn portable_component_and_depth_bounds_are_explicit() {
        assert!(!portable_component(&"a".repeat(256)));
        assert!(!portable_relative_path(&format!(
            "context/{}.md",
            "a/".repeat(64)
        )));
        assert!(portable_relative_path(
            "context/very-deep-but-portable/note.md"
        ));
    }
    #[cfg(windows)]
    #[test]
    fn windows_native_roots_work_while_drive_relative_and_case_aliases_fail() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("Career Workspace");
        initialize(&root).unwrap();
        assert!(absolute_path(&root).unwrap().is_absolute());
        assert!(absolute_path(Path::new(r"C:Career")).is_err());
        assert!(absolute_path(Path::new(r"\Career")).is_err());
        fs::write(root.join("context/Profile.md"), b"Original").unwrap();
        assert!(safe_path(&root, "context/Profile.md").is_ok());
        assert!(safe_path(&root, "context/profile.md").is_err());
        assert!(
            crate::storage::safe_path(&root, "context/profile.md", &["context"], &[".md"]).is_err()
        );
        assert!(safe_path(&root, "Context/Profile.md").is_err());
    }
}
