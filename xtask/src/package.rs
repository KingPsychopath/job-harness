//! Verify the installed artifact in an isolated fictional workspace with no SDK keys.
use anyhow::{Context, Result, bail, ensure};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation},
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_STDOUT: u64 = 8 * 1024 * 1024;
const MAX_STDERR: u64 = 64 * 1024;
struct Installed {
    executable: PathBuf,
    base: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    workspace_scoped: bool,
}
struct Output {
    status: ExitStatus,
    stdout: Vec<u8>,
}
impl Installed {
    fn command(&self, root: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new(&self.executable);
        if self.workspace_scoped {
            command.arg("--root").arg(root);
        }
        command
            .args(arguments)
            .current_dir(&self.base)
            .env_clear()
            .envs(&self.environment)
            .kill_on_drop(true);
        command
    }
    async fn output(
        &self,
        root: &Path,
        arguments: &[&str],
        value: Option<&Value>,
    ) -> Result<Output> {
        let mut command = self.command(root, arguments);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("Launch copied native executable")?;
        let mut stdin = child.stdin.take().context("Native input pipe missing")?;
        let stdout = child.stdout.take().context("Native output pipe missing")?;
        let stderr = child.stderr.take().context("Native error pipe missing")?;
        let bytes = value
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();
        let outcome = timeout(COMMAND_TIMEOUT, async {
            let write = async {
                stdin.write_all(&bytes).await?;
                drop(stdin);
                Ok::<_, std::io::Error>(())
            };
            let read_out = async {
                let mut bytes = Vec::new();
                stdout.take(MAX_STDOUT + 1).read_to_end(&mut bytes).await?;
                Ok::<_, std::io::Error>(bytes)
            };
            let read_err = async {
                let mut bytes = Vec::new();
                stderr.take(MAX_STDERR + 1).read_to_end(&mut bytes).await?;
                Ok::<_, std::io::Error>(bytes)
            };
            tokio::try_join!(write, read_out, read_err, child.wait())
        })
        .await;
        match outcome {
            Ok(Ok((_, stdout, stderr, status))) => {
                ensure!(
                    stdout.len() as u64 <= MAX_STDOUT && stderr.len() as u64 <= MAX_STDERR,
                    "Native output exceeded verification bounds"
                );
                Ok(Output { status, stdout })
            }
            failure => {
                let _ = child.start_kill();
                let _ = timeout(Duration::from_secs(5), child.wait()).await;
                match failure {
                    Ok(Err(error)) => Err(error.into()),
                    _ => bail!("Native command timed out: {arguments:?}"),
                }
            }
        }
    }
    async fn run(&self, root: &Path, arguments: &[&str], value: Option<&Value>) -> Result<Value> {
        let output = self.output(root, arguments, value).await?;
        ensure!(
            output.status.success(),
            "Native command failed: {arguments:?}"
        );
        let mut result: Value = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("Native command returned invalid JSON: {arguments:?}"))?;
        if result.get("ok").is_some() {
            ensure!(
                result["schema_version"] == 2 && result["ok"] == true,
                "Native versioned envelope failed: {arguments:?}"
            );
            result = result["data"].take();
        }
        Ok(result)
    }
    async fn mcp(&self, root: &Path, profile: &str, specs: &Value) -> Result<()> {
        let mut command = self.command(root, &["mcp", "--tool-profile", profile]);
        command.stderr(Stdio::null());
        let transport = TokioChildProcess::new(command)?;
        let config = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("job-harness-artifact-verifier", env!("CARGO_PKG_VERSION")),
        );
        let service = timeout(MCP_TIMEOUT, config.serve(transport))
            .await
            .context("MCP initialize timed out")??;
        let verification = async {
            ensure!(
                service.peer_info().is_some_and(|info| info
                    .server_info
                    .as_ref()
                    .is_some_and(|info| info.name == "job-harness"
                        && info.version == env!("CARGO_PKG_VERSION"))),
                "MCP server identity differs"
            );
            let catalog = timeout(MCP_TIMEOUT, service.list_tools(None))
                .await
                .context("MCP tools/list timed out")??;
            ensure!(
                catalog.next_cursor.is_none(),
                "Artifact verifier expects a bounded complete catalog"
            );
            let expected: BTreeSet<String> = if profile == "focused" {
                job_harness_protocol::FOCUSED_TOOLS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect()
            } else {
                specs
                    .as_object()
                    .context("Native catalog must be an object")?
                    .keys()
                    .cloned()
                    .collect()
            };
            let actual: BTreeSet<String> = catalog
                .tools
                .iter()
                .map(|tool| tool.name.to_string())
                .collect();
            ensure!(
                expected == actual && actual.len() == catalog.tools.len(),
                "MCP {profile} catalog differs from the authoritative profile"
            );
            for tool in catalog.tools {
                let tool = serde_json::to_value(tool)?;
                let name = tool["name"].as_str().context("MCP tool name missing")?;
                ensure!(
                    tool["inputSchema"] == specs[name]["inputSchema"]
                        && tool["outputSchema"] == specs[name]["outputSchema"],
                    "MCP advertised schemas differ: {name}"
                );
            }
            let result = timeout(
                MCP_TIMEOUT,
                service.call_tool(
                    CallToolRequestParams::new("bootstrap").with_arguments(Default::default()),
                ),
            )
            .await
            .context("MCP bootstrap timed out")??;
            let structured = result
                .structured_content
                .as_ref()
                .context("MCP structured result missing")?;
            ensure!(
                structured["schema_version"] == 2 && structured["ok"] == true,
                "MCP bootstrap envelope failed"
            );
            job_harness_core::contracts::validate(&specs["bootstrap"]["outputSchema"], structured)?;
            let text = serde_json::to_value(&result)?;
            let text: Value = serde_json::from_str(
                text["content"][0]["text"]
                    .as_str()
                    .context("MCP text result missing")?,
            )?;
            ensure!(
                text == *structured,
                "MCP text and structured results differ"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        timeout(Duration::from_secs(5), service.cancel())
            .await
            .context("MCP cleanup timed out")??;
        verification
    }
}
fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .context("Native verification paths must be UTF-8")
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, job_harness_core::storage::encoded(value))?;
    Ok(())
}
fn installed(
    base: &Path,
    source: &Path,
    program: &str,
    workspace_scoped: bool,
) -> Result<Installed> {
    let bin = base.join("bin");
    fs::create_dir(&bin)?;
    let executable = bin.join(format!(
        "{program}{}",
        if cfg!(windows) { ".exe" } else { "" }
    ));
    fs::copy(source, &executable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    }
    let host = base.join("host");
    fs::create_dir(&host)?;
    let mut environment = BTreeMap::from([
        (OsString::from("PATH"), bin.into_os_string()),
        (OsString::from("HOME"), host.clone().into_os_string()),
        (
            OsString::from("XDG_CONFIG_HOME"),
            host.join("settings").into_os_string(),
        ),
        (OsString::from("LC_ALL"), OsString::from("C")),
    ]);
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            environment.insert(name.into(), value);
        }
    }
    #[cfg(windows)]
    {
        let temporary = base.join("tmp");
        fs::create_dir(&temporary)?;
        for name in ["TEMP", "TMP"] {
            environment.insert(name.into(), temporary.clone().into_os_string());
        }
        environment.insert("USERPROFILE".into(), host.clone().into_os_string());
        environment.insert("APPDATA".into(), host.join("AppData").into_os_string());
    }
    Ok(Installed {
        executable,
        base: base.to_owned(),
        environment,
        workspace_scoped,
    })
}
pub async fn verify(binary: &Path) -> Result<Value> {
    // Isolation belongs to each spawned Command, never std::env::set_var or
    // the invoking user's shell. Every workspace operation also has --root.
    let parent_environment: Vec<_> = ["HOME", "XDG_CONFIG_HOME", "APPDATA", "USERPROFILE"]
        .into_iter()
        .map(|name| (name, std::env::var_os(name)))
        .collect();
    let source = binary
        .canonicalize()
        .context("Select an existing native job-harness executable")?;
    ensure!(source.is_file(), "Native executable must be a file");
    let temporary = tempfile::Builder::new()
        .prefix("job-harness-native-")
        .tempdir()?;
    let base = temporary.path().canonicalize()?;
    let installed = installed(&base, &source, "job-harness", true)?;
    let root = base.join("career");
    let restored = base.join("restored");
    let mut checks = Vec::new();
    let version = installed.output(&root, &["--version"], None).await?;
    ensure!(version.status.success(), "Native version command failed");
    let version = String::from_utf8(version.stdout)?.trim().to_owned();
    ensure!(
        version == format!("job-harness {}", env!("CARGO_PKG_VERSION")),
        "Native version identity differs"
    );
    checks
        .push("copied executable with child-only PATH, HOME/XDG settings and no model credentials");
    installed.run(&root, &["init"], None).await?;
    ensure!(
        installed.run(&root, &["call", "bootstrap"], None).await?["contract_version"] == "2.0",
        "Bootstrap contract version differs"
    );
    checks.push("initialize and bootstrap outside checkout");
    let capabilities = installed
        .run(&root, &["capabilities", "--json"], None)
        .await?;
    ensure!(
        capabilities["schema_version"] == 1
            && capabilities["base"]["runtime"]
                .as_str()
                .is_some_and(|text| text.contains("no Python")),
        "Base capability metadata is missing"
    );
    let preview = base.join("setup-preview");
    let isolated_settings = base.join("host/settings/job-harness/settings.json");
    let plan = installed
        .run(
            &root,
            &[
                "setup",
                "--workspace",
                text(&preview)?,
                "--capability",
                "mcp",
                "--capability",
                "retrieval",
                "--plan",
                "--json",
            ],
            None,
        )
        .await?;
    ensure!(
        plan["downloads_started"] == false
            && plan["external_access_granted"] == false
            && plan["configured"] == false
            && !preview.exists()
            && !isolated_settings.exists(),
        "Setup plan caused filesystem/download/access effects"
    );
    let configured_root = base.join("setup-configured");
    let configured = installed
        .run(
            &root,
            &[
                "setup",
                "--workspace",
                text(&configured_root)?,
                "--capability",
                "mcp",
                "--json",
            ],
            None,
        )
        .await?;
    let settings: Value = serde_json::from_slice(&fs::read(&isolated_settings)?)?;
    ensure!(
        configured["configured"] == true
            && configured["created"] == true
            && configured["downloads_started"] == false
            && configured["external_access_granted"] == false
            && settings["workspace"] == json!(configured_root.canonicalize()?)
            && installed.run(&root, &["call", "bootstrap"], None).await?["ledger"] == json!(root),
        "Native setup did not confine host configuration to its child XDG directory or preserve explicit root precedence"
    );
    let demo_root = base.join("fictional-demo");
    let demo = installed
        .run(&root, &["demo", "--workspace", text(&demo_root)?], None)
        .await?;
    ensure!(
        demo["fictional"] == true
            && demo["external_actions"] == false
            && demo["search"]["matches"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
        "Native demo did not preserve its fictional/no-action boundary"
    );
    checks.push(
        "capability catalog, setup plan, child-only XDG configuration and native fictional demo",
    );

    let specs = installed.run(&root, &["schemas"], None).await?;
    ensure!(
        specs == Value::Object(job_harness_core::contracts::specs().clone()),
        "Copied binary tool catalog differs from current authoritative schemas"
    );
    let skill = installed
        .run(
            &root,
            &["call", "read_record"],
            Some(&json!({"path":".agents/skills/career-apply/SKILL.md"})),
        )
        .await?;
    ensure!(
        skill["origin"] == "toolkit"
            && skill["text"]
                .as_str()
                .is_some_and(|text| text.contains("Apply to roles")),
        "Embedded focused skill missing"
    );
    let observations = installed
        .run(
            &root,
            &["call", "read_record"],
            Some(&json!({"path":"docs/host-observations.md"})),
        )
        .await?;
    ensure!(
        observations["origin"] == "toolkit"
            && observations["text"]
                .as_str()
                .is_some_and(|text| text.contains("completeness_confirmed")
                    && text.contains("request_message_ids")),
        "Embedded observation contracts missing"
    );
    let workflows = installed
        .run(
            &root,
            &["call", "read_record"],
            Some(&json!({"path":"docs/workflows.md"})),
        )
        .await?;
    ensure!(
        workflows["origin"] == "toolkit"
            && workflows["text"].as_str().is_some_and(|text| [
                "Building experience",
                "Exploring a move",
                "Searching",
                "Starting somewhere"
            ]
            .iter()
            .all(|heading| text.contains(heading))),
        "Embedded workflow playbooks missing"
    );
    checks.push("embedded schemas, focused skill, observations and playbooks");
    let pdf = b"%PDF-1.4\nSynthetic native-package fixture, never an actual CV.\n%%EOF\n";
    fs::write(root.join("output/primary-cv.pdf"), pdf)?;
    let config_path = root.join("config/career-ledger.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path)?)?;
    config["primary_cv"] = json!("output/primary-cv.pdf");
    write_json(&config_path, &config)?;
    let record = root.join("applications/example.md");
    let tracking = json!({"stage":"saved","checked":"2026-10-12","company":"Example Company","vacancy_url":"https://example.com/roles/demo","source":"Synthetic verification","next_action":"Review synthetic vacancy"});
    fs::write(
        &record,
        format!(
            "# Example Company — Engineer\n\n<!-- tracking\n{}\n-->\n\nSynthetic record only.\n",
            serde_json::to_string(&tracking)?
        ),
    )?;
    ensure!(
        installed.run(&root, &["workspace", "index"], None).await?["documents"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "Native index contains no documents"
    );
    ensure!(
        installed
            .run(&root, &["workspace", "search", "Synthetic"], None)
            .await?["matches"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "Native indexed search returned no source references"
    );
    checks.push("native SQLite index and source references");
    let supplied = base.join("supplied.txt");
    fs::write(
        &supplied,
        "Built a synthetic dashboard.\nUnselected supplied detail stays in the original.\n",
    )?;
    let imported = installed
        .run(
            &root,
            &["source", "import", "--input", text(&supplied)?],
            None,
        )
        .await?;
    let source_path = imported["path"]
        .as_str()
        .context("Imported source path missing")?;
    ensure!(
        fs::read(root.join(source_path))? == fs::read(&supplied)?
            && imported["facts_extracted"] == false
            && imported["primary_cv_changed"] == false,
        "Import changed supplied bytes or canonical facts"
    );
    let missing = base.join("not-initialized");
    let mut selection = installed
        .run(
            &missing,
            &["document", "template", "--kind", "application_answers"],
            None,
        )
        .await?;
    ensure!(
        !missing.exists(),
        "Template unexpectedly created a workspace"
    );
    selection["sections"][0]["text"] = json!("Built a synthetic dashboard.");
    selection["sections"][0]["citations"] =
        json!([{"path":source_path,"sha256":imported["sha256"],"start_line":1,"end_line":1}]);
    let selected = base.join("selection.json");
    write_json(&selected, &selection)?;
    let document_path = "output/application-answer.md";
    let rendered = installed
        .run(
            &root,
            &[
                "document",
                "render",
                "--input",
                text(&selected)?,
                "--output",
                document_path,
            ],
            None,
        )
        .await?;
    ensure!(
        rendered["citations_checked"] == 1 && rendered["claims_independently_verified"] == false,
        "Rendered document provenance differs"
    );
    let document = fs::read(root.join(document_path))?;
    ensure!(
        !String::from_utf8_lossy(&document).contains("Unselected supplied detail"),
        "Unselected original content leaked into rendered document"
    );
    let outcomes = installed
        .run(&root, &["workspace", "outcomes"], None)
        .await?;
    ensure!(
        outcomes["counts"]["pending"] == 1
            && outcomes["counts"]["rejected"] == 0
            && outcomes["counts"]["submitted_denominator"] == 0
            && outcomes["counts"]["submission_unknown"] == 1
            && outcomes["discovery_channel"]["groups"][0]["value"].is_null(),
        "Outcome report guessed a submission or discovery channel"
    );
    let denied = installed
        .output(
            &root,
            &[
                "document",
                "render",
                "--input",
                text(&selected)?,
                "--output",
                "context/new-facts.md",
            ],
            None,
        )
        .await?;
    ensure!(
        !denied.status.success()
            && serde_json::from_slice::<Value>(&denied.stdout)?["error"]["code"] == "path_denied"
            && !root.join("context/new-facts.md").exists(),
        "Render bypassed the canonical fact boundary"
    );
    let status = installed.run(&root, &["status", "--json"], None).await?;
    ensure!(
        status["health"]["records"] == 1 && status["attention"]["live_mail_checked"] == false,
        "Status invented provider observations"
    );
    let human = installed.output(&root, &["status"], None).await?;
    let human_text = String::from_utf8(human.stdout)?;
    ensure!(
        human.status.success()
            && human_text.contains("Job Harness · Workspace")
            && human_text.contains("Next tool:"),
        "Human status output missing"
    );
    checks.push("preserved source import, cited selection-only render and descriptive outcomes");
    let bundle = base.join("private.jhb");
    installed
        .run(
            &root,
            &["workspace", "export", "--output", text(&bundle)?],
            None,
        )
        .await?;
    installed
        .run(
            &restored,
            &["workspace", "import", "--input", text(&bundle)?],
            None,
        )
        .await?;
    ensure!(
        fs::read(restored.join("output/primary-cv.pdf"))? == pdf
            && fs::read(restored.join("applications/example.md"))? == fs::read(&record)?
            && fs::read(restored.join(source_path))? == fs::read(&supplied)?
            && fs::read(restored.join(document_path))? == document,
        "Private bundle transfer changed exact selected bytes"
    );
    ensure!(
        installed
            .run(&restored, &["call", "bootstrap"], None)
            .await?["health"]["records"]
            == 1,
        "Restored bootstrap record count differs"
    );
    checks.push("private bundle transfer preserves CV, source, document and vacancy bytes");
    for profile in ["focused", "full"] {
        installed.mcp(&root, profile, &specs).await?;
        checks.push(if profile == "focused" {
            "official MCP client: focused initialize, advertised tools and bootstrap"
        } else {
            "official MCP client: full initialize, authoritative catalog and bootstrap"
        });
    }
    ensure!(
        parent_environment
            .iter()
            .all(|(name, value)| std::env::var_os(name).as_ref() == value.as_ref()),
        "Artifact verification changed the invoking process's environment"
    );
    checks.push("parent HOME and host-settings environment unchanged");
    Ok(
        json!({"ok":true,"task":"package","version":version,"checks":checks,"focused_tools":job_harness_protocol::FOCUSED_TOOLS.len(),"full_tools":specs.as_object().map_or(0, serde_json::Map::len),"private_data_used":false}),
    )
}
pub async fn verify_component(
    binary: &Path,
    component: crate::release::Component,
) -> Result<Value> {
    if component == crate::release::Component::Core {
        return verify(binary).await;
    }
    let parent_environment: Vec<_> = ["HOME", "XDG_CONFIG_HOME", "APPDATA", "USERPROFILE"]
        .into_iter()
        .map(|name| (name, std::env::var_os(name)))
        .collect();
    let source = binary
        .canonicalize()
        .context("Select an existing optional native executable")?;
    ensure!(source.is_file(), "Native component must be a file");
    let temporary = tempfile::Builder::new()
        .prefix("job-harness-component-")
        .tempdir()?;
    let base = temporary.path().canonicalize()?;
    let installed = installed(&base, &source, component.executable(), false)?;
    let version = installed.output(&base, &["--version"], None).await?;
    ensure!(
        version.status.success(),
        "Copied optional component version command failed"
    );
    let version = String::from_utf8(version.stdout)?.trim().to_owned();
    ensure!(
        version == format!("{} {}", component.executable(), env!("CARGO_PKG_VERSION")),
        "Copied optional component package version differs"
    );
    let help = installed.output(&base, &["--help"], None).await?;
    ensure!(
        help.status.success() && !help.stdout.is_empty(),
        "Copied optional component help failed outside its source tree"
    );
    let mut checks = vec![
        "copied executable outside checkout with child-only environment and exact package version",
        "help exits without a workspace, credentials or model artifacts",
    ];
    if component == crate::release::Component::Local {
        let list = installed.run(&base, &["list"], None).await?;
        ensure!(
            list.is_object() && !list.as_object().unwrap().is_empty(),
            "Optional provider catalog is empty"
        );
        let doctor = installed.run(&base, &["doctor"], None).await?;
        ensure!(
            doctor["sdk_loads"] == 0
                && doctor["model_downloads"] == 0
                && doctor["native_worker"].is_object(),
            "Optional provider doctor loaded a runtime or downloaded a model"
        );
        checks.push(
            "version 2 model catalog and fresh native doctor worker with zero SDK loads/downloads",
        );
    } else {
        checks.push("version/help only; native window, folder picker and OS approval remain separate acceptance checks");
    }
    ensure!(
        parent_environment
            .iter()
            .all(|(name, value)| std::env::var_os(name).as_ref() == value.as_ref()),
        "Component verification changed the invoking process's environment"
    );
    checks.push("parent HOME and host-settings environment unchanged");
    Ok(
        json!({"ok":true,"task":"package","component":component.name(),"version":version,"checks":checks,"private_data_used":false,"sdk_loads":0,"model_downloads":0,"native_window_checked":false}),
    )
}
