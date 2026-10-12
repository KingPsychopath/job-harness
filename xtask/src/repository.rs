//! Audit only Git's tracked/non-ignored source inventory, never target or scratch.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const ALLOWED: &[&str] = &[
    ".agents",
    ".github",
    ".cargo",
    ".gitattributes",
    ".gitignore",
    "AGENTS.md",
    "README.md",
    "LICENSE",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "CHANGELOG.md",
    "Makefile",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "crates",
    "apps",
    "xtask",
    "resources",
    "gui",
    "examples",
    "providers",
    "docs",
    "scripts",
    "tests",
];
const FORBIDDEN_EXTENSIONS: &[&str] = &[
    "jhb",
    "bundle",
    "sqlite",
    "sqlite3",
    "db",
    "py",
    "pyc",
    "pyo",
    "task",
    "tflite",
    "safetensors",
    "litertlm",
    "onnx",
    "gguf",
    "pt",
    "pth",
    "bin",
    "so",
    "dll",
    "dylib",
    "a",
    "lib",
    "exe",
    "zip",
    "gz",
    "tgz",
    "whl",
    "jar",
    "class",
    "o",
    "obj",
    "pdb",
];
fn inventory(root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()
        .context("Read generic source inventory with Git")?;
    ensure!(output.status.success(), "Git source inventory failed");
    let mut paths = BTreeSet::new();
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let relative = std::str::from_utf8(bytes).context("Generic source paths must be UTF-8")?;
        let path = root.join(relative);
        if fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() || metadata.file_type().is_symlink())
        {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}
pub(crate) fn private_content(data: &[u8]) -> bool {
    // Construct scanner needles instead of excluding this guard's own source.
    let patterns = [
        ["/", "Users/"].concat(),
        ["-----BEGIN ", "PRIVATE KEY-----"].concat(),
        ["-----BEGIN ", "OPENSSH PRIVATE KEY-----"].concat(),
    ];
    patterns.iter().any(|pattern| {
        data.windows(pattern.len())
            .any(|window| window == pattern.as_bytes())
    })
}
fn audit_file(root: &Path, path: &Path, links: &mut usize) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    let first = relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .context("Invalid source path")?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        ALLOWED.contains(&first) && !FORBIDDEN_EXTENSIONS.contains(&extension.as_str()),
        "Private/runtime file in source: {}",
        relative.display()
    );
    ensure!(
        extension != "pdf" || relative == Path::new("examples/cv/example.pdf"),
        "Only the audited fictional CV PDF belongs in source: {}",
        relative.display()
    );
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Generic source requires regular portable files: {}",
        relative.display()
    );
    let data = fs::read(path)?;
    ensure!(
        !private_content(&data),
        "Private path/key content in generic source: {}",
        relative.display()
    );
    ensure!(
        !data.starts_with(b"\x7fELF")
            && !data.starts_with(b"MZ")
            && ![
                b"\xcf\xfa\xed\xfe".as_slice(),
                b"\xce\xfa\xed\xfe".as_slice(),
                b"\xfe\xed\xfa\xcf".as_slice(),
                b"\xfe\xed\xfa\xce".as_slice()
            ]
            .iter()
            .any(|magic| data.starts_with(magic)),
        "Native executable/library in source: {}",
        relative.display()
    );
    if extension == "svg" {
        roxmltree::Document::parse(std::str::from_utf8(&data)?)
            .with_context(|| format!("Invalid SVG: {}", relative.display()))?;
    }
    if extension == "json" {
        serde_json::from_slice::<Value>(&data)
            .with_context(|| format!("Invalid JSON: {}", relative.display()))?;
    }
    if extension == "md" {
        let text = std::str::from_utf8(&data)?;
        let link_pattern = Regex::new(r"\]\(([^)]+)\)")?;
        let scheme = Regex::new(r"^[a-z][a-z0-9+.-]*:")?;
        for capture in link_pattern.captures_iter(text) {
            let target = capture[1]
                .split('#')
                .next()
                .unwrap_or("")
                .trim_matches(['<', '>']);
            if target.is_empty() || scheme.is_match(target) {
                continue;
            }
            ensure!(
                path.parent()
                    .context("Source needs a parent")?
                    .join(target)
                    .exists(),
                "Broken local link in {}: {target}",
                relative.display()
            );
            *links += 1;
        }
        if path.file_name().is_some_and(|name| name == "SKILL.md")
            && (relative.starts_with("resources/skills") || relative.starts_with(".agents/skills"))
        {
            let mut parts = text.splitn(3, "---");
            ensure!(
                parts.next().is_some_and(|before| before.trim().is_empty()),
                "Missing skill frontmatter: {}",
                relative.display()
            );
            let front = parts.next().context("Missing skill frontmatter")?;
            ensure!(
                parts.next().is_some(),
                "Missing skill frontmatter ending: {}",
                relative.display()
            );
            let name = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .context("Skill name must be UTF-8")?;
            ensure!(
                Regex::new(&format!(r"(?m)^name:\s*{}\s*$", regex::escape(name)))?.is_match(front)
                    && Regex::new(r"(?m)^description:\s*\S")?.is_match(front),
                "Invalid skill metadata: {name}"
            );
        }
    }
    Ok(())
}
pub fn check(root: &Path) -> Result<Value> {
    let paths = inventory(root)?;
    let mut links = 0;
    let mut skills = 0;
    for path in &paths {
        audit_file(root, path, &mut links)?;
        if path.file_name().is_some_and(|name| name == "SKILL.md") {
            skills += 1;
        }
    }
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml::Value>()?;
    ensure!(
        manifest
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("publish"))
            .and_then(toml::Value::as_bool)
            == Some(false),
        "Public publication must stay disabled"
    );
    if paths.is_empty() {
        bail!("No generic source files were checked");
    }
    Ok(
        json!({"ok":true,"task":"repository","files":paths.len(),"links":links,"skills":skills,"checks":["Git source inventory excludes ignored build/scratch","local links","SVG and JSON parsing","focused skill frontmatter","private paths/key markers","runtime weights/binaries excluded","publication disabled"]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guard_checks_its_own_source_without_a_privacy_exemption() {
        assert!(!private_content(include_bytes!("repository.rs")));
        assert!(private_content(
            ["/", "Users/fictional-person/workspace"]
                .concat()
                .as_bytes()
        ));
        assert!(private_content(
            ["-----BEGIN ", "PRIVATE KEY-----"].concat().as_bytes()
        ));
    }
    #[test]
    fn runtime_files_and_broken_links_fail_without_reading_private_roots() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("docs")).unwrap();
        let path = root.path().join("docs/guide.md");
        fs::write(&path, "[missing](missing.md)").unwrap();
        assert!(audit_file(root.path(), &path, &mut 0).is_err());
        fs::write(&path, "[remote](https://example.test/source)\\n").unwrap();
        audit_file(root.path(), &path, &mut 0).unwrap();
        let weights = root.path().join("docs/model.safetensors");
        fs::write(&weights, b"unread model placeholder").unwrap();
        assert!(audit_file(root.path(), &weights, &mut 0).is_err());
        let source = root.path().join("docs/not-a-model.rs");
        fs::write(&source, b"MZnative executable placeholder").unwrap();
        assert!(audit_file(root.path(), &source, &mut 0).is_err());
    }
}
