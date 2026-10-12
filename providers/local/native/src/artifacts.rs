use crate::catalog;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const LIMIT: u64 = 24 * 1024 * 1024 * 1024;
pub fn hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "model_integrity_error: artifact missing")?;
    let mut state = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|_| "model_integrity_error: cannot read artifact")?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
    }
    Ok(hex::encode(state.finalize()))
}
fn check(path: &Path, bytes: u64, sha: &str) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "model_integrity_error: artifact missing")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != bytes
        || hash(path)? != sha
    {
        return Err("model_integrity_error: artifact size or SHA256 mismatch".into());
    }
    Ok(())
}
fn safe_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || Path::new(value).is_absolute()
        || Path::new(value)
            .components()
            .any(|v| !matches!(v, std::path::Component::Normal(_)))
    {
        return Err("model_integrity_error: unsafe artifact path".into());
    }
    Ok(())
}
fn files(spec: &Value) -> Result<Vec<(PathBuf, u64, String, String)>, String> {
    if let Some(components) = spec["components"].as_array() {
        let mut files = Vec::new();
        for component in components {
            let role = component["role"]
                .as_str()
                .ok_or("model_integrity_error: component role missing")?;
            safe_name(role)?;
            for item in component["files"]
                .as_array()
                .ok_or("model_integrity_error: files missing")?
            {
                let name = item["path"]
                    .as_str()
                    .ok_or("model_integrity_error: path missing")?;
                safe_name(name)?;
                let sha = item["sha256"].as_str().ok_or(
                    "unsupported_runtime: metadata-only profile lacks complete reviewed hashes",
                )?;
                files.push((
                    PathBuf::from(role).join(name),
                    item["bytes"]
                        .as_u64()
                        .ok_or("model_integrity_error: bytes missing")?,
                    sha.into(),
                    item["url"]
                        .as_str()
                        .ok_or("model_integrity_error: URL missing")?
                        .into(),
                ));
            }
        }
        Ok(files)
    } else {
        Ok(vec![(
            PathBuf::from(
                spec["artifact"]
                    .as_str()
                    .or_else(|| spec["filename"].as_str())
                    .ok_or("model_integrity_error: artifact missing")?,
            ),
            spec["download_bytes"]
                .as_u64()
                .ok_or("model_integrity_error: bytes missing")?,
            spec["sha256"]
                .as_str()
                .ok_or("model_integrity_error: SHA256 missing")?
                .into(),
            spec["source"]
                .as_str()
                .ok_or("model_integrity_error: URL missing")?
                .into(),
        )])
    }
}
fn ensure_supported(spec: &Value) -> Result<(), String> {
    if spec["status"]
        .as_str()
        .is_some_and(|s| s.starts_with("metadata_only"))
    {
        return Err("unsupported_runtime: this profile is reviewed metadata only; no download or inference adapter".into());
    }
    Ok(())
}
pub fn verify(cache: &Path, spec: &Value) -> Result<PathBuf, String> {
    ensure_supported(spec)?;
    let dir = catalog::dir(cache, spec);
    let manifest: Value = serde_json::from_slice(
        &fs::read(dir.join("manifest.json"))
            .map_err(|_| "model_integrity_error: explicit install required")?,
    )
    .map_err(|_| "model_integrity_error: invalid manifest")?;
    if manifest["id"] != spec["id"] || manifest["scope"] != catalog::token(spec) {
        return Err("model_integrity_error: manifest scope mismatch".into());
    }
    let files = files(spec)?;
    for (name, bytes, sha, _) in &files {
        check(&dir.join(name), *bytes, sha)?;
    }
    if spec["components"].is_array() {
        for component in spec["components"].as_array().unwrap() {
            let role = component["role"].as_str().unwrap();
            let expected = files
                .iter()
                .filter(|(p, _, _, _)| p.starts_with(role))
                .map(|(p, _, _, _)| dir.join(p))
                .collect::<std::collections::HashSet<_>>();
            for entry in fs::read_dir(dir.join(role))
                .map_err(|_| "model_integrity_error: component missing")?
            {
                let p = entry
                    .map_err(|_| "model_integrity_error: component unreadable")?
                    .path();
                if !expected.contains(&p) {
                    return Err("model_integrity_error: unlisted artifact in component".into());
                }
            }
        }
    }
    if let Some(member) = spec["member"].as_str() {
        let archive = File::open(dir.join(spec["filename"].as_str().unwrap()))
            .map_err(|_| "model_integrity_error: native archive missing")?;
        let mut archive = zip::ZipArchive::new(archive)
            .map_err(|_| "model_integrity_error: native archive invalid")?;
        let mut entry = archive
            .by_name(member)
            .map_err(|_| "unsupported_runtime: pinned native library missing in archive")?;
        if entry.size() > 512 * 1024 * 1024 {
            return Err("model_integrity_error: native library unexpectedly large".into());
        }
        let mut hash = Sha256::new();
        std::io::copy(&mut entry, &mut HashWriter(&mut hash))
            .map_err(|_| "model_integrity_error: native archive unreadable")?;
        let expected = hex::encode(hash.finalize());
        let lib = dir.join(Path::new(member).file_name().unwrap());
        if crate::artifacts::hash(&lib)? != expected {
            return Err("model_integrity_error: extracted native library mismatch".into());
        }
        if spec["native_sha256"]
            .as_str()
            .is_some_and(|sha| sha != expected)
        {
            return Err("model_integrity_error: known native library SHA256 mismatch".into());
        }
        return Ok(lib);
    }
    Ok(if let Some(artifact) = spec["artifact"].as_str() {
        dir.join(artifact)
    } else {
        dir
    })
}
struct HashWriter<'a>(&'a mut Sha256);
impl Write for HashWriter<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn download(url: &str, path: &Path, bytes: u64, sha: &str) -> Result<(), String> {
    if bytes > LIMIT || !url.starts_with("https://") {
        return Err("download_failed: artifact scope invalid".into());
    }
    if check(path, bytes, sha).is_ok() {
        return Ok(());
    }
    let tmp = path.with_extension("part");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "download_failed: cannot create cache")?;
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(1200)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|_| "download_failed: public artifact unavailable; retry explicit download")?;
    let mut reader = response.body_mut().as_reader().take(bytes + 1);
    let mut output = File::create(&tmp).map_err(|_| "download_failed: cache unavailable")?;
    let copied = std::io::copy(&mut reader, &mut output)
        .map_err(|_| "download_failed: interrupted transfer; retry explicit download")?;
    output
        .sync_all()
        .map_err(|_| "download_failed: cache write failed")?;
    if copied != bytes || hash(&tmp)? != sha {
        let _ = fs::remove_file(&tmp);
        return Err("model_integrity_error: downloaded size or SHA256 mismatch".into());
    }
    fs::rename(&tmp, path).map_err(|_| "download_failed: cannot publish artifact")?;
    Ok(())
}
pub fn install(cache: &Path, spec: &Value, approved: &str) -> Result<Value, String> {
    ensure_supported(spec)?;
    if approved != catalog::token(spec) {
        return Err("approval_required: inspect info then supply its exact --approve token".into());
    }
    let dir = catalog::dir(cache, spec);
    for (name, bytes, sha, url) in files(spec)? {
        download(&url, &dir.join(name), bytes, &sha)?;
    }
    if let Some(member) = spec["member"].as_str() {
        let mut archive = zip::ZipArchive::new(
            File::open(dir.join(spec["filename"].as_str().unwrap()))
                .map_err(|_| "download_failed: native archive unavailable")?,
        )
        .map_err(|_| "model_integrity_error: native archive invalid")?;
        let mut entry = archive
            .by_name(member)
            .map_err(|_| "unsupported_runtime: pinned library missing")?;
        if entry.size() > 512 * 1024 * 1024 {
            return Err("model_integrity_error: native library unexpectedly large".into());
        }
        let target = dir.join(Path::new(member).file_name().unwrap());
        let tmp = target.with_extension("part");
        let mut output = File::create(&tmp).map_err(|_| "download_failed: cache write failed")?;
        std::io::copy(&mut entry, &mut output).map_err(|_| "download_failed: extract failed")?;
        output
            .sync_all()
            .map_err(|_| "download_failed: sync failed")?;
        fs::rename(tmp, target).map_err(|_| "download_failed: cannot publish native library")?;
    }
    fs::create_dir_all(&dir).map_err(|_| "download_failed: cache unavailable")?;
    let manifest = json!({"id":spec["id"],"scope":catalog::token(spec),"revision":spec["revision"],"version":spec["version"]});
    let tmp = dir.join("manifest.part");
    fs::write(&tmp, serde_json::to_vec_pretty(&manifest).unwrap())
        .map_err(|_| "download_failed: manifest write failed")?;
    fs::rename(tmp, dir.join("manifest.json"))
        .map_err(|_| "download_failed: manifest publication failed")?;
    let installed = verify(cache, spec)?;
    Ok(json!({"installed":true,"id":spec["id"],"path":installed}))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(bytes: &[u8]) -> Value {
        json!({"id":"fixture","revision":"immutable","artifact":"fixture.bin","download_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes)),"source":"https://public.invalid/fixture.bin"})
    }
    #[test]
    fn complete_cached_file_installs_without_network_and_repairs_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"fictional public model";
        let spec = fixture(bytes);
        let dir = catalog::dir(temp.path(), &spec);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("fixture.bin"), bytes).unwrap();
        fs::write(dir.join("manifest.json"), b"interrupted").unwrap();
        install(temp.path(), &spec, &catalog::token(&spec)).unwrap();
        assert!(verify(temp.path(), &spec).is_ok());
        fs::write(dir.join("fixture.bin"), b"corrupt").unwrap();
        assert!(verify(temp.path(), &spec).is_err());
    }
    #[test]
    fn multi_artifact_complete_manifest_and_extra_file_guard() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"synthetic";
        let spec = json!({"id":"fixture","revision":"immutable","components":[{"role":"base","files":[{"path":"weights.bin","bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes)),"url":"https://public.invalid/weights.bin"}]}]});
        let dir = catalog::dir(temp.path(), &spec);
        fs::create_dir_all(dir.join("base")).unwrap();
        fs::write(dir.join("base/weights.bin"), bytes).unwrap();
        install(temp.path(), &spec, &catalog::token(&spec)).unwrap();
        assert!(verify(temp.path(), &spec).is_ok());
        fs::write(dir.join("base/unreviewed.bin"), bytes).unwrap();
        assert!(verify(temp.path(), &spec).unwrap_err().contains("unlisted"));
    }
    #[test]
    fn missing_approval_and_metadata_only_do_not_create_cache() {
        let temp = tempfile::tempdir().unwrap();
        let spec = fixture(b"fixture");
        assert!(
            install(temp.path(), &spec, "wrong")
                .unwrap_err()
                .contains("approval_required")
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        let mut spec = fixture(b"fixture");
        spec["status"] = json!("metadata_only_not_downloaded");
        assert!(
            install(temp.path(), &spec, &catalog::token(&spec))
                .unwrap_err()
                .contains("metadata only")
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    #[test]
    fn native_archive_extraction_is_single_member_and_hash_checked() {
        use zip::{ZipWriter, write::SimpleFileOptions};
        let temp = tempfile::tempdir().unwrap();
        let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("sdk/native.dylib", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"synthetic native library").unwrap();
        zip.start_file("sdk/unneeded.py", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"never extracted").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let spec = json!({"id":"native-fixture","version":"1","filename":"sdk.whl","download_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes)),"source":"https://public.invalid/sdk.whl","member":"sdk/native.dylib","native_sha256":hex::encode(Sha256::digest(b"synthetic native library"))});
        let dir = catalog::dir(temp.path(), &spec);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("sdk.whl"), bytes).unwrap();
        install(temp.path(), &spec, &catalog::token(&spec)).unwrap();
        assert_eq!(
            fs::read(verify(temp.path(), &spec).unwrap()).unwrap(),
            b"synthetic native library"
        );
        assert!(!dir.join("unneeded.py").exists());
        fs::write(dir.join("native.dylib"), b"tampered").unwrap();
        assert!(verify(temp.path(), &spec).is_err());
    }
    #[test]
    fn unsafe_paths_are_rejected() {
        assert!(safe_name("../outside").is_err());
        assert!(safe_name("/outside").is_err());
        assert!(safe_name("good/file.bin").is_ok());
    }
}
