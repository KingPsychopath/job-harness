//! Native CLI: compact JSON for piping; help and progress never contaminate MCP.
mod context;
mod daily;
pub mod host;
mod onboarding;
mod presentation;
use clap::{Parser, Subcommand, ValueEnum};
use job_harness_core::{Engine, HarnessError, contracts};
use serde_json::{Value, json};
use std::{
    fs,
    io::{IsTerminal, Read},
    path::PathBuf,
    time::Duration,
};

#[derive(Parser)]
#[command(version, about="A durable career workspace. Clear tools for any agent.", long_about=None)]
struct Cli {
    /// Private workspace selected by the host. Overrides env and host settings.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Clone, Copy, ValueEnum)]
enum Profile {
    Focused,
    Full,
}
impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::Full => "full",
        }
    }
}
#[derive(Subcommand)]
enum Command {
    /// Create a searchable fictional workspace; refuses existing data and performs no external actions.
    Demo {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// See the included toolkit and optional capabilities, requirements and next commands.
    Capabilities {
        #[arg(long)]
        json: bool,
    },
    /// Set up a private workspace and choose optional capabilities; never downloads them silently.
    Setup {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        capability: Vec<String>,
        /// Print the plan without creating a workspace or saving host settings.
        #[arg(long)]
        plan: bool,
        #[arg(long)]
        json: bool,
    },
    #[cfg(windows)]
    #[command(name = "__acp-launch", hide = true)]
    AcpLaunch {
        agent: PathBuf,
        #[arg(last = true)]
        arguments: Vec<String>,
    },
    /// A small read-only human workspace view; --json returns its compact versioned envelope.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Check daily recruiting progress or record reviewed local updates; no applications or implicit mail actions.
    Daily {
        #[command(subcommand)]
        action: daily::DailyCommand,
    },
    /// Explicit supplied-file preservation; never imports career facts automatically.
    Source {
        #[command(subcommand)]
        action: presentation::SourceCommand,
    },
    /// Templates and selected cited documents, with no automatic CV promotion.
    Document {
        #[command(subcommand)]
        action: presentation::DocumentCommand,
    },
    /// Create an empty workspace outside the software checkout.
    Init,
    /// Rebuild derived pipeline files, or check their exact bytes without events.
    Pipeline {
        #[arg(long)]
        check: bool,
    },
    /// Select a default workspace in owner-only host settings.
    Configure {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Invoke one typed tool. JSON from stdin or --input; compact JSON to stdout.
    Call {
        tool: String,
        #[arg(long)]
        input: Option<PathBuf>,
    },
    /// Inspect the complete tool catalog or a single input/output schema.
    Schemas { tool: Option<String> },
    /// Inspect a workspace/workflow configuration schema.
    ConfigSchema {
        #[arg(value_parser=["ledger","workflows"])]
        kind: String,
    },
    /// Run local MCP over stdio. stdout belongs exclusively to the protocol.
    Mcp {
        #[arg(long, value_enum, default_value = "focused")]
        tool_profile: Profile,
        /// Restrict tools to coordination without submission boundaries or tracking corrections.
        #[arg(long)]
        deny_external_actions: bool,
    },
    /// Print host launch configuration. Contains paths, never credentials.
    Connect {
        #[arg(value_parser=["mcp","acp"])]
        target: String,
        #[arg(long, value_enum, default_value = "focused")]
        tool_profile: Profile,
    },
    /// Private transfer, index and search operations.
    Workspace {
        #[command(subcommand)]
        action: Workspace,
    },
    /// Validate, prepare or build an evidence-backed Typst CV.
    Cv {
        #[command(subcommand)]
        action: Cv,
    },
    /// Open a read-only local dashboard. No writable or arbitrary-tool HTTP API.
    Gui {
        #[arg(long, default_value = "0")]
        port: u16,
        /// Enable explicit reviewed local changes. External actions stay unavailable.
        #[arg(long)]
        allow_local_writes: bool,
    },
    /// Hand off a prompt to your authenticated external ACP agent (stable v1).
    Acp {
        #[arg(long)]
        agent: PathBuf,
        #[arg(long, allow_hyphen_values = true)]
        agent_arg: Vec<String>,
        #[arg(long)]
        prompt: String,
        #[arg(long, default_value = "300")]
        timeout_seconds: u64,
        /// Explicitly enable submission-boundary tools for an authorized external-capable host.
        #[arg(long)]
        allow_external_actions: bool,
    },
    /// Run a bounded local-tool loop with a local or remote compatible model.
    Agent {
        #[command(subcommand)]
        action: Agent,
    },
    /// Inspect and explicitly resolve a conflicting local journal.
    Recover {
        #[command(subcommand)]
        action: Recover,
    },
    /// Preserve supplied notes and review sourced proposals before accepting them.
    Notes {
        #[command(subcommand)]
        action: Notes,
    },
    /// Optional semantic retrieval, model context and supplied audio adapters.
    Context {
        #[command(subcommand)]
        action: context::ContextCommand,
    },
}
#[derive(Subcommand)]
enum Notes {
    Import(Box<NotesImport>),
    List {
        #[arg(long, default_value = "0")]
        offset: usize,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long)]
        include_archived: bool,
    },
    Review {
        note_id: String,
        #[arg(long)]
        proposal_id: Option<String>,
        #[arg(long)]
        include_original: bool,
    },
    Propose {
        #[arg(long)]
        input: PathBuf,
    },
    Accept {
        #[arg(long)]
        input: PathBuf,
    },
    Archive {
        #[arg(long)]
        input: PathBuf,
    },
}
#[derive(clap::Args)]
struct NotesImport {
    #[arg(long)]
    input: PathBuf,
    #[arg(long,value_parser=["personal_notes","transcript","provider_summary","generated_summary"],default_value="personal_notes")]
    kind: String,
    #[arg(long)]
    source_label: Option<String>,
    #[arg(long)]
    occurred_at: Option<String>,
    #[arg(long)]
    occurred_period: Option<String>,
    #[arg(long)]
    valid_from: Option<String>,
    #[arg(long)]
    valid_to: Option<String>,
    #[arg(long)]
    supersedes: Option<String>,
    #[arg(long)]
    source_group: Option<String>,
    #[arg(long)]
    source_uri: Option<String>,
    #[arg(long)]
    link_application: Vec<String>,
    #[arg(long)]
    link_project: Vec<String>,
    #[arg(long)]
    key: String,
}

#[derive(Subcommand)]
enum Recover {
    Inspect,
    Resolve {
        #[arg(long,value_parser=["journal","current"])]
        keep: String,
        #[arg(long)]
        expected_sha256: String,
        #[arg(long, required = true)]
        evidence: Vec<String>,
        #[arg(long)]
        key: String,
    },
}
#[derive(Subcommand)]
enum Agent {
    Run {
        /// OpenAI-compatible base URL (/v1) or full chat-completions endpoint.
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        model: String,
        /// Name of an injected environment variable. Never pass the key itself.
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        prompt: String,
        #[arg(long, default_value = "8")]
        max_steps: usize,
        #[arg(long, default_value = "32")]
        max_tool_calls: usize,
        /// Opt in to local workspace mutations. No external actions are provided.
        #[arg(long)]
        allow_local_writes: bool,
        #[arg(long, default_value = "300")]
        timeout_seconds: u64,
    },
}
#[derive(Subcommand)]
enum Workspace {
    Export {
        #[arg(long)]
        output: PathBuf,
        #[arg(long,value_parser=["workspace","profile"],default_value="workspace")]
        kind: String,
        #[arg(long)]
        drop_history: bool,
    },
    Import {
        #[arg(long)]
        input: PathBuf,
    },
    /// Rebuild the disposable SQLite search view from canonical source records.
    Index,
    /// Search the disposable index, preserving canonical record references.
    Search {
        query: String,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Read indexed counts and status.
    Overview,
    /// Descriptive recorded counts, with separate supplied discovery/ATS attribution.
    Outcomes,
}
#[derive(Subcommand)]
enum Cv {
    Validate {
        #[arg(long)]
        input: PathBuf,
    },
    Prepare {
        #[arg(long, default_value = "content/selection.json")]
        content: String,
        #[arg(long, default_value = "output/cv-preview.pdf")]
        output: String,
    },
    Build {
        #[arg(long, default_value = "content/selection.json")]
        content: String,
        #[arg(long, default_value = "output/cv-preview.pdf")]
        output: String,
    },
}

fn success(data: Value) -> Value {
    json!({"schema_version":2,"ok":true,"data":data,"error":null,"next":[]})
}
fn error(error: anyhow::Error) -> Value {
    let (code, message) = error.downcast_ref::<HarnessError>().map(|e|(e.code.as_str(),e.message.as_str()))
        .unwrap_or(("command_failed","Command failed. Check input, selected workspace and host configuration; preserve unfinished external work before retrying."));
    json!({"schema_version":2,"ok":false,"data":null,"error":{"code":code,"message":message,"retryable":false,
        "recovery":"Inspect the workspace and command help; never repeat an unresolved submission."},"next":[]})
}
fn input(path: Option<PathBuf>) -> anyhow::Result<Value> {
    let mut raw = Vec::new();
    if let Some(path) = path {
        fs::File::open(path)?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut raw)?;
    } else if !std::io::stdin().is_terminal() {
        std::io::stdin()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut raw)?;
    }
    if raw.len() > 8 * 1024 * 1024 {
        return Err(HarnessError::new("invalid_arguments", "JSON input exceeds 8 MiB.").into());
    }
    let value = if raw.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&raw)
            .map_err(|_| HarnessError::new("invalid_arguments", "Input must be a JSON object."))?
    };
    if !value.is_object() {
        return Err(HarnessError::new("invalid_arguments", "Input must be a JSON object.").into());
    }
    Ok(value)
}

async fn execute(cli: Cli) -> anyhow::Result<Option<Value>> {
    match cli.command {
        Command::Demo { workspace } => return Ok(Some(success(onboarding::demo(&workspace)?))),
        Command::Capabilities { json } => {
            let data = onboarding::catalog();
            if json {
                return Ok(Some(success(data)));
            }
            print!("{}", onboarding::text(&data));
            return Ok(None);
        }
        Command::Setup {
            workspace,
            mut capability,
            plan,
            json,
        } => {
            if capability.is_empty() && !json && !plan && std::io::stdin().is_terminal() {
                capability = onboarding::prompt()?;
            }
            let data = onboarding::setup(&workspace, &capability, plan)?;
            if json {
                return Ok(Some(success(data)));
            }
            print!("{}", onboarding::text(&data));
            return Ok(None);
        }
        #[cfg(windows)]
        Command::AcpLaunch { agent, arguments } => {
            job_harness_protocol::acp_windows::launch(&agent, &arguments)?;
            return Ok(None);
        }
        Command::Document {
            action: presentation::DocumentCommand::Template { kind },
        } => {
            return Ok(Some(job_harness_core::presentation::document_template(
                &kind,
            )?));
        }
        Command::Schemas { tool } => {
            let data = match tool {
                Some(name) => contracts::specs().get(&name).cloned().ok_or_else(|| {
                    HarnessError::new(
                        "unknown_tool",
                        "Unknown tool; run job-harness schemas for the catalog.",
                    )
                })?,
                None => Value::Object(contracts::specs().clone()),
            };
            return Ok(Some(data));
        }
        Command::ConfigSchema { kind } => {
            return Ok(Some(if kind == "ledger" {
                contracts::ledger_schema()
            } else {
                contracts::workflow_schema()
            }));
        }
        Command::Configure { workspace } => return Ok(Some(success(host::configure(&workspace)?))),
        Command::Context { action }
            if matches!(
                &action,
                context::ContextCommand::Models
                    | context::ContextCommand::Transcribe { .. }
                    | context::ContextCommand::Triage { .. }
            ) =>
        {
            return Ok(Some(success(
                context::execute(std::path::Path::new("."), action).await?,
            )));
        }
        Command::Cv {
            action: Cv::Validate { input: path },
        } => {
            let data: Value = serde_json::from_slice(&fs::read(path)?)?;
            return Ok(Some(success(job_harness_core::cv::validate(&data)?)));
        }
        _ => {}
    }
    let root = host::resolve(cli.root)?;
    let data = match cli.command {
        Command::Capabilities { .. } | Command::Setup { .. } | Command::Demo { .. } => {
            unreachable!()
        }
        Command::Status { json } => {
            let snapshot = job_harness_core::presentation::status_snapshot(&root)?;
            if json || snapshot["ok"] != true {
                return Ok(Some(snapshot));
            }
            print!(
                "{}",
                job_harness_core::presentation::status_text(&snapshot)?
            );
            return Ok(None);
        }
        Command::Source { action } => presentation::source(&root, action)?,
        Command::Daily { action } => return Ok(Some(daily::execute(&root, action)?)),
        Command::Document { action } => presentation::document(&root, action)?,
        Command::Init => job_harness_core::workspace::initialize(&root)?,
        Command::Pipeline { check } => {
            let root = root.canonicalize()?;
            let _guard = job_harness_core::storage::WorkspaceLock::acquire(&root, !check)?;
            if check {
                let pipeline = job_harness_core::pipeline::build(&root)?;
                let stale = fs::read(root.join("applications/pipeline.md"))
                    .ok()
                    .as_deref()
                    != Some(pipeline.markdown.as_bytes())
                    || fs::read(root.join("applications/pipeline.json"))
                        .ok()
                        .as_deref()
                        != Some(
                            job_harness_core::pipeline::json_bytes(&pipeline.records)?.as_slice(),
                        );
                if stale {
                    return Ok(Some(
                        json!({"schema_version":2,"ok":false,"data":{"stale":true,"records":pipeline.records.len()},"error":{"code":"stale_view","message":"Derived pipeline files are stale.","retryable":false,"recovery":"Run job-harness pipeline to rebuild the views; canonical records are preserved."},"next":[]}),
                    ));
                }
                json!({"stale":false,"records":pipeline.records.len()})
            } else {
                job_harness_core::pipeline::write(&root)?
            }
        }
        Command::Call { tool, input: path } => {
            return Ok(Some(Engine::open(&root)?.call(&tool, &input(path)?)));
        }
        Command::Mcp {
            tool_profile,
            deny_external_actions,
        } => {
            job_harness_protocol::mcp::serve(
                &root,
                tool_profile.name(),
                &Value::Object(contracts::specs().clone()),
                deny_external_actions,
            )
            .await?;
            return Ok(None);
        }
        Command::Connect {
            target,
            tool_profile,
        } => {
            let root = root.canonicalize()?;
            Engine::open(&root)?;
            let executable = std::env::current_exe()?;
            let config =
                job_harness_protocol::mcp_connection(&executable, &root, tool_profile.name());
            if target == "mcp" {
                config
            } else {
                json!({"protocol":"acp","version":1,"role":"client","workspace":root,"mcp":config,
                "command":"job-harness --root PATH acp --agent /absolute/path/to/agent --prompt TASK",
                "permission_policy":"cancel_requests","note":"Authenticate the external ACP agent in its own host. ACP is a client-agent protocol, not a replacement for MCP tools."})
            }
        }
        Command::Workspace { action } => match action {
            Workspace::Export {
                output,
                kind,
                drop_history,
            } => job_harness_core::bundle::export_bundle_with_options(
                &root,
                &output,
                &kind,
                drop_history,
            )?,
            Workspace::Import { input } => job_harness_core::bundle::import_bundle(&root, &input)?,
            Workspace::Index => job_harness_core::index::refresh(&root)?,
            Workspace::Search { query, limit } => {
                job_harness_core::index::search(&root, &query, limit)?
            }
            Workspace::Overview => job_harness_core::index::overview(&root)?,
            Workspace::Outcomes => job_harness_core::presentation::outcomes(&root)?,
        },
        Command::Cv { action } => match action {
            Cv::Prepare { content, output } => {
                job_harness_core::cv::prepare(&root, &content, &output)?
            }
            Cv::Build { content, output } => job_harness_core::cv::build(&root, &content, &output)?,
            Cv::Validate { .. } => unreachable!(),
        },
        Command::Gui {
            port,
            allow_local_writes,
        } => {
            job_harness_protocol::dashboard::serve(&root, port, allow_local_writes).await?;
            return Ok(None);
        }
        Command::Acp {
            agent,
            agent_arg,
            prompt,
            timeout_seconds,
            allow_external_actions,
        } => {
            Engine::open(&root)?;
            if !(1..=3600).contains(&timeout_seconds) {
                return Err(HarnessError::new(
                    "invalid_arguments",
                    "ACP timeout must be 1–3600 seconds.",
                )
                .into());
            }
            job_harness_protocol::acp::run(
                &root,
                &std::env::current_exe()?,
                &agent,
                agent_arg,
                prompt,
                Duration::from_secs(timeout_seconds),
                allow_external_actions,
            )
            .await?
        }
        Command::Agent {
            action:
                Agent::Run {
                    endpoint,
                    model,
                    api_key_env,
                    prompt,
                    max_steps,
                    max_tool_calls,
                    allow_local_writes,
                    timeout_seconds,
                },
        } => {
            job_harness_protocol::provider::run(
                &root,
                job_harness_protocol::provider::Options {
                    endpoint,
                    model,
                    api_key_env,
                    prompt,
                    max_steps,
                    max_tool_calls,
                    allow_local_writes,
                    timeout: Duration::from_secs(timeout_seconds),
                },
            )
            .await?
        }
        Command::Recover { action } => match action {
            Recover::Inspect => Engine::open(&root)?.pending_transaction()?,
            Recover::Resolve {
                keep,
                expected_sha256,
                evidence,
                key,
            } => Engine::open(&root)?.resolve_pending(&keep, &evidence, &key, &expected_sha256)?,
        },
        Command::Notes { action } => {
            let (tool, args) = match action {
                Notes::Import(options) => {
                    let NotesImport {
                        input: path,
                        kind,
                        source_label,
                        occurred_at,
                        occurred_period,
                        valid_from,
                        valid_to,
                        supersedes,
                        source_group,
                        source_uri,
                        link_application,
                        link_project,
                        key,
                    } = *options;
                    let mut bytes = Vec::new();
                    fs::File::open(path)?
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut bytes)?;
                    if bytes.len() > 1024 * 1024 {
                        return Err(HarnessError::new(
                            "invalid_arguments",
                            "Supplied notes exceed 1 MiB.",
                        )
                        .into());
                    }
                    let text=String::from_utf8(bytes).map_err(|_|HarnessError::new("invalid_arguments","Supply UTF-8 text; transcribe an audio file through the optional context adapter first."))?;
                    let mut args = json!({"text":text,"source_kind":kind,"idempotency_key":key,"links":link_application.into_iter().map(|path|json!({"kind":"application","path":path})).chain(link_project.into_iter().map(|path|json!({"kind":"project","path":path}))).collect::<Vec<_>>()});
                    if let Some(label) = source_label {
                        args["source_label"] = json!(label);
                    }
                    if let Some(value) = occurred_at {
                        args["occurred_at"] = json!(value);
                    }
                    for (field, value) in [
                        ("occurred_period", occurred_period),
                        ("valid_from", valid_from),
                        ("valid_to", valid_to),
                        ("supersedes", supersedes),
                        ("source_group", source_group),
                    ] {
                        if let Some(value) = value {
                            args[field] = json!(value);
                        }
                    }
                    if let Some(value) = source_uri {
                        args["source_uri"] = json!(value);
                    }
                    ("intake_note", args)
                }
                Notes::List {
                    offset,
                    limit,
                    include_archived,
                } => (
                    "list_notes",
                    json!({"offset":offset,"limit":limit,"include_archived":include_archived}),
                ),
                Notes::Review {
                    note_id,
                    proposal_id,
                    include_original,
                } => {
                    let mut args = json!({"note_id":note_id,"include_original":include_original});
                    if let Some(id) = proposal_id {
                        args["proposal_id"] = json!(id);
                    }
                    ("review_note", args)
                }
                Notes::Propose { input: path } => ("propose_note", input(Some(path))?),
                Notes::Accept { input: path } => ("accept_note", input(Some(path))?),
                Notes::Archive { input: path } => ("archive_note", input(Some(path))?),
            };
            return Ok(Some(Engine::open(&root)?.call(tool, &args)));
        }
        Command::Context { action } => context::execute(&root, action).await?,
        Command::Schemas { .. } | Command::ConfigSchema { .. } | Command::Configure { .. } => {
            unreachable!()
        }
        #[cfg(windows)]
        Command::AcpLaunch { .. } => unreachable!(),
    };
    Ok(Some(success(data)))
}

pub async fn run() {
    let cli = Cli::parse();
    let protocol = matches!(&cli.command, Command::Mcp { .. });
    #[cfg(windows)]
    let protocol = protocol || matches!(&cli.command, Command::AcpLaunch { .. });
    let result = match execute(cli).await {
        Ok(None) => return,
        Ok(Some(value)) => value,
        Err(err) => error(err),
    };
    if protocol {
        eprintln!("{result}");
    } else {
        println!("{result}");
    }
    if result.get("ok") == Some(&Value::Bool(false)) {
        std::process::exit(1);
    }
}
