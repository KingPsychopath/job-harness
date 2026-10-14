//! Software-only release archives. Private workspaces and model weights cannot enter.
use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, GzBuilder};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::Command,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum Component {
    Core,
    Local,
    Desktop,
}
impl Component {
    pub(crate) fn executable(self) -> &'static str {
        match self {
            Self::Core => "job-harness",
            Self::Local => "job-harness-local",
            Self::Desktop => "job-harness-desktop",
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Local => "local",
            Self::Desktop => "desktop",
        }
    }
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(arguments)
        .current_dir(root)
        .output()?;
    ensure!(
        output.status.success(),
        "Release source metadata is unavailable"
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn component_version(manifest: &toml::Value, workspace_version: &str) -> Result<String> {
    let version = manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .context("Release component version is missing")?;
    if let Some(version) = version.as_str() {
        ensure!(
            version == workspace_version,
            "Release component version differs from the workspace release tag"
        );
        Ok(version.to_owned())
    } else {
        ensure!(
            version.get("workspace").and_then(toml::Value::as_bool) == Some(true),
            "Release component must declare or inherit its package version"
        );
        Ok(workspace_version.to_owned())
    }
}
fn binary_identity(path: &Path, target: &str) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut header = [0; 4096];
    let read = file.read(&mut header)?;
    let expected_arm = target.starts_with("aarch64-");
    if target.contains("-windows-") {
        ensure!(
            read >= 64 && header[..2] == *b"MZ",
            "Windows release requires a native PE executable"
        );
        let offset = u32::from_le_bytes(header[60..64].try_into()?) as usize;
        ensure!(
            offset.checked_add(6).is_some_and(|end| end <= read)
                && header[offset..offset + 4] == *b"PE\0\0",
            "Invalid PE executable header"
        );
        ensure!(
            u16::from_le_bytes(header[offset + 4..offset + 6].try_into()?)
                == if expected_arm { 0xaa64 } else { 0x8664 },
            "PE architecture differs from release target"
        );
    } else if target.contains("-linux-") {
        ensure!(
            read >= 20 && header[..4] == *b"\x7fELF" && header[4] == 2 && header[5] == 1,
            "Linux release requires a 64-bit little-endian ELF executable"
        );
        ensure!(
            u16::from_le_bytes(header[18..20].try_into()?) == if expected_arm { 183 } else { 62 },
            "ELF architecture differs from release target"
        );
    } else if target.ends_with("-apple-darwin") {
        ensure!(
            read >= 8 && header[..4] == [0xcf, 0xfa, 0xed, 0xfe],
            "macOS release requires a 64-bit Mach-O executable"
        );
        ensure!(
            u32::from_le_bytes(header[4..8].try_into()?)
                == if expected_arm { 0x0100000c } else { 0x01000007 },
            "Mach-O architecture differs from release target"
        );
    } else {
        bail!("Supported release targets are native Windows, Linux or macOS");
    }
    Ok(())
}
fn macos_app(version: &str, binary: Vec<u8>, icon: Vec<u8>) -> Result<Vec<(String, Vec<u8>, u32)>> {
    ensure!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|segment| !segment.is_empty()
                    && segment.bytes().all(|byte| byte.is_ascii_digit())),
        "macOS app metadata requires a numeric three-part release version"
    );
    ensure!(
        icon.len() >= 16
            && icon[..4] == *b"icns"
            && u32::from_be_bytes(icon[4..8].try_into()?) as usize == icon.len(),
        "macOS application requires the reviewed ICNS icon with a valid header"
    );
    let mut offset = 8;
    while offset < icon.len() {
        ensure!(offset + 8 <= icon.len(), "Incomplete ICNS icon element");
        let size = u32::from_be_bytes(icon[offset + 4..offset + 8].try_into()?) as usize;
        ensure!(
            size >= 8
                && offset
                    .checked_add(size)
                    .is_some_and(|end| end <= icon.len()),
            "Invalid ICNS icon element length"
        );
        offset += size;
    }
    let info = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Job Harness</string>
  <key>CFBundleExecutable</key><string>job-harness-desktop</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundleIdentifier</key><string>dev.jobharness.desktop</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Job Harness</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
"#
    );
    Ok(vec![
        (
            "Job Harness.app/Contents/MacOS/job-harness-desktop".into(),
            binary,
            0o755,
        ),
        (
            "Job Harness.app/Contents/Info.plist".into(),
            info.into_bytes(),
            0o644,
        ),
        (
            "Job Harness.app/Contents/Resources/icon.icns".into(),
            icon,
            0o644,
        ),
    ])
}
fn installation(component: Component, windows: bool, macos: bool) -> String {
    let documentation = "Documentation: https://github.com/machineyearningengineer/job-harness";
    match component {
        Component::Core => {
            let directory = if windows {
                "$HOME\\Career\\workspace"
            } else {
                "$HOME/Career/workspace"
            };
            let shell = if windows { "powershell" } else { "sh" };
            let programs = if windows {
                "job-harness.exe and jh.exe"
            } else {
                "job-harness and jh"
            };
            format!(
                "# Job Harness\n\nCopy {programs} to a user-owned directory on PATH. Then run:\n\n```{shell}\njh setup --workspace \"{directory}\"\njh status\njh connect mcp\n```\n\nSetup offers an optional capability plan. It never silently downloads software or grants external access. Runtime needs no Python, Rust, VM or model key. Private workspaces stay outside this software directory.\n\n{documentation}\n"
            )
        }
        Component::Local => format!(
            "# Optional local model provider\n\nInstall job-harness-local separately and run `job-harness-local --help` for its current commands. This archive contains no weights or vendor native libraries. Choose and install a supported native model runtime and approved weights explicitly; core career operations remain independent.\n\n{documentation}/blob/main/docs/MODELS.md\n"
        ),
        Component::Desktop if macos => format!(
            "# Job Harness for macOS\n\nCopy Job Harness.app to Applications and open it. Choose an existing private workspace when the window asks. Private workspace data stays outside this archive.\n\nThis preview has no Developer ID signature or notarization. Verify its archive SHA256 and use your normal macOS approval process before opening it. The app grants no browser, mail, calendar or model credentials.\n\n{documentation}/blob/main/apps/desktop/README.md\n"
        ),
        Component::Desktop => format!(
            "# Optional desktop window\n\nInstall job-harness-desktop separately. It uses the same local workspace and the platform webview; its dependencies are documented in the desktop guide. Private workspace data stays outside this archive. A window grants no browser, mail, calendar or model credentials.\n\n{documentation}/blob/main/apps/desktop/README.md\n"
        ),
    }
}

fn append_tar(
    archive: &mut tar::Builder<flate2::write::GzEncoder<fs::File>>,
    name: &str,
    bytes: &[u8],
    mode: u32,
    epoch: u64,
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_mtime(epoch);
    header.set_uid(0);
    header.set_gid(0);
    header.set_cksum();
    archive.append_data(&mut header, name, bytes)?;
    Ok(())
}

fn archive(
    destination: &mut fs::File,
    name: &str,
    entries: &[(String, Vec<u8>, u32)],
    component: Component,
    windows: bool,
    epoch: u64,
) -> Result<()> {
    if windows {
        let mut archive = ZipWriter::new(destination);
        for (filename, bytes, mode) in entries {
            archive.start_file(
                format!("{name}/{filename}"),
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(*mode),
            )?;
            archive.write_all(bytes)?;
        }
        archive.finish()?;
    } else {
        let file = destination.try_clone()?;
        let encoder = GzBuilder::new()
            .mtime(epoch.try_into().unwrap_or(u32::MAX))
            .write(file, Compression::default());
        let mut archive = tar::Builder::new(encoder);
        for (filename, bytes, mode) in entries {
            append_tar(
                &mut archive,
                &format!("{name}/{filename}"),
                bytes,
                *mode,
                epoch,
            )?;
        }
        if component == Component::Core {
            let mut alias = tar::Header::new_gnu();
            alias.set_entry_type(tar::EntryType::Symlink);
            alias.set_size(0);
            alias.set_mode(0o755);
            alias.set_mtime(epoch);
            alias.set_uid(0);
            alias.set_gid(0);
            alias.set_link_name("job-harness")?;
            alias.set_cksum();
            archive.append_data(&mut alias, format!("{name}/jh"), std::io::empty())?;
        }
        archive.into_inner()?.finish()?.sync_all()?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn stage(
    root: &Path,
    component: Component,
    tag: &str,
    target: &str,
    binary: &Path,
    alias: Option<&Path>,
    output: &Path,
    allow_dirty: bool,
) -> Result<Value> {
    ensure!(
        (target.starts_with("x86_64-") || target.starts_with("aarch64-"))
            && target.bytes().all(|ch| ch.is_ascii_lowercase()
                || ch.is_ascii_digit()
                || ch == b'-'
                || ch == b'_'),
        "Use a supported native target triple"
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml::Value>()?;
    let version = manifest
        .get("workspace")
        .and_then(|value| value.get("package"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .context("Workspace version missing")?;
    ensure!(
        tag == format!("v{version}"),
        "Release tag must match the package version"
    );
    let manifest_path = root.join(match component {
        Component::Core => "crates/harness-cli/Cargo.toml",
        Component::Local => "providers/local/native/Cargo.toml",
        Component::Desktop => "apps/desktop/Cargo.toml",
    });
    component_version(
        &fs::read_to_string(&manifest_path)?.parse::<toml::Value>()?,
        version,
    )?;
    let source_commit = git(root, &["rev-parse", "HEAD"])?;
    let source_dirty =
        !git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty();
    ensure!(
        allow_dirty || !source_dirty,
        "Release source is dirty; commit it first or use --allow-dirty for a clearly marked local experiment"
    );
    let epoch = git(root, &["show", "-s", "--format=%ct", "HEAD"])?.parse::<u64>()?;
    let toolchain = fs::read_to_string(root.join("rust-toolchain.toml"))?.parse::<toml::Value>()?;
    let rust = toolchain
        .get("toolchain")
        .and_then(|value| value.get("channel"))
        .and_then(toml::Value::as_str)
        .context("Pinned Rust toolchain missing")?;
    let binary = binary.canonicalize()?;
    let expected_executable = format!(
        "{}{}",
        component.executable(),
        if target.contains("-windows-") {
            ".exe"
        } else {
            ""
        }
    );
    ensure!(
        binary.file_name().and_then(|name| name.to_str()) == Some(expected_executable.as_str()),
        "Selected binary filename differs from the requested release component"
    );
    binary_identity(&binary, target)?;
    let windows = target.contains("-windows-");
    let macos = target.ends_with("-apple-darwin");
    ensure!(
        component == Component::Core || alias.is_none(),
        "Only the core component has a jh alias"
    );
    let alias = if windows && component == Component::Core {
        Some(
            alias
                .map(Path::to_owned)
                .unwrap_or_else(|| binary.with_file_name("jh.exe"))
                .canonicalize()?,
        )
    } else {
        None
    };
    if let Some(alias) = &alias {
        binary_identity(alias, target)?;
    }
    let name = format!("{}-{tag}-{target}", component.executable());
    let output = if output.is_absolute() {
        output.to_owned()
    } else {
        root.join(output)
    };
    fs::create_dir_all(&output)?;
    let extension = if windows { ".zip" } else { ".tar.gz" };
    let archive_path = output.join(format!("{name}{extension}"));
    let checksum_path = output.join(format!("{name}{extension}.sha256"));
    ensure!(
        !archive_path.exists() && !checksum_path.exists(),
        "Release outputs already exist; do not overwrite reviewed artifacts"
    );
    let binary_bytes = fs::read(&binary)?;
    let executable_name = format!(
        "{}{}",
        component.executable(),
        if windows { ".exe" } else { "" }
    );
    let mut entries = if component == Component::Desktop && macos {
        macos_app(
            version,
            binary_bytes,
            fs::read(root.join("apps/desktop/icons/icon.icns"))?,
        )?
    } else {
        vec![(executable_name, binary_bytes, 0o755)]
    };
    if let Some(alias) = alias {
        entries.push(("jh.exe".into(), fs::read(alias)?, 0o755));
    }
    entries.push(("LICENSE".into(), fs::read(root.join("LICENSE"))?, 0o644));
    entries.push((
        "INSTALL.md".into(),
        installation(component, windows, macos).as_bytes().to_vec(),
        0o644,
    ));
    let mut build = json!({"component":component.name(),"version":version,"target":target,"source_commit":source_commit,"source_dirty":source_dirty,"rust":rust,"release_channel":"private","remote_visibility_checked":false,"binary_sha256":job_harness_core::storage::digest(&entries[0].1),"software_only":true});
    if component == Component::Desktop && macos {
        build["macos_app_bundle"] = json!(true);
        build["developer_id_signed"] = json!(false);
        build["notarized"] = json!(false);
    }
    entries.push((
        "BUILD.json".into(),
        job_harness_core::storage::encoded(&build),
        0o644,
    ));
    let notices = crate::licenses::collect(root, &manifest_path, target, component.executable())?;
    entries.push((
        "THIRD_PARTY.json".into(),
        job_harness_core::storage::encoded(&notices.inventory),
        0o644,
    ));
    entries.extend(
        notices
            .files
            .into_iter()
            .map(|(name, bytes)| (name, bytes, 0o644)),
    );
    let license_coverage = notices.inventory["coverage"].clone();
    let mut temporary = tempfile::Builder::new()
        .prefix(".release-")
        .tempfile_in(&output)?;
    archive(
        temporary.as_file_mut(),
        &name,
        &entries,
        component,
        windows,
        epoch,
    )?;
    temporary.as_file().sync_all()?;
    let bytes = fs::read(temporary.path())?;
    let checksum = job_harness_core::storage::digest(&bytes);
    temporary
        .persist_noclobber(&archive_path)
        .map_err(|_| anyhow::anyhow!("Release output changed before publication"))?;
    fs::write(
        &checksum_path,
        format!(
            "{checksum}  {}\n",
            archive_path
                .file_name()
                .and_then(|name| name.to_str())
                .context("Invalid archive filename")?
        ),
    )?;
    Ok(
        json!({"ok":true,"task":"release-stage","archive":archive_path,"checksum_file":checksum_path,"sha256":checksum,"build":build,"license_coverage":license_coverage,"files":entries.len()+usize::from(!windows&&component==Component::Core),"published":false}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn component_versions_cannot_silently_diverge_from_the_release_tag() {
        let inherited: toml::Value = "[package]\nversion.workspace=true\n".parse().unwrap();
        assert_eq!(component_version(&inherited, "0.2.0").unwrap(), "0.2.0");
        let explicit: toml::Value = "[package]\nversion='0.2.0'\n".parse().unwrap();
        assert_eq!(component_version(&explicit, "0.2.0").unwrap(), "0.2.0");
        assert!(component_version(&explicit, "0.3.0").is_err());
        let absent: toml::Value = "[package]\nname='component'\n".parse().unwrap();
        assert!(component_version(&absent, "0.2.0").is_err());
    }
    #[test]
    fn release_binary_identity_checks_target_architecture_and_os() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("binary");
        let mut bytes = vec![0; 64];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        binary_identity(&path, "x86_64-unknown-linux-gnu").unwrap();
        assert!(binary_identity(&path, "aarch64-unknown-linux-gnu").is_err());
        assert!(binary_identity(&path, "x86_64-pc-windows-msvc").is_err());
    }
    #[test]
    fn archives_preserve_only_named_software_and_explicit_core_aliases() {
        for component in [Component::Core, Component::Local, Component::Desktop] {
            for windows in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("archive");
                let executable = format!(
                    "{}{}",
                    component.executable(),
                    if windows { ".exe" } else { "" }
                );
                let mut entries = vec![
                    (
                        executable.clone(),
                        b"fictional software bytes".to_vec(),
                        0o755,
                    ),
                    ("LICENSE".into(), b"fictional license".to_vec(), 0o644),
                    (
                        "INSTALL.md".into(),
                        installation(component, windows, false).into_bytes(),
                        0o644,
                    ),
                    (
                        "BUILD.json".into(),
                        b"{\"software_only\":true}\n".to_vec(),
                        0o644,
                    ),
                ];
                if windows && component == Component::Core {
                    entries.push(("jh.exe".into(), b"fictional alias bytes".to_vec(), 0o755));
                }
                archive(
                    &mut fs::File::create(&path).unwrap(),
                    "release",
                    &entries,
                    component,
                    windows,
                    1_000_000,
                )
                .unwrap();
                let mut names = std::collections::BTreeSet::new();
                if windows {
                    let mut zipped = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
                    for index in 0..zipped.len() {
                        let entry = zipped.by_index(index).unwrap();
                        names.insert(entry.name().to_owned());
                    }
                } else {
                    let decoded = flate2::read::GzDecoder::new(fs::File::open(&path).unwrap());
                    let mut tar = tar::Archive::new(decoded);
                    for entry in tar.entries().unwrap() {
                        let entry = entry.unwrap();
                        let path = entry.path().unwrap().to_string_lossy().into_owned();
                        assert_eq!(entry.header().mtime().unwrap(), 1_000_000);
                        assert_eq!(entry.header().uid().unwrap(), 0);
                        if path.ends_with("/jh") {
                            assert_eq!(entry.header().entry_type(), tar::EntryType::Symlink);
                            assert_eq!(
                                entry.link_name().unwrap().unwrap(),
                                Path::new("job-harness")
                            );
                        }
                        names.insert(path);
                    }
                }
                let mut expected: std::collections::BTreeSet<_> = entries
                    .iter()
                    .map(|(name, _, _)| format!("release/{name}"))
                    .collect();
                if !windows && component == Component::Core {
                    expected.insert("release/jh".into());
                }
                assert_eq!(names, expected);
                assert_eq!(
                    names.len(),
                    if component == Component::Core { 5 } else { 4 }
                );
            }
        }
    }
    #[test]
    fn stable_software_inputs_and_epoch_produce_reproducible_archives() {
        for windows in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let first = root.path().join("first");
            let second = root.path().join("second");
            let entries = vec![
                ("job-harness".into(), b"same software bytes".to_vec(), 0o755),
                (
                    "BUILD.json".into(),
                    b"{\"source_commit\":\"fixture\"}\n".to_vec(),
                    0o644,
                ),
            ];
            for path in [&first, &second] {
                archive(
                    &mut fs::File::create(path).unwrap(),
                    "release",
                    &entries,
                    Component::Core,
                    windows,
                    1_000_000,
                )
                .unwrap();
            }
            assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
        }
    }
    #[test]
    fn core_install_uses_capability_setup_and_standalone_components_stay_optional() {
        assert!(
            installation(Component::Core, true, false)
                .contains(r#"jh setup --workspace "$HOME\Career\workspace""#)
        );
        assert!(installation(Component::Core, false, false).contains("jh status"));
        assert!(
            installation(Component::Local, false, false)
                .contains("no weights or vendor native libraries")
        );
        assert!(installation(Component::Desktop, false, false).contains("platform webview"));
    }
    #[test]
    fn macos_desktop_archive_has_one_executable_inside_a_real_app_bundle() {
        let mut icon = b"icns".to_vec();
        icon.extend(16u32.to_be_bytes());
        icon.extend(b"icp4");
        icon.extend(8u32.to_be_bytes());
        let binary = b"reviewed native executable".to_vec();
        let entries = macos_app("0.2.0", binary.clone(), icon.clone()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("desktop.tar.gz");
        archive(
            &mut fs::File::create(&path).unwrap(),
            "desktop-release",
            &entries,
            Component::Desktop,
            false,
            1_000_000,
        )
        .unwrap();
        let mut tar =
            tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(path).unwrap()));
        let mut names = BTreeMap::new();
        for entry in tar.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let mode = entry.header().mode().unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            names.insert(name, (mode, bytes));
        }
        assert_eq!(names.len(), 3);
        assert_eq!(
            names["desktop-release/Job Harness.app/Contents/MacOS/job-harness-desktop"],
            (0o755, binary)
        );
        assert_eq!(
            names["desktop-release/Job Harness.app/Contents/Resources/icon.icns"],
            (0o644, icon)
        );
        let info = String::from_utf8(
            names["desktop-release/Job Harness.app/Contents/Info.plist"]
                .1
                .clone(),
        )
        .unwrap();
        roxmltree::Document::parse_with_options(
            &info,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(info.contains("<key>CFBundleExecutable</key><string>job-harness-desktop</string>"));
        assert!(info.contains("<key>CFBundleShortVersionString</key><string>0.2.0</string>"));
        assert!(
            info.contains("<key>CFBundleIdentifier</key><string>dev.jobharness.desktop</string>")
        );
        assert!(info.contains("<key>NSHighResolutionCapable</key><true/>"));
        assert!(!names.contains_key("desktop-release/job-harness-desktop"));
        assert!(!names.contains_key("desktop-release/jh"));
        assert!(
            installation(Component::Desktop, false, true)
                .contains("Copy Job Harness.app to Applications and open it")
        );
        assert!(macos_app("0.2.0", Vec::new(), b"wrong icon".to_vec()).is_err());
        assert!(macos_app("0.2.0-preview", Vec::new(), b"icns\0\0\0\x08".to_vec()).is_err());
    }
}
