//! Version-1 private .jhb ZIP bundles. Software, credentials and derived indexes never travel here.
use crate::workspace::{absolute_path, assert_external_workspace, safe_path};
use crate::{HarnessError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const FORMAT: &str = "job-harness.workspace";
pub const MAX_FILES: usize = 10_000;
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const DATA_DIRS: &[&str] = &[
    "applications",
    "context",
    "content",
    "config",
    "sources",
    "output",
    "fonts",
    "docs",
];
const DATA_FILES: &[&str] = &["AGENTS.md", "README.md", "resume.typ"];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BundleKind {
    Workspace,
    Profile,
}
impl BundleKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "workspace" => Ok(Self::Workspace),
            "profile" => Ok(Self::Profile),
            _ => Err(HarnessError::validation(
                "Bundle kind must be workspace or profile",
            )),
        }
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    schema_version: u32,
    kind: BundleKind,
    created_at: String,
    files: Vec<FileEntry>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    path: String,
    size: u64,
    sha256: String,
}
fn failure(message: impl Into<String>) -> HarnessError {
    HarnessError::validation(message)
}
fn zip_error(error: impl std::fmt::Display) -> HarnessError {
    failure(format!("Invalid private bundle: {error}"))
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn folded(name: &str) -> String {
    name.nfc().case_fold().collect::<String>()
}

/// Detect aliases in directory prefixes too: A/one.md and a/two.md must not
/// merge into one directory after transfer to a case-insensitive filesystem.
fn register_aliases(seen: &mut BTreeMap<String, String>, name: &str) -> Result<()> {
    let mut prefix = String::new();
    for part in name.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let key = folded(&prefix);
        if seen.get(&key).is_some_and(|existing| existing != &prefix) {
            return Err(failure(
                "Bundle directory or file paths collide across case or Unicode normalization",
            ));
        }
        seen.insert(key, prefix.clone());
    }
    Ok(())
}

pub fn allowed_path(name: &str) -> bool {
    if !crate::workspace::portable_relative_path(name) {
        return false;
    }
    let parts = name.split('/').collect::<Vec<_>>();
    if parts.iter().any(|p| {
        p.is_empty()
            || *p == "."
            || *p == ".."
            || [
                ".git",
                ".scratch",
                ".venv",
                "node_modules",
                "__pycache__",
                ".harness",
            ]
            .contains(&p.to_lowercase().as_str())
            || p.to_lowercase().starts_with(".career-tmp-")
            || p.to_lowercase().starts_with(".job-harness-")
    }) {
        return false;
    }
    let base = parts.last().unwrap().to_lowercase();
    if [
        ".lock",
        ".ds_store",
        "credentials.json",
        "credentials",
        "cookies.json",
        "session.json",
        ".netrc",
        "cookies",
        "login data",
        "logins.json",
        "key4.db",
        "cookies.sqlite",
        "cookies-journal",
        "login data-journal",
        "local state",
        "web data",
        "storage-state.json",
        "storage_state.json",
    ]
    .contains(&base.as_str())
        || base.contains(".sqlite")
        || base.starts_with(".env")
        || base.starts_with("id_")
        || [".key", ".pem", ".p12", ".jhb", ".har", ".kdbx"]
            .iter()
            .any(|e| base.ends_with(e))
    {
        return false;
    }
    if crate::workspace::GUIDES
        .iter()
        .any(|g| name == format!("docs/{g}"))
    {
        return false;
    }
    DATA_FILES.contains(&name) || (parts.len() > 1 && DATA_DIRS.contains(&parts[0]))
}

fn configured_paths(settings: &Value) -> BTreeSet<String> {
    let mut paths = BTreeSet::from(["config/career-ledger.json".into()]);
    let defaults = json!(crate::workspace::DEFAULT_ANSWERS);
    for value in settings
        .get("answer_sources")
        .unwrap_or(&defaults)
        .as_array()
        .expect("validated config")
    {
        paths.insert(value.as_str().unwrap().to_owned());
    }
    if let Some(cv) = settings["primary_cv"].as_str() {
        paths.insert(cv.to_owned());
    }
    paths
}
fn profile_paths(settings: &Value) -> BTreeSet<String> {
    let mut paths = configured_paths(settings);
    paths.extend(["config/career-workflows.json", "README.md", "AGENTS.md"].map(str::to_owned));
    paths
}
fn profile_allowed(name: &str, configured: &BTreeSet<String>) -> bool {
    !name.starts_with("applications/operations/")
        && (name.starts_with("context/")
            || name.starts_with("content/")
            || configured.contains(name))
}

fn check_portable_state(root: &Path) -> Result<()> {
    let state = crate::state::load(root)?;
    if state
        .get("pending")
        .is_some_and(|v| !v.is_null() && v != &json!({}) && v != &json!([]))
    {
        return Err(failure(
            "Workspace has an interrupted journal; recover the source before transfer",
        ));
    }
    // Begun and unknown attempts deliberately survive; only unfinished local journal work blocks copying.
    crate::pipeline::build(root)?;
    Ok(())
}

pub fn export(root: &Path, destination: &Path, kind: BundleKind) -> Result<Value> {
    export_bundle(
        root,
        destination,
        match kind {
            BundleKind::Workspace => "workspace",
            BundleKind::Profile => "profile",
        },
    )
}
pub fn export_bundle(root: &Path, destination: &Path, kind: &str) -> Result<Value> {
    export_bundle_with_options(root, destination, kind, false)
}

pub fn export_bundle_with_options(
    root: &Path,
    destination: &Path,
    kind: &str,
    allow_drop_history: bool,
) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let destination = absolute_path(destination)?;
    let kind = BundleKind::parse(kind)?;
    assert_external_workspace(&destination)?;
    if destination.starts_with(&root) {
        return Err(failure("Write private bundles outside the workspace"));
    }
    if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
        return Err(failure("Existing bundle filenames are never replaced"));
    }
    if !root.join("config/career-ledger.json").is_file() {
        return Err(failure("Choose an initialized private workspace"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| failure("Bundle needs a parent directory"))?;
    fs::create_dir_all(parent)?;
    let _guard = crate::storage::WorkspaceLock::acquire(&root, false)?;
    let settings = crate::workspace::settings(&root)?;
    check_portable_state(&root)?;
    if kind == BundleKind::Profile {
        let state = crate::state::load(&root)?;
        let unresolved = state["attempts"]
            .as_object()
            .expect("validated state")
            .iter()
            .filter(|(_, a)| {
                ["reserved", "submitting", "outcome_unknown"]
                    .contains(&a["status"].as_str().unwrap_or(""))
            })
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            return Err(HarnessError::new(
                "unresolved_attempts",
                format!(
                    "A profile transfer would omit unresolved attempts ({}). Export a full workspace or resolve the real outcomes first.",
                    unresolved.join(", ")
                ),
            ));
        }
        let history = !state["attempts"]
            .as_object()
            .expect("validated state")
            .is_empty()
            || crate::pipeline::build(&root)?.records.iter().any(|record| {
                !["saved", "prepared"].contains(&record["stage"].as_str().unwrap_or(""))
            });
        if history && !allow_drop_history {
            return Err(HarnessError::new(
                "history_acknowledgement_required",
                "Profile bundles omit application history. For the same person moving machines, use a full workspace. Deliberate profile-only transfer requires --drop-history.",
            ));
        }
    }
    let configured = profile_paths(&settings);
    let mut paths: Vec<(PathBuf, String)> = Vec::new();
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    let walker = walkdir::WalkDir::new(&root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let relative = e.path().strip_prefix(&root).unwrap_or(e.path());
            let parts = relative
                .components()
                .filter_map(|p| p.as_os_str().to_str())
                .collect::<Vec<_>>();
            !parts.iter().any(|p| {
                [
                    ".git",
                    ".scratch",
                    ".venv",
                    "node_modules",
                    "__pycache__",
                    ".harness",
                ]
                .contains(&p.to_lowercase().as_str())
                    || p.starts_with(".career-tmp-")
                    || p.starts_with(".job-harness-")
            }) && (e.depth() != 1 || !e.file_type().is_dir() || DATA_DIRS.contains(&parts[0]))
        });
    for entry in walker {
        let entry = entry.map_err(|e| failure(e.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        let name = crate::storage::portable_relative(&root, entry.path())
            .map_err(|_| failure("Workspace data uses a nonportable bundle path"))?;
        if DATA_DIRS.contains(&name.split('/').next().unwrap_or(""))
            && !crate::workspace::portable_relative_path(&name)
        {
            return Err(failure(format!(
                "Workspace data uses a nonportable bundle path: {name}"
            )));
        }
        if entry.file_type().is_symlink() {
            if allowed_path(&name) || DATA_DIRS.contains(&name.as_str()) {
                return Err(failure(format!("Bundles refuse symlinked data: {name}")));
            }
            continue;
        }
        if !entry.file_type().is_file()
            || !allowed_path(&name)
            || (kind == BundleKind::Profile && !profile_allowed(&name, &configured))
        {
            continue;
        }
        let size = entry.metadata().map_err(|e| failure(e.to_string()))?.len();
        total = total
            .checked_add(size)
            .ok_or_else(|| failure("Bundle size overflow"))?;
        if size > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES || paths.len() >= MAX_FILES {
            return Err(failure("Bundle exceeds file or size limits"));
        }
        if !names.insert(folded(&name)) {
            return Err(failure(
                "Bundle paths collide across case or Unicode normalization",
            ));
        }
        register_aliases(&mut aliases, &name)?;
        paths.push((entry.path().to_owned(), name));
    }
    let included = paths
        .iter()
        .map(|(_, name)| name.clone())
        .collect::<BTreeSet<_>>();
    let missing = configured_paths(&settings)
        .difference(&included)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(failure(format!(
            "Configured files cannot be bundled: {}",
            missing.join(", ")
        )));
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".job-harness-bundle-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut manifest = Manifest {
        format: FORMAT.into(),
        schema_version: 1,
        kind,
        created_at: chrono::Utc::now().to_rfc3339(),
        files: vec![],
    };
    let mut extensions = BTreeMap::<String, usize>::new();
    {
        let mut archive = ZipWriter::new(temporary.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        let mut read_total = 0u64;
        for (_, name) in &paths {
            let path = safe_path(&root, name)?;
            let mut options_read = fs::OpenOptions::new();
            options_read.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options_read.custom_flags(libc::O_NOFOLLOW);
            }
            let mut file = options_read.open(path)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err(failure("Bundle source changed type"));
            }
            let mut data = Vec::new();
            Read::by_ref(&mut file)
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut data)?;
            read_total += data.len() as u64;
            if data.len() as u64 > MAX_FILE_BYTES || read_total > MAX_TOTAL_BYTES {
                return Err(failure("Bundle source changed beyond size limits"));
            }
            archive.start_file(name, options).map_err(zip_error)?;
            archive.write_all(&data)?;
            manifest.files.push(FileEntry {
                path: name.clone(),
                size: data.len() as u64,
                sha256: digest(&data),
            });
            let extension = Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_else(|| "no extension".into());
            *extensions.entry(extension).or_default() += 1;
        }
        let bytes = serde_json::to_vec(&manifest)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES
            || read_total + bytes.len() as u64 > MAX_TOTAL_BYTES
        {
            return Err(failure(
                "Bundle manifest or total expansion exceeds its size limit",
            ));
        }
        archive
            .start_file("manifest.json", options)
            .map_err(zip_error)?;
        archive.write_all(&bytes)?;
        archive.finish().map_err(zip_error)?;
    }
    temporary.as_file().sync_all()?;
    // Hard-link publication provides no-overwrite even if another writer wins the filename.
    fs::hard_link(temporary.path(), &destination).map_err(|e| {
        failure(format!(
            "Safe no-overwrite publication failed; choose a local filesystem: {e}"
        ))
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(
        json!({"bundle":destination,"kind":kind,"files":paths.len(),"extensions":extensions,"encrypted":false,"history_dropped":kind==BundleKind::Profile && allow_drop_history,"note":"Private plaintext ZIP. Encrypt separately before sharing. No provider sessions are included."}),
    )
}

/// The ZIP reader deduplicates exact names internally. Check raw central entries first.
/// Export limits never require ZIP64 or multi-disk archives; ordinary Python v1 ZIPs fit.
fn preflight(source: &Path) -> Result<usize> {
    let mut input = fs::File::open(source)?;
    let length = input.metadata()?.len();
    if !(22..=MAX_TOTAL_BYTES + 64 * 1024 * 1024).contains(&length) {
        return Err(failure(
            "Bundle physical size exceeds its limit or is invalid",
        ));
    }
    let tail_length = length.min(65_535 + 22) as usize;
    input.seek(SeekFrom::End(-(tail_length as i64)))?;
    let mut tail = vec![0; tail_length];
    input.read_exact(&mut tail)?;
    let end = (0..=tail.len() - 22)
        .rev()
        .find(|i| {
            tail[*i..*i + 4] == [0x50, 0x4b, 0x05, 0x06]
                && *i + 22 + u16::from_le_bytes([tail[*i + 20], tail[*i + 21]]) as usize
                    == tail.len()
        })
        .ok_or_else(|| failure("Bundle lacks a valid ZIP end record"))?;
    let end_record = &tail[end..];
    let small = |offset: usize| u16::from_le_bytes([end_record[offset], end_record[offset + 1]]);
    let large = |offset: usize| {
        u32::from_le_bytes(
            end_record[offset..offset + 4]
                .try_into()
                .expect("fixed ZIP header"),
        )
    };
    let count = small(10) as usize;
    let dir_size = large(12) as u64;
    let dir_offset = large(16) as u64;
    let end_position = length - tail_length as u64 + end as u64;
    if small(4) != 0
        || small(6) != 0
        || small(8) != small(10)
        || count > MAX_FILES + 1
        || count == 65_535
        || dir_offset == u32::MAX as u64
        || dir_size == u32::MAX as u64
        || dir_offset.checked_add(dir_size) != Some(end_position)
    {
        return Err(failure(
            "Bundle uses unsupported multi-disk/ZIP64 layout or invalid directory limits",
        ));
    }
    input.seek(SeekFrom::Start(dir_offset))?;
    let mut names = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    let mut total = 0u64;
    for _ in 0..count {
        if input
            .stream_position()?
            .checked_add(46)
            .is_none_or(|end| end > end_position)
        {
            return Err(failure("Truncated ZIP central directory"));
        }
        let mut header = [0; 46];
        input.read_exact(&mut header)?;
        if header[..4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(failure("Invalid ZIP central directory record"));
        }
        let small = |offset: usize| u16::from_le_bytes([header[offset], header[offset + 1]]);
        let large = |offset: usize| {
            u32::from_le_bytes(
                header[offset..offset + 4]
                    .try_into()
                    .expect("fixed ZIP header"),
            )
        };
        let name_length = small(28) as usize;
        let extra = small(30) as u64;
        let comment = small(32) as u64;
        if name_length == 0
            || input
                .stream_position()?
                .checked_add(name_length as u64 + extra + comment)
                .is_none_or(|end| end > end_position)
        {
            return Err(failure("Invalid ZIP path or central directory length"));
        }
        let mut name = vec![0; name_length];
        input.read_exact(&mut name)?;
        let name =
            std::str::from_utf8(&name).map_err(|_| failure("Bundle paths must use UTF-8"))?;
        if !names.insert(folded(name)) {
            return Err(failure("Duplicate bundle paths in the raw ZIP directory"));
        }
        register_aliases(&mut aliases, name)?;
        if small(8) & 1 != 0 {
            return Err(failure(
                "Decrypt private bundle outside the harness before import",
            ));
        }
        let size = large(24) as u64;
        total = total
            .checked_add(size)
            .ok_or_else(|| failure("Bundle expansion overflow"))?;
        if (name == "manifest.json" && size > MAX_MANIFEST_BYTES)
            || (name != "manifest.json" && (!allowed_path(name) || size > MAX_FILE_BYTES))
            || total > MAX_TOTAL_BYTES
        {
            return Err(failure(
                "Bundle has unsafe paths or exceeds expansion limits",
            ));
        }
        let mode = (large(38) >> 16) & 0o170000;
        if mode != 0 && mode != 0o100000 {
            return Err(failure("Bundle entries must be regular files"));
        }
        input.seek(SeekFrom::Current((extra + comment) as i64))?;
    }
    if input.stream_position()? != end_position {
        return Err(failure(
            "Bundle directory count or size does not match its end record",
        ));
    }
    if !names.contains("manifest.json") {
        return Err(failure("Bundle lacks a unique manifest"));
    }
    Ok(count)
}

fn inspect_archive(archive: &mut ZipArchive<fs::File>) -> Result<Manifest> {
    if archive.len() > MAX_FILES + 1 {
        return Err(failure("Bundle exceeds file count limit"));
    }
    let mut names = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    let mut exact = BTreeSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(zip_error)?;
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| failure("Bundle expansion overflow"))?;
        if !names.insert(folded(entry.name())) || !exact.insert(entry.name().to_owned()) {
            return Err(failure("Duplicate bundle paths"));
        }
        register_aliases(&mut aliases, entry.name())?;
        if entry.encrypted() {
            return Err(failure(
                "Decrypt private bundle outside the harness before import",
            ));
        }
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
        {
            return Err(failure("Bundle contains symlink or directory entries"));
        }
        if entry.name() != "manifest.json"
            && (!allowed_path(entry.name()) || entry.size() > MAX_FILE_BYTES)
        {
            return Err(failure("Bundle contains unsafe or oversized paths"));
        }
    }
    if total > MAX_TOTAL_BYTES || !exact.contains("manifest.json") {
        return Err(failure("Bundle exceeds expansion limit or lacks manifest"));
    }
    let mut entry = archive.by_name("manifest.json").map_err(zip_error)?;
    if entry.size() > MAX_MANIFEST_BYTES {
        return Err(failure("Manifest too large"));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut entry)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    drop(entry);
    if manifest.format != FORMAT || manifest.schema_version != 1 || manifest.files.len() > MAX_FILES
    {
        return Err(failure("Unsupported bundle manifest version"));
    }
    let mut declared = BTreeSet::new();
    for file in &manifest.files {
        if !allowed_path(&file.path)
            || !declared.insert(file.path.clone())
            || file.size > MAX_FILE_BYTES
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(failure("Invalid bundle file manifest"));
        }
        let entry = archive.by_name(&file.path).map_err(zip_error)?;
        if entry.size() != file.size {
            return Err(failure("Bundle size differs from manifest"));
        }
    }
    declared.insert("manifest.json".into());
    if declared != exact || !declared.contains("config/career-ledger.json") {
        return Err(failure(
            "Bundle contains undeclared files or missing configuration",
        ));
    }
    Ok(manifest)
}

fn read_verified(archive: &mut ZipArchive<fs::File>, entry: &FileEntry) -> Result<Vec<u8>> {
    let file = archive.by_name(&entry.path).map_err(zip_error)?;
    let mut data = Vec::new();
    file.take(entry.size + 1).read_to_end(&mut data)?;
    if data.len() as u64 != entry.size || digest(&data) != entry.sha256 {
        return Err(failure(format!("Bundle digest mismatch: {}", entry.path)));
    }
    Ok(data)
}
pub fn import(source: &Path, destination: &Path) -> Result<Value> {
    import_bundle(destination, source)
}
pub fn import_bundle(root: &Path, source: &Path) -> Result<Value> {
    let root = absolute_path(root)?;
    assert_external_workspace(&root)?;
    if root.exists() && (!root.is_dir() || fs::read_dir(&root)?.next().is_some()) {
        return Err(failure(
            "Import only into a new or empty workspace; no merging",
        ));
    }
    let parent = root
        .parent()
        .ok_or_else(|| failure("Workspace needs a parent"))?;
    fs::create_dir_all(parent)?;
    // Snapshot incoming bytes once so the raw directory check and ZIP reader see the same archive.
    let mut snapshot = tempfile::Builder::new()
        .prefix(".job-harness-import-archive-")
        .tempfile_in(parent)?;
    let input = fs::File::open(source)?;
    let maximum = MAX_TOTAL_BYTES + 64 * 1024 * 1024;
    if input.metadata()?.len() > maximum {
        return Err(failure("Bundle physical size exceeds its limit"));
    }
    let copied = std::io::copy(&mut input.take(maximum + 1), snapshot.as_file_mut())?;
    if copied > maximum {
        return Err(failure(
            "Bundle changed beyond its size limit while reading",
        ));
    }
    snapshot.as_file().sync_all()?;
    let central_count = preflight(snapshot.path())?;
    let mut archive = ZipArchive::new(fs::File::open(snapshot.path())?).map_err(zip_error)?;
    if archive.len() != central_count {
        return Err(failure("Duplicate or hidden ZIP directory entries"));
    }
    let manifest = inspect_archive(&mut archive)?;
    let config_entry = manifest
        .files
        .iter()
        .find(|e| e.path == "config/career-ledger.json")
        .unwrap();
    let config: Value = serde_json::from_slice(&read_verified(&mut archive, config_entry)?)?;
    crate::workspace::validate_settings(&config)?;
    let included = manifest
        .files
        .iter()
        .map(|e| e.path.clone())
        .collect::<BTreeSet<_>>();
    if !configured_paths(&config).is_subset(&included) {
        return Err(failure("Bundle lacks configured answer sources or CV"));
    }
    if manifest.kind == BundleKind::Profile {
        let configured = profile_paths(&config);
        if manifest
            .files
            .iter()
            .any(|e| !profile_allowed(&e.path, &configured))
        {
            return Err(failure("Profile bundle contains application history"));
        }
    }
    let stage = tempfile::Builder::new()
        .prefix(".job-harness-import-")
        .tempdir_in(parent)?;
    if manifest.kind == BundleKind::Profile {
        crate::workspace::populate(stage.path())?;
    }
    for entry in &manifest.files {
        let data = read_verified(&mut archive, entry)?;
        let path = safe_path(stage.path(), &entry.path)?;
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, data)?;
    }
    fs::create_dir_all(stage.path().join("applications"))?;
    crate::workspace::settings(stage.path())?;
    check_portable_state(stage.path())?;
    crate::pipeline::write(stage.path())?;
    crate::workspace::private_tree(stage.path())?;
    crate::workspace::publish_directory(stage.path(), &root)?;
    Ok(
        json!({"workspace":root,"kind":manifest.kind,"files":manifest.files.len(),"note":"Imported locally. Provider sessions remain host-owned; no external action was executed."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        crate::workspace::populate(t.path()).unwrap();
        t
    }
    #[test]
    fn nested_archive_names_and_manifest_paths_are_portable_and_restore_exact_bytes() {
        let root = workspace();
        let native = root
            .path()
            .join("context")
            .join("projects")
            .join("nested")
            .join("evidence.md");
        fs::create_dir_all(native.parent().unwrap()).unwrap();
        fs::write(&native, b"# Synthetic nested evidence\nExact bytes.\n").unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("nested.jhb");
        let exported = export_bundle(root.path(), &file, "workspace").unwrap();
        assert!(exported["files"].as_u64().unwrap() > 0);
        let mut archive = ZipArchive::new(fs::File::open(&file).unwrap()).unwrap();
        for index in 0..archive.len() {
            let name = archive.by_index(index).unwrap().name().to_owned();
            assert!(!name.contains('\\'));
        }
        let manifest: Value =
            serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
        assert!(
            manifest["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["path"] == "context/projects/nested/evidence.md")
        );
        let restored = out.path().join("restored");
        import_bundle(&restored, &file).unwrap();
        assert_eq!(
            fs::read(restored.join("context/projects/nested/evidence.md")).unwrap(),
            fs::read(native).unwrap()
        );
    }
    fn unknown_attempt(root: &Path) {
        fs::write(
            root.join("applications/answers.json"),
            r#"{"experience":"Synthetic fixture"}"#,
        )
        .unwrap();
        fs::write(root.join("output/cv.pdf"), b"%PDF-1.7\nSynthetic fixture\n").unwrap();
        let engine = crate::Engine::open(root).unwrap();
        let call = |name: &str, args: Value| {
            let result = engine.call(name, &args);
            assert_eq!(result["ok"], true, "{result}");
            result["data"].clone()
        };
        let run = call(
            "start_run",
            json!({"operation":"apply","scope":{"sources":["employer"],"max_submissions":1,"dry_run":false},"user_request":"Synthetic test only","idempotency_key":"run"}),
        );
        call(
            "create_application",
            json!({"run_id":run["run_id"],"record":"applications/example-engineer-123.md","company":"Example","role":"Engineer","vacancy_url":"https://example.invalid/job/123","source":"employer","posting_id":"123","fit_reason":"Synthetic fixture only","evidence":["context/profile/application-decisions.md"],"idempotency_key":"create"}),
        );
        let prepared = call(
            "prepare_application",
            json!({"run_id":run["run_id"],"record":"applications/example-engineer-123.md","company":"Example","vacancy_url":"https://example.invalid/job/123","source":"employer","posting_id":"123","cv_path":"output/cv.pdf","answers_path":"applications/answers.json","idempotency_key":"prepare"}),
        );
        call(
            "begin_submission",
            json!({"attempt_id":prepared["attempt_id"],"idempotency_key":"begin"}),
        );
        call(
            "finish_submission",
            json!({"attempt_id":prepared["attempt_id"],"outcome":"outcome_unknown","summary":"Synthetic lost response","evidence":[],"idempotency_key":"unknown"}),
        );
    }
    #[test]
    fn full_round_trip_preserves_unknown_and_bytes() {
        let t = workspace();
        let ops = t.path().join("applications/operations");
        fs::create_dir_all(&ops).unwrap();
        unknown_attempt(t.path());
        let mut state: Value =
            serde_json::from_slice(&fs::read(ops.join("state.json")).unwrap()).unwrap();
        state["pending"] = Value::Null;
        state["plugin_extension"] = json!({"fixture":true});
        fs::write(ops.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("data.jhb");
        export_bundle(t.path(), &file, "workspace").unwrap();
        let dest = out.path().join("restored");
        import_bundle(&dest, &file).unwrap();
        assert_eq!(
            fs::read(ops.join("state.json")).unwrap(),
            fs::read(dest.join("applications/operations/state.json")).unwrap()
        );
        assert!(!dest.join(".git").exists());
        let restored = crate::Engine::open(&dest)
            .unwrap()
            .call("reconcile_tracker", &json!({}));
        assert_eq!(restored["ok"], true, "{restored}");
        assert_eq!(restored["data"]["unresolved_attempt_count"], 1);
    }
    #[test]
    fn profile_refuses_to_drop_unknown_attempts() {
        let t = workspace();
        unknown_attempt(t.path());
        let out = tempfile::tempdir().unwrap();
        let error =
            export_bundle(t.path(), &out.path().join("profile.jhb"), "profile").unwrap_err();
        assert_eq!(error.code, "unresolved_attempts");
        export_bundle(t.path(), &out.path().join("full.jhb"), "workspace").unwrap();
    }
    #[test]
    fn forbidden_paths_are_filtered() {
        for name in [
            "../outside",
            "context/../secret",
            "context/.Git/config",
            "context/.env",
            "sources/auth.har",
            "context/credentials.json",
            "context/key4.db",
            "context/id_ed25519",
            "applications/operations/.lock",
            "applications/operations/index.sqlite-wal",
        ] {
            assert!(!allowed_path(name), "{name}");
        }
        assert!(allowed_path("context/story.md"));
    }
    #[test]
    fn no_overwrite_and_missing_configured_data() {
        let t = workspace();
        assert!(export_bundle(t.path(), &t.path().join("self.jhb"), "workspace").is_err());
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("data.jhb");
        export_bundle(t.path(), &file, "workspace").unwrap();
        assert!(export_bundle(t.path(), &file, "workspace").is_err());
        fs::remove_file(t.path().join("applications/approved-answers.md")).unwrap();
        assert!(export_bundle(t.path(), &out.path().join("bad.jhb"), "workspace").is_err());
    }
    #[test]
    fn profile_omits_history() {
        let t = workspace();
        fs::write(
            t.path().join("applications/job.md"),
            "# Example\nStatus: Applied",
        )
        .unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("profile.jhb");
        assert_eq!(
            export_bundle(t.path(), &file, "profile").unwrap_err().code,
            "history_acknowledgement_required"
        );
        export_bundle_with_options(t.path(), &file, "profile", true).unwrap();
        let dest = out.path().join("restored");
        import_bundle(&dest, &file).unwrap();
        assert!(!dest.join("applications/job.md").exists());
    }
    fn rewrite(source: &Path, target: &Path, mut transform: impl FnMut(&str, Vec<u8>) -> Vec<u8>) {
        let mut input = ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
        let mut output = ZipWriter::new(fs::File::create(target).unwrap());
        for index in 0..input.len() {
            let mut file = input.by_index(index).unwrap();
            let name = file.name().to_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            output
                .start_file(
                    &name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            output.write_all(&transform(&name, bytes)).unwrap();
        }
        output.finish().unwrap();
    }
    #[test]
    fn modified_payload_cannot_publish_destination() {
        let t = workspace();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("data.jhb");
        export_bundle(t.path(), &file, "workspace").unwrap();
        let corrupt = out.path().join("corrupt.jhb");
        rewrite(&file, &corrupt, |name, bytes| {
            if name == "applications/approved-answers.md" {
                let mut changed = bytes;
                changed[0] = b'!';
                changed
            } else {
                bytes
            }
        });
        let target = out.path().join("restored");
        assert!(import_bundle(&target, &corrupt).is_err());
        assert!(!target.exists());
    }
    #[test]
    fn relabeled_profile_cannot_import_history() {
        let t = workspace();
        fs::write(
            t.path().join("applications/job.md"),
            "# Example\nStatus: Applied",
        )
        .unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("data.jhb");
        export_bundle(t.path(), &file, "workspace").unwrap();
        let corrupt = out.path().join("profile.jhb");
        rewrite(&file, &corrupt, |name, bytes| {
            if name == "manifest.json" {
                let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
                manifest["kind"] = json!("profile");
                serde_json::to_vec(&manifest).unwrap()
            } else {
                bytes
            }
        });
        let target = out.path().join("restored");
        assert!(import_bundle(&target, &corrupt).is_err());
        assert!(!target.exists());
    }
    #[test]
    fn malformed_coordinator_never_publishes_import() {
        let root = workspace();
        let out = tempfile::tempdir().unwrap();
        let original = out.path().join("valid.jhb");
        export_bundle(root.path(), &original, "workspace").unwrap();
        let bad_state=serde_json::to_vec(&json!({"schema_version":1,"runs":{},"attempts":{"broken":{"status":"submitted"}},"events":[]})).unwrap();
        let malformed = out.path().join("bad.jhb");
        rewrite(&original, &malformed, |name, bytes| {
            if name == "applications/operations/state.json" {
                bad_state.clone()
            } else if name == "manifest.json" {
                let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
                for entry in manifest["files"].as_array_mut().unwrap() {
                    if entry["path"] == "applications/operations/state.json" {
                        entry["sha256"] = json!(digest(&bad_state));
                        entry["size"] = json!(bad_state.len());
                    }
                }
                serde_json::to_vec(&manifest).unwrap()
            } else {
                bytes
            }
        });
        let destination = out.path().join("restored");
        let error = import_bundle(&destination, &malformed).unwrap_err();
        assert_eq!(error.code, "invalid_state");
        assert!(!destination.exists());
    }
    #[test]
    fn interrupted_local_journal_is_not_transferable() {
        let t = workspace();
        fs::create_dir_all(t.path().join("applications/operations")).unwrap();
        fs::write(t.path().join("applications/operations/state.json"),serde_json::to_vec(&json!({"schema_version":1,"runs":{},"attempts":{},"events":[],"pending":{"operations":["unfinished"]}})).unwrap()).unwrap();
        let out = tempfile::tempdir().unwrap();
        assert!(export_bundle(t.path(), &out.path().join("bad.jhb"), "workspace").is_err());
    }
    #[test]
    fn full_import_does_not_add_profile_scaffolding() {
        let t = workspace();
        fs::remove_file(t.path().join("AGENTS.md")).unwrap();
        fs::remove_file(t.path().join("README.md")).unwrap();
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("data.jhb");
        export_bundle(t.path(), &file, "workspace").unwrap();
        let target = out.path().join("restored");
        import_bundle(&target, &file).unwrap();
        assert!(!target.join("AGENTS.md").exists());
        assert!(!target.join("README.md").exists());
    }
    #[test]
    fn duplicate_central_directory_names_are_refused() {
        let out = tempfile::tempdir().unwrap();
        let path = out.path().join("duplicate.jhb");
        let mut writer = ZipWriter::new(fs::File::create(&path).unwrap());
        for name in ["context/a.md", "context/b.md"] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(b"synthetic").unwrap();
        }
        writer.finish().unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let from = b"context/b.md";
        let to = b"context/a.md";
        for index in 0..bytes.len() - from.len() {
            if &bytes[index..index + from.len()] == from {
                bytes[index..index + from.len()].copy_from_slice(to);
            }
        }
        fs::write(&path, bytes).unwrap();
        assert!(preflight(&path).unwrap_err().message.contains("Duplicate"));
    }
    #[test]
    fn unicode_and_case_collisions_are_refused() {
        assert_eq!(folded("context/Straße.md"), folded("context/STRASSE.md"));
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("collision.jhb");
        let mut zip = ZipWriter::new(fs::File::create(&file).unwrap());
        for name in ["context/café.md", "context/cafe\u{301}.md"] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"same").unwrap();
        }
        zip.finish().unwrap();
        let mut archive = ZipArchive::new(fs::File::open(file).unwrap()).unwrap();
        assert!(inspect_archive(&mut archive).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn source_symlinks_fail() {
        let t = workspace();
        std::os::unix::fs::symlink("/etc/passwd", t.path().join("context/story.md")).unwrap();
        let out = tempfile::tempdir().unwrap();
        assert!(export_bundle(t.path(), &out.path().join("bad.jhb"), "workspace").is_err());
    }
    #[test]
    fn bundles_reject_windows_device_names_trailing_aliases_and_streams() {
        for name in [
            "context/CON.md",
            "context/NUL.txt",
            "context/AUX",
            "context/PRN.json",
            "context/COM1.md",
            "context/LPT9.txt",
            "context/COM³.md",
            "context/name.md.",
            "context/name.md ",
            "context/file:secret.md",
            "context/path?/file.md",
        ] {
            assert!(!allowed_path(name), "{name}");
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("unsafe.jhb");
            let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"synthetic data").unwrap();
            zip.finish().unwrap();
            let destination = root.path().join("restored");
            assert!(import_bundle(&destination, &path).is_err(), "{name}");
            assert!(!destination.exists(), "{name}");
        }
    }
    #[test]
    fn directory_case_aliases_cannot_merge_distinct_sources_after_transfer() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("aliases.jhb");
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        for name in ["context/Project/first.md", "context/project/second.md"] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"independent source").unwrap();
        }
        zip.finish().unwrap();
        assert!(preflight(&path).unwrap_err().message.contains("collide"));
        let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        assert!(
            inspect_archive(&mut archive)
                .unwrap_err()
                .message
                .contains("collide")
        );
    }
    #[cfg(unix)]
    #[test]
    fn export_refuses_nonportable_data_instead_of_silently_dropping_it() {
        let root = workspace();
        fs::write(root.path().join("context/CON.md"), b"Exact original").unwrap();
        let out = tempfile::tempdir().unwrap();
        let target = out.path().join("export.jhb");
        assert!(
            export_bundle(root.path(), &target, "workspace")
                .unwrap_err()
                .message
                .contains("nonportable")
        );
        assert!(!target.exists());
        assert_eq!(
            fs::read(root.path().join("context/CON.md")).unwrap(),
            b"Exact original"
        );
    }
}
