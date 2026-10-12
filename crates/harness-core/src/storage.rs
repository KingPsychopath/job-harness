//! Local transactions: one workspace lock, fsynced atomic files, compatible journal.
use crate::{HarnessError, Result};
use fs2::FileExt;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub struct WorkspaceLock {
    file: File,
}
/// Reject every existing symlink component before creating private files.
/// The selected root is canonicalized by Engine; host callers must do likewise.
pub fn reject_symlink_ancestors(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for piece in path.components() {
        current.push(piece.as_os_str());
        if current.is_symlink() {
            // macOS's OS-owned /var and /tmp aliases are outside a selected workspace.
            // Preserve strict refusal for every candidate-controlled component below them.
            #[cfg(target_os = "macos")]
            if [("/var", "/private/var"), ("/tmp", "/private/tmp")]
                .iter()
                .any(|(alias, target)| {
                    current == Path::new(alias)
                        && current.canonicalize().ok().as_deref() == Some(Path::new(target))
                })
            {
                continue;
            }
            return Err(HarnessError::new(
                "path_denied",
                "Private writes refuse symlinked path components",
            ));
        }
    }
    Ok(())
}
impl WorkspaceLock {
    pub fn acquire(root: &Path, exclusive: bool) -> Result<Self> {
        let home = crate::workspace::safe_path(root, "applications/operations")?;
        reject_symlink_ancestors(&home)?;
        fs::create_dir_all(&home)?;
        let path = crate::workspace::safe_path(root, "applications/operations/.lock")?;
        if path.is_symlink() {
            return Err(HarnessError::new(
                "path_denied",
                "Coordinator lock must not be a symlink",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
        }
        let file = options.open(path)?;
        if exclusive {
            FileExt::lock_exclusive(&file)?
        } else {
            FileExt::lock_shared(&file)?
        }
        Ok(Self { file })
    }
}
impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
/// Historical sort_keys=True, indent=2, ensure_ascii=False UTF-8 encoding.
/// Existing packet and idempotency hashes must survive a language change.
pub fn encoded(value: &Value) -> Vec<u8> {
    job_harness_domain::canonical::encoded(value)
}
/// Encode an internally derived native path in the portable relative wire format.
/// This converts native separators between components, never characters inside a
/// component. External strings still go through the strict portable-path guards.
pub(crate) fn portable_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| HarnessError::new("path_denied", "Path is outside the selected workspace"))?;
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str().ok_or_else(|| {
                HarnessError::new("path_denied", "Workspace paths must be portable UTF-8")
            }),
            _ => Err(HarnessError::new(
                "path_denied",
                "Workspace paths must contain only normal relative components",
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    let encoded = components.join("/");
    if !crate::workspace::portable_relative_path(&encoded) {
        return Err(HarnessError::new(
            "path_denied",
            "Workspace paths must use portable filename components",
        ));
    }
    Ok(encoded)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_symlink_ancestors(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| HarnessError::validation("Destination must have a parent"))?;
    fs::create_dir_all(parent)?;
    if path.is_symlink() {
        return Err(HarnessError::new(
            "path_denied",
            "Writes refuse symlinked destinations",
        ));
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".career-tmp-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, SetFileAttributesW};
        let native: Vec<u16> = temporary
            .path()
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // Named temporary files start with FILE_ATTRIBUTE_TEMPORARY. Clear it
        // before publication so the durable coordinator is not a cache hint.
        if unsafe { SetFileAttributesW(native.as_ptr(), FILE_ATTRIBUTE_NORMAL) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        publish_rename(temporary.path(), path, true)?;
    }
    #[cfg(not(windows))]
    temporary
        .persist(path)
        .map_err(|_| HarnessError::new("io_error", "Atomic publication failed"))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
/// Same-volume publication; never copy/delete through another volume.
/// The caller holds the workspace lock when replacing canonical state.
pub(crate) fn publish_rename(source: &Path, destination: &Path, replace: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        // Canonical parents supply the extended-length drive/UNC prefix; native
        // paths are not confused with the forward-slash relative wire format.
        let source = source.canonicalize()?;
        let destination = destination
            .parent()
            .ok_or_else(|| HarnessError::validation("Destination needs a parent"))?
            .canonicalize()?
            .join(
                destination
                    .file_name()
                    .ok_or_else(|| HarnessError::validation("Destination needs a filename"))?,
            );
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        };
        let source = wide(&source);
        let destination = wide(&destination);
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        // Source and destination are owned buffers, NUL-terminated for this call.
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if !replace && destination.exists() {
            return Err(HarnessError::validation(
                "Publication destination already exists",
            ));
        }
        fs::rename(source, destination)?;
        Ok(())
    }
}

pub fn load_state(root: &Path) -> Result<Value> {
    crate::state::load(root)
}
pub fn commit_state(root: &Path, state: &Value) -> Result<()> {
    let path = safe_path(
        root,
        "applications/operations/state.json",
        &["applications"],
        &[".json"],
    )?;
    atomic_write(&path, &encoded(state))
}
pub fn safe_path(
    root: &Path,
    relative: &str,
    roots: &[&str],
    extensions: &[&str],
) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if !crate::workspace::portable_relative_path(relative) {
        return Err(HarnessError::new(
            "path_denied",
            "Use an allowlisted portable workspace-relative path",
        ));
    }
    let first = rel
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("");
    let singles = ["AGENTS.md", "README.md", "resume.typ"];
    if !roots.contains(&first) && !singles.contains(&relative) {
        return Err(HarnessError::new(
            "path_denied",
            "Path outside workspace allowlist",
        ));
    }
    let ext = rel
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    if !extensions.contains(&ext.as_str()) {
        return Err(HarnessError::validation("Unsupported file type"));
    }
    let base = root.canonicalize()?;
    let path = base.join(rel);
    let mut current = base.clone();
    for piece in rel.components() {
        crate::workspace::exact_component(&current, piece.as_os_str())?;
        current.push(piece.as_os_str());
        if current.exists() || current.is_symlink() {
            let actual = current.canonicalize()?;
            if !actual.starts_with(&base) {
                return Err(HarnessError::new(
                    "path_denied",
                    "Path escapes the selected workspace",
                ));
            }
        }
    }
    Ok(path)
}
pub fn rebuild_index(root: &Path) -> Result<Value> {
    crate::index::refresh(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_nested_paths_use_portable_wire_components() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let native = root.join("context").join("projects").join("alpha.md");
        let relative = portable_relative(root, &native).unwrap();
        assert_eq!(relative, "context/projects/alpha.md");
        assert_eq!(
            json!({"path":relative})["path"],
            "context/projects/alpha.md"
        );
        assert!(portable_relative(root, root).is_err());
        assert!(portable_relative(root, &root.join("context/../outside.md")).is_err());
        assert!(portable_relative(root, &root.parent().unwrap().join("outside.md")).is_err());
        for untrusted in [
            "context\\projects\\alpha.md",
            "C:/context/alpha.md",
            "//server/share/alpha.md",
            "context/alpha.md:stream",
        ] {
            assert!(safe_path(root, untrusted, &["context"], &[".md"]).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_conversion_does_not_launder_literal_backslashes() {
        let workspace = tempfile::tempdir().unwrap();
        let native = workspace.path().join("context").join("fictional\\notes.md");
        assert!(portable_relative(workspace.path(), &native).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn native_drive_and_unc_roots_encode_relative_paths_only() {
        for root in [
            Path::new(r"C:\workspace"),
            Path::new(r"\\server\share\workspace"),
            Path::new(r"\\?\C:\workspace"),
        ] {
            let path = root.join("applications").join("nested").join("role.md");
            assert!(path.as_os_str().to_string_lossy().contains('\\'));
            assert_eq!(
                portable_relative(root, &path).unwrap(),
                "applications/nested/role.md"
            );
        }
    }
    #[test]
    fn historical_hash_encoding_handles_unicode_and_sorting() {
        assert_eq!(
            String::from_utf8(encoded(&json!({"z":["£","😀"],"a":true}))).unwrap(),
            "{\n  \"a\": true,\n  \"z\": [\n    \"£\",\n    \"😀\"\n  ]\n}\n"
        );
    }
    #[test]
    fn writes_are_atomic_and_reject_escaping_links() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("private");
        fs::create_dir_all(root.join("context")).unwrap();
        atomic_write(&root.join("context/facts.md"), b"facts").unwrap();
        assert_eq!(fs::read(root.join("context/facts.md")).unwrap(), b"facts");
        assert!(safe_path(&root, "context/../outside.md", &["context"], &[".md"]).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(t.path(), root.join("context/escape")).unwrap();
            assert!(safe_path(&root, "context/escape/facts.md", &["context"], &[".md"]).is_err());
        }
    }
    #[test]
    fn replacement_keeps_exact_bytes_and_failed_publication_keeps_destination() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        let old = encoded(&json!({"schema_version":1,"label":"£ and 日本語","runs":{}}));
        let new = encoded(&json!({"schema_version":1,"label":"Replaced 😀","runs":{"a":1}}));
        atomic_write(&path, &old).unwrap();
        atomic_write(&path, &new).unwrap();
        assert_eq!(fs::read(&path).unwrap(), new);
        let directory = root.path().join("keep-directory");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("keep.txt"), b"retained").unwrap();
        assert!(atomic_write(&directory, b"cannot replace directory").is_err());
        assert_eq!(fs::read(directory.join("keep.txt")).unwrap(), b"retained");
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".career-tmp-")
        }));
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            assert_eq!(
                fs::metadata(path).unwrap().file_attributes()
                    & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_TEMPORARY,
                0
            );
        }
    }
    #[test]
    fn coordinator_lock_excludes_writers_and_survives_multiple_readers() {
        let root = tempfile::tempdir().unwrap();
        let first = WorkspaceLock::acquire(root.path(), false).unwrap();
        let second = WorkspaceLock::acquire(root.path(), false).unwrap();
        let probe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.path().join("applications/operations/.lock"))
            .unwrap();
        assert!(FileExt::try_lock_exclusive(&probe).is_err());
        drop(first);
        assert!(FileExt::try_lock_exclusive(&probe).is_err());
        drop(second);
        FileExt::try_lock_exclusive(&probe).unwrap();
        FileExt::unlock(&probe).unwrap();
        let writer = WorkspaceLock::acquire(root.path(), true).unwrap();
        assert!(FileExt::try_lock_shared(&probe).is_err());
        drop(writer);
        FileExt::try_lock_shared(&probe).unwrap();
        FileExt::unlock(&probe).unwrap();
    }
}
