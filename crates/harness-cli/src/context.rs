//! Optional context services selected by the trusted host; no mandatory model runtime.
use clap::{Args, Subcommand};
use job_harness_protocol::context_host::{self, Connection};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub struct ModelArgs {
    #[arg(long)]
    endpoint: String,
    #[arg(long)]
    model: String,
    /// Immutable model version/digest supplied by your runtime.
    #[arg(long)]
    revision: String,
    #[arg(long)]
    api_key_env: Option<String>,
    #[arg(long, default_value = "300")]
    timeout_seconds: u64,
}
impl ModelArgs {
    fn connection(&self) -> Connection {
        Connection {
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            revision: self.revision.clone(),
            key_env: self.api_key_env.clone(),
            timeout_seconds: self.timeout_seconds,
        }
    }
}
#[derive(Subcommand)]
pub enum ContextCommand {
    /// Show optional capability requirements; no probes, downloads or keys.
    Models,
    /// Embed only explicitly selected sources into a disposable cache.
    Index {
        #[command(flatten)]
        connection: ModelArgs,
        #[arg(long, required = true)]
        source: Vec<String>,
        #[arg(long, default_value = "40")]
        chunk_lines: usize,
        #[arg(long, default_value = "")]
        document_prefix: String,
        #[arg(long, default_value = "")]
        query_prefix: String,
    },
    /// Hybrid lexical/vector retrieval with exact source hashes and line anchors.
    Search {
        #[command(flatten)]
        connection: ModelArgs,
        #[arg(long)]
        query: String,
        #[arg(long, default_value = "40")]
        chunk_lines: usize,
        #[arg(long, default_value = "")]
        document_prefix: String,
        #[arg(long, default_value = "")]
        query_prefix: String,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Draft a cited enrichment; original/approved records remain untouched.
    Enrich {
        #[command(flatten)]
        connection: ModelArgs,
        #[arg(long)]
        note_id: String,
        #[arg(long)]
        source: Vec<String>,
    },
    /// Draft text from an explicit audio file through a compatible endpoint.
    Transcribe {
        #[command(flatten)]
        connection: ModelArgs,
        #[arg(long)]
        audio: PathBuf,
    },
    /// Typed inbox suggestion, using rules or an explicitly selected model.
    Triage {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
    },
    /// Inspect the local vector cache, without connecting a model.
    Inspect,
}
pub async fn execute(root: &Path, action: ContextCommand) -> anyhow::Result<Value> {
    match action {
        ContextCommand::Models => Ok(context_host::model_metadata()),
        ContextCommand::Inspect => Ok(job_harness_core::semantic::inspect(root)?),
        ContextCommand::Index {
            connection,
            source,
            chunk_lines,
            document_prefix,
            query_prefix,
        } => {
            context_host::index_sources(
                root,
                &connection.connection(),
                &source,
                chunk_lines,
                &document_prefix,
                &query_prefix,
            )
            .await
        }
        ContextCommand::Search {
            connection,
            query,
            chunk_lines,
            document_prefix,
            query_prefix,
            limit,
        } => {
            context_host::semantic_search(
                root,
                &connection.connection(),
                &query,
                chunk_lines,
                &document_prefix,
                &query_prefix,
                limit,
            )
            .await
        }
        ContextCommand::Enrich {
            connection,
            note_id,
            source,
        } => context_host::enrich(root, &connection.connection(), &note_id, &source).await,
        ContextCommand::Transcribe { connection, audio } => {
            context_host::transcribe(&connection.connection(), &audio).await
        }
        ContextCommand::Triage {
            input,
            endpoint,
            model,
            revision,
            api_key_env,
        } => {
            let metadata = fs::metadata(&input)?;
            if metadata.len() > 100_000 {
                return Err(job_harness_core::HarnessError::validation(
                    "Select a thread up to one hundred thousand bytes",
                )
                .into());
            }
            let text = fs::read_to_string(input)?;
            if let Some(endpoint) = endpoint {
                let model = model.ok_or_else(|| {
                    job_harness_core::HarnessError::validation("Select a model explicitly")
                })?;
                let revision = revision.ok_or_else(|| {
                    job_harness_core::HarnessError::validation("Select a revision explicitly")
                })?;
                context_host::classify(
                    &Connection {
                        endpoint,
                        model,
                        revision,
                        key_env: api_key_env,
                        timeout_seconds: 300,
                    },
                    &text,
                )
                .await
            } else {
                Ok(json!({"decision":context_host::rule_triage(&text),"required_model":false}))
            }
        }
    }
}
