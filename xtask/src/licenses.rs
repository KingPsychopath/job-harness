//! Offline dependency notices with explicit coverage, not inferred legal compliance.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Notices {
    pub inventory: Value,
    pub files: Vec<(String, Vec<u8>)>,
}
struct LicenseSource {
    path: String,
    bytes: Vec<u8>,
    provenance: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedIndex {
    schema_version: u32,
    packages: Vec<PinnedPackage>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedPackage {
    name: String,
    version: String,
    declared_license: String,
    repository: String,
    commit: String,
    path_in_vcs: String,
    texts: Vec<PinnedText>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedText {
    path: String,
    upstream_path: Option<String>,
    sha256: String,
    bytes: u64,
    content_role: Option<String>,
    source_excerpt: Option<SourceExcerpt>,
    standard_license: Option<StandardLicense>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceExcerpt {
    path: String,
    sha256: String,
    start_line: usize,
    end_line: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StandardLicense {
    id: String,
    source_url: String,
    referenced_url: String,
}
fn pin_provenance(
    pin: &PinnedPackage,
    text: &PinnedText,
    directory: &Path,
    data: &[u8],
    prior: &[LicenseSource],
) -> Result<Value> {
    let role = text.content_role.as_deref().unwrap_or("license_or_notice");
    ensure!(
        [
            "license_or_notice",
            "upstream_licensing_statement",
            "source_license_header",
            "standard_license"
        ]
        .contains(&role),
        "Unknown pinned notice content role"
    );
    if let Some(standard) = &text.standard_license {
        ensure!(
            text.source_excerpt.is_none()
                && text.upstream_path.is_none()
                && role == "standard_license"
                && pin.declared_license == standard.id
                && standard.id == "MPL-2.0"
                && standard.referenced_url == "https://mozilla.org/MPL/2.0/"
                && standard
                    .source_url
                    .starts_with("https://www.mozilla.org/media/MPL/2.0/")
                && standard.source_url.ends_with(".txt")
                && public_link(&standard.source_url).as_deref()
                    == Some(standard.source_url.as_str())
                && prior.iter().any(|source| source.provenance["content_role"]
                    == "source_license_header"
                    && source
                        .bytes
                        .windows(standard.referenced_url.len())
                        .any(|window| window == standard.referenced_url.as_bytes())),
            "Standard license must match this published source header's exact declared license and reference"
        );
        return Ok(
            json!({"kind":"standard_license","content_role":role,"license_id":standard.id,"publisher":"Mozilla Foundation","source_url":standard.source_url,"referenced_url":standard.referenced_url}),
        );
    }
    let upstream = text
        .upstream_path
        .as_deref()
        .context("Pinned upstream notice path missing")?;
    ensure!(
        job_harness_core::bundle::allowed_path(&format!("docs/{upstream}")),
        "Invalid pinned upstream notice path"
    );
    let mut provenance = json!({"kind":"pinned_upstream","content_role":role,"repository":pin.repository,"commit":pin.commit,"crate_path_in_vcs":pin.path_in_vcs,"upstream_path":upstream,"upstream_url":format!("{}/blob/{}/{}",pin.repository,pin.commit,upstream)});
    if let Some(excerpt) = &text.source_excerpt {
        ensure!(
            role == "source_license_header"
                && job_harness_core::bundle::allowed_path(&format!("docs/{}", excerpt.path))
                && excerpt.start_line > 0
                && excerpt.end_line >= excerpt.start_line
                && excerpt.end_line <= 200
                && regex::Regex::new("^[0-9a-f]{64}$")?.is_match(&excerpt.sha256)
                && upstream
                    == if pin.path_in_vcs.is_empty() {
                        excerpt.path.clone()
                    } else {
                        format!("{}/{}", pin.path_in_vcs, excerpt.path)
                    },
            "Pinned source header must have exact bounded published-source anchors"
        );
        let source = directory.join(&excerpt.path);
        job_harness_core::storage::reject_symlink_ancestors(&source)?;
        ensure!(
            source.canonicalize()?.starts_with(directory)
                && fs::metadata(&source)?.len() <= 2 * 1024 * 1024,
            "Pinned source header escapes its package or exceeds bounds"
        );
        let source = fs::read(source)?;
        ensure!(
            job_harness_core::storage::digest(&source) == excerpt.sha256,
            "Published source header differs from its pinned complete-source digest"
        );
        let expected: String = std::str::from_utf8(&source)?
            .split_inclusive('\n')
            .skip(excerpt.start_line - 1)
            .take(excerpt.end_line - excerpt.start_line + 1)
            .collect();
        ensure!(
            expected.as_bytes() == data,
            "Pinned notice must retain the exact declared source-header lines"
        );
        provenance["source_excerpt"] = json!({"path":excerpt.path,"source_sha256":excerpt.sha256,"start_line":excerpt.start_line,"end_line":excerpt.end_line});
    } else {
        ensure!(
            license_name(Path::new(upstream))
                && role != "source_license_header"
                && role != "standard_license",
            "Pinned notice requires a canonical upstream license/notice file"
        );
    }
    Ok(provenance)
}
fn pinned_sources(root: &Path, package: &Value, directory: &Path) -> Result<Vec<LicenseSource>> {
    let index_path = root.join("resources/licenses/index.json");
    if !index_path.exists() {
        return Ok(Vec::new());
    }
    let index_path = job_harness_core::storage::safe_path(
        root,
        "resources/licenses/index.json",
        &["resources"],
        &[".json"],
    )?;
    let index: PinnedIndex = serde_json::from_slice(&fs::read(index_path)?)?;
    ensure!(
        index.schema_version == 1,
        "Unsupported pinned notices index"
    );
    let matches: Vec<_> = index
        .packages
        .iter()
        .filter(|pin| package["name"] == pin.name && package["version"] == pin.version)
        .collect();
    ensure!(
        matches.len() <= 1,
        "Duplicate pinned dependency notice identity"
    );
    let Some(pin) = matches.first() else {
        return Ok(Vec::new());
    };
    ensure!(
        package["source"]
            .as_str()
            .is_some_and(|source| source.starts_with("registry+"))
            && package["license"] == pin.declared_license
            && package["repository"]
                .as_str()
                .and_then(public_link)
                .as_deref()
                == Some(pin.repository.as_str()),
        "Pinned notice differs from the published dependency identity"
    );
    let commit = regex::Regex::new("^[0-9a-f]{40}$")?;
    let sha256 = regex::Regex::new("^[0-9a-f]{64}$")?;
    ensure!(
        commit.is_match(&pin.commit)
            && pin.repository.starts_with("https://github.com/")
            && pin.repository.split('/').count() == 5
            && public_link(&pin.repository).as_deref() == Some(pin.repository.as_str())
            && (pin.path_in_vcs.is_empty()
                || job_harness_core::bundle::allowed_path(&format!("docs/{}", pin.path_in_vcs))),
        "Pinned upstream notice requires an exact public repository and commit"
    );
    let vcs: Value = serde_json::from_slice(&fs::read(directory.join(".cargo_vcs_info.json"))?)?;
    ensure!(
        vcs["git"]["sha1"] == pin.commit && vcs["path_in_vcs"] == pin.path_in_vcs,
        "Pinned notice commit differs from this published crate's VCS evidence"
    );
    ensure!(
        !pin.texts.is_empty() && pin.texts.len() <= 16,
        "Pinned notice text list must be bounded and non-empty"
    );
    let prefix = format!("text/{}-{}/", pin.name, pin.version);
    let mut paths = BTreeSet::new();
    let mut result = Vec::new();
    for text in &pin.texts {
        ensure!(
            text.path.starts_with(&prefix)
                && paths.insert(text.path.clone())
                && license_name(Path::new(&text.path))
                && sha256.is_match(&text.sha256)
                && text.bytes <= 2 * 1024 * 1024,
            "Invalid pinned notice path, digest or size"
        );
        let source = job_harness_core::storage::safe_path(
            root,
            &format!("resources/licenses/{}", text.path),
            &["resources"],
            &[
                "", ".txt", ".md", ".rst", ".html", ".mit", ".apache", ".bsd", ".lesser",
            ],
        )?;
        let data = fs::read(source)?;
        ensure!(
            data.len() as u64 == text.bytes
                && job_harness_core::storage::digest(&data) == text.sha256
                && !crate::repository::private_content(&data),
            "Pinned dependency notice text does not match its recorded digest"
        );
        result.push(LicenseSource {
            path: Path::new(&text.path)
                .strip_prefix(&prefix)?
                .to_str()
                .context("Pinned license destination is not UTF-8")?
                .to_owned(),
            provenance: pin_provenance(pin, text, directory, &data, &result)?,
            bytes: data,
        });
    }
    Ok(result)
}
fn license_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ["license", "licence", "copying", "notice", "copyright"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        && [
            "", "txt", "md", "rst", "html", "mit", "apache", "bsd", "lesser",
        ]
        .contains(&extension.as_str())
}
fn public_link(value: &str) -> Option<String> {
    let base = value.split(['?', '#']).next()?;
    job_harness_domain::identity::normal_url(base).ok()
}
fn closure(metadata: &Value, root_package: &str) -> Result<BTreeSet<String>> {
    let packages = metadata["packages"]
        .as_array()
        .context("Cargo package metadata missing")?;
    let selected = packages
        .iter()
        .find(|package| package["name"] == root_package)
        .context("Release component package missing from Cargo metadata")?;
    let selected = selected["id"]
        .as_str()
        .context("Cargo package identity missing")?;
    let nodes: BTreeMap<_, _> = metadata["resolve"]["nodes"]
        .as_array()
        .context("Cargo dependency graph missing")?
        .iter()
        .filter_map(|node| node["id"].as_str().map(|id| (id, node)))
        .collect();
    let mut pending = VecDeque::from([selected.to_owned()]);
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let node = nodes
            .get(id.as_str())
            .context("Cargo dependency node missing")?;
        for dependency in node["deps"].as_array().into_iter().flatten() {
            let included = dependency["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind["kind"].is_null() || kind["kind"] == "build");
            if included {
                pending.push_back(
                    dependency["pkg"]
                        .as_str()
                        .context("Cargo dependency identity missing")?
                        .to_owned(),
                );
            }
        }
    }
    Ok(seen)
}
fn sources(
    root: &Path,
    package: &Value,
    project_license: Option<&str>,
) -> Result<Vec<LicenseSource>> {
    let manifest = PathBuf::from(
        package["manifest_path"]
            .as_str()
            .context("Cargo manifest source missing")?,
    );
    let directory = manifest
        .parent()
        .context("Cargo source package needs a parent")?
        .canonicalize()?;
    let mut found = BTreeSet::new();
    if directory.starts_with(root)
        && package["license"]
            .as_str()
            .is_some_and(|license| Some(license) == project_license)
    {
        found.insert(root.join("LICENSE").canonicalize()?);
    }
    for entry in walkdir::WalkDir::new(&directory)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !["target", ".git", ".scratch", ".build"]
                .contains(&entry.file_name().to_str().unwrap_or(""))
        })
    {
        let entry = entry?;
        if entry.file_type().is_file() && license_name(entry.path()) {
            found.insert(entry.path().to_owned());
        }
        ensure!(
            found.len() <= 512,
            "Dependency has too many license notice files"
        );
    }
    if let Some(declared) = package["license_file"].as_str() {
        let candidate = directory.join(declared);
        if candidate.exists() && license_name(&candidate) && !candidate.is_symlink() {
            let canonical = candidate.canonicalize()?;
            if canonical.starts_with(&directory)
                || canonical == root.join("LICENSE").canonicalize()?
            {
                found.insert(canonical);
            }
        }
    }
    let mut files = Vec::new();
    let mut total = 0usize;
    for path in found {
        let canonical = path.canonicalize()?;
        ensure!(
            canonical.starts_with(&directory)
                || canonical == root.join("LICENSE").canonicalize()?,
            "Declared license source escapes its package"
        );
        ensure!(
            fs::metadata(&canonical)?.len() <= 2 * 1024 * 1024,
            "Dependency license notice exceeds two MiB"
        );
        let relative = canonical
            .strip_prefix(&directory)
            .unwrap_or_else(|_| Path::new("LICENSE"));
        let relative = relative
            .to_str()
            .context("License source filename must be UTF-8")?
            .replace(std::path::MAIN_SEPARATOR, "/");
        ensure!(
            job_harness_core::bundle::allowed_path(&format!("docs/{relative}")),
            "Dependency notice has an unsafe archive filename"
        );
        let data = fs::read(canonical)?;
        ensure!(
            !crate::repository::private_content(&data),
            "Dependency notice contains a private path or key marker"
        );
        total += data.len();
        ensure!(
            total <= 8 * 1024 * 1024,
            "Dependency license notices exceed eight MiB"
        );
        files.push(LicenseSource {
            path: relative,
            bytes: data,
            provenance: json!({"kind":if directory.starts_with(root) {"workspace_source"} else {"published_source"}}),
        });
    }
    if files.is_empty() {
        files = pinned_sources(root, package, &directory)?;
    }
    Ok(files)
}
pub fn collect(root: &Path, manifest: &Path, target: &str, package_name: &str) -> Result<Notices> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            target,
            "--manifest-path",
        ])
        .arg(manifest)
        .current_dir(root)
        .output()?;
    ensure!(
        output.status.success(),
        "Offline dependency metadata unavailable; fetch this component's locked sources before staging"
    );
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let included = closure(&metadata, package_name)?;
    let project_manifest = fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml::Value>()?;
    let project_license = project_manifest
        .get("workspace")
        .and_then(|value| value.get("package"))
        .and_then(|value| value.get("license"))
        .and_then(toml::Value::as_str);
    let mut packages: Vec<_> = metadata["packages"]
        .as_array()
        .context("Cargo packages missing")?
        .iter()
        .filter(|package| {
            package["id"]
                .as_str()
                .is_some_and(|id| included.contains(id))
        })
        .collect();
    packages.sort_by(|a, b| {
        (a["name"].as_str(), a["version"].as_str())
            .cmp(&(b["name"].as_str(), b["version"].as_str()))
    });
    let mut rows = Vec::new();
    let mut files = Vec::new();
    let mut missing = Vec::new();
    let mut text_roles = BTreeMap::<String, usize>::new();
    let mut total_notice_bytes = 0usize;
    let mut notice = String::from(
        "# Third-party dependency notices\n\nGenerated offline from the selected manifest's normal/build dependency closure for this target; dev-only edges are excluded. Cargo may unify workspace features, so this inventory can include code paths not linked into this component. Package-declared SPDX expressions and available license texts are recorded; missing texts remain explicit in THIRD_PARTY.json. Downloaded model weights and optional model runtime libraries are not included.\n\n",
    );
    for package in packages {
        let name = package["name"]
            .as_str()
            .context("Dependency name missing")?;
        let version = package["version"]
            .as_str()
            .context("Dependency version missing")?;
        let source_kind = if package["source"].is_null() {
            "local_source"
        } else if package["source"]
            .as_str()
            .is_some_and(|source| source.starts_with("registry+"))
        {
            "registry"
        } else {
            "git_source"
        };
        let mut links = Vec::new();
        if source_kind == "registry" {
            links.push(format!("https://crates.io/crates/{name}/{version}"));
        }
        if let Some(repository) = package["repository"].as_str().and_then(public_link) {
            links.push(repository);
        }
        let mut license_files = Vec::new();
        for source in sources(root, package, project_license)? {
            *text_roles
                .entry(
                    source.provenance["content_role"]
                        .as_str()
                        .unwrap_or("license_or_notice")
                        .to_owned(),
                )
                .or_default() += 1;
            let path = format!("third-party/licenses/{name}-{version}/{}", source.path);
            license_files.push(json!({"path":path,"sha256":job_harness_core::storage::digest(&source.bytes),"bytes":source.bytes.len(),"provenance":source.provenance}));
            total_notice_bytes += source.bytes.len();
            ensure!(
                total_notice_bytes <= 64 * 1024 * 1024 && files.len() < 4096,
                "Collected dependency notices exceed staging bounds"
            );
            files.push((path, source.bytes));
        }
        if license_files.is_empty() {
            missing
                .push(json!({"name":name,"version":version,"declared_license":package["license"]}));
        }
        notice.push_str(&format!(
            "## {name} {version}\n\nDeclared license: {}\n\n",
            package["license"].as_str().unwrap_or("not supplied")
        ));
        for link in &links {
            notice.push_str(&format!("- {link}\n"));
        }
        if license_files.is_empty() {
            notice.push_str("\nNo license text was found in available package sources; this gap is recorded in the inventory.\n");
        } else {
            for file in &license_files {
                notice.push_str(&format!(
                    "- [{}]({})",
                    file["path"].as_str().unwrap_or_default(),
                    file["path"].as_str().unwrap_or_default()
                ));
                if let Some(url) = file["provenance"]["upstream_url"]
                    .as_str()
                    .or_else(|| file["provenance"]["source_url"].as_str())
                {
                    notice.push_str(&format!(" — [recorded source]({url})"));
                }
                notice.push('\n');
            }
        }
        notice.push('\n');
        rows.push(json!({"name":name,"version":version,"source_kind":source_kind,"declared_license":package["license"],"links":links,"license_files":license_files}));
    }
    let inventory = json!({"schema_version":1,"component":package_name,"target":target,"scope":"offline Cargo normal/build resolved closure; dev-only excluded; workspace feature unification can overinclude code paths","packages":rows,"coverage":{"resolved_packages":rows.len(),"packages_with_collected_text":rows.len()-missing.len(),"packages_without_collected_text":missing,"license_text_files":files.len(),"collected_text_roles":text_roles},"optional_model_weights_included":false,"optional_model_runtime_libraries_included":false,"legal_obligations_assessed":false});
    files.push(("THIRD_PARTY_NOTICES.md".into(), notice.into_bytes()));
    Ok(Notices { inventory, files })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn license_closure_excludes_dev_only_edges_and_keeps_build_dependencies() {
        let metadata = json!({"packages":[{"name":"root","id":"root"}],"resolve":{"nodes":[{"id":"root","deps":[{"pkg":"normal","dep_kinds":[{"kind":null}]},{"pkg":"builder","dep_kinds":[{"kind":"build"}]},{"pkg":"dev","dep_kinds":[{"kind":"dev"}]}]},{"id":"normal","deps":[]},{"id":"builder","deps":[]},{"id":"dev","deps":[]}]}});
        assert_eq!(
            closure(&metadata, "root").unwrap(),
            BTreeSet::from(["root".into(), "normal".into(), "builder".into()])
        );
    }
    #[test]
    fn notice_links_remove_credentials_host_paths_and_query_material() {
        assert!(public_link("https://token@example.test/project").is_none());
        assert_eq!(
            public_link("https://example.test/project?private=ignored#section"),
            Some("https://example.test/project".into())
        );
        assert!(license_name(Path::new("LICENSE-APACHE")));
        assert!(!license_name(Path::new("LICENSE.dll")));
    }
    #[test]
    fn cached_notices_require_exact_published_identity_commit_and_content() {
        let root = tempfile::tempdir().unwrap();
        let dependency = root.path().join("dependency");
        let cache = root.path().join("resources/licenses/text/example-1.0.0");
        fs::create_dir_all(&dependency).unwrap();
        fs::create_dir_all(&cache).unwrap();
        let commit = "1234567890123456789012345678901234567890";
        let vcs = json!({"git":{"sha1":commit},"path_in_vcs":"crates/example"});
        fs::write(dependency.join(".cargo_vcs_info.json"), vcs.to_string()).unwrap();
        let data = b"Exact unchanged upstream license text\n";
        fs::write(cache.join("LICENSE"), data).unwrap();
        let mut index = json!({"schema_version":1,"packages":[{"name":"example","version":"1.0.0","declared_license":"MIT","repository":"https://github.com/example/project","commit":commit,"path_in_vcs":"crates/example","texts":[{"path":"text/example-1.0.0/LICENSE","upstream_path":"LICENSE","sha256":job_harness_core::storage::digest(data),"bytes":data.len()}]}]});
        let index_path = root.path().join("resources/licenses/index.json");
        fs::write(&index_path, index.to_string()).unwrap();
        let package = json!({"name":"example","version":"1.0.0","source":"registry+https://github.com/rust-lang/crates.io-index","license":"MIT","repository":"https://github.com/example/project/"});
        let result = pinned_sources(root.path(), &package, &dependency).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bytes, data);
        assert_eq!(result[0].provenance["commit"], commit);
        assert_eq!(result[0].provenance["kind"], "pinned_upstream");
        fs::write(cache.join("LICENSE"), b"different license\n").unwrap();
        assert!(pinned_sources(root.path(), &package, &dependency).is_err());
        fs::write(cache.join("LICENSE"), data).unwrap();
        fs::write(
            dependency.join(".cargo_vcs_info.json"),
            "{\"git\":{\"sha1\":\"wrong\"}}",
        )
        .unwrap();
        assert!(pinned_sources(root.path(), &package, &dependency).is_err());
        fs::write(dependency.join(".cargo_vcs_info.json"), vcs.to_string()).unwrap();
        let mut changed = package.clone();
        changed["license"] = json!("Apache-2.0");
        assert!(pinned_sources(root.path(), &changed, &dependency).is_err());
        changed["version"] = json!("2.0.0");
        assert!(
            pinned_sources(root.path(), &changed, &dependency)
                .unwrap()
                .is_empty()
        );
        index["packages"][0]["texts"][0]["path"] = json!("text/example-1.0.0/../LICENSE");
        fs::write(index_path, index.to_string()).unwrap();
        assert!(pinned_sources(root.path(), &package, &dependency).is_err());
    }
    #[test]
    fn standard_license_body_requires_a_verified_exact_source_header() {
        let tmp = tempfile::tempdir().unwrap();
        let directory = tmp.path().canonicalize().unwrap();
        let declaration = b"// License: MPL-2.0; https://mozilla.org/MPL/2.0/\n";
        let source = [declaration.as_slice(), b"pub fn source_code() {}\n"].concat();
        fs::write(directory.join("lib.rs"), &source).unwrap();
        let pin = PinnedPackage {
            name: "example".into(),
            version: "1.0.0".into(),
            declared_license: "MPL-2.0".into(),
            repository: "https://github.com/example/project".into(),
            commit: "1234567890123456789012345678901234567890".into(),
            path_in_vcs: "crates/example".into(),
            texts: Vec::new(),
        };
        let header = PinnedText {
            path: "text/example-1.0.0/NOTICE.txt".into(),
            upstream_path: Some("crates/example/lib.rs".into()),
            sha256: job_harness_core::storage::digest(declaration),
            bytes: declaration.len() as u64,
            content_role: Some("source_license_header".into()),
            source_excerpt: Some(SourceExcerpt {
                path: "lib.rs".into(),
                sha256: job_harness_core::storage::digest(&source),
                start_line: 1,
                end_line: 1,
            }),
            standard_license: None,
        };
        let declaration_provenance =
            pin_provenance(&pin, &header, &directory, declaration, &[]).unwrap();
        assert_eq!(declaration_provenance["source_excerpt"]["start_line"], 1);
        assert!(pin_provenance(&pin, &header, &directory, b"changed declaration\n", &[]).is_err());
        let prior = vec![LicenseSource {
            path: "NOTICE.txt".into(),
            bytes: declaration.to_vec(),
            provenance: declaration_provenance,
        }];
        let standard = PinnedText {
            path: "text/example-1.0.0/LICENSE-MPL.txt".into(),
            upstream_path: None,
            sha256: String::new(),
            bytes: 0,
            content_role: Some("standard_license".into()),
            source_excerpt: None,
            standard_license: Some(StandardLicense {
                id: "MPL-2.0".into(),
                source_url: "https://www.mozilla.org/media/MPL/2.0/pinned.txt".into(),
                referenced_url: "https://mozilla.org/MPL/2.0/".into(),
            }),
        };
        assert!(pin_provenance(&pin, &standard, &directory, b"body", &[]).is_err());
        let provenance = pin_provenance(&pin, &standard, &directory, b"body", &prior).unwrap();
        assert_eq!(provenance["kind"], "standard_license");
        assert!(provenance.get("commit").is_none());
        let mut changed = source;
        changed.push(b'x');
        fs::write(directory.join("lib.rs"), changed).unwrap();
        assert!(pin_provenance(&pin, &header, &directory, declaration, &[]).is_err());
    }
}
