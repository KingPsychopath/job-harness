//! Small explicit host file adapters; these commands grant no external action authority.
use clap::Subcommand;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum SourceCommand {
    /// Preserve a supplied PDF/UTF-8 JSON/Markdown/text file; never infer facts or set primary CV.
    Import {
        #[arg(long)]
        input: PathBuf,
    },
}
#[derive(Subcommand)]
pub enum DocumentCommand {
    /// Emit a draft selection with empty text/citations; fill missing facts before rendering.
    Template {
        #[arg(long,value_parser=["cover_letter","application_answers","interview_story","portfolio_case_study","period_review"])]
        kind: String,
    },
    /// Verify exact source citations and render only the selected wording to a new destination.
    Render {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long,value_parser=["markdown","json"],default_value="markdown")]
        format: String,
    },
}
pub fn source(root: &Path, action: SourceCommand) -> anyhow::Result<Value> {
    match action {
        SourceCommand::Import { input } => {
            Ok(job_harness_core::presentation::import_source(root, &input)?)
        }
    }
}
pub fn document(root: &Path, action: DocumentCommand) -> anyhow::Result<Value> {
    match action {
        DocumentCommand::Template { kind } => {
            Ok(job_harness_core::presentation::document_template(&kind)?)
        }
        DocumentCommand::Render {
            input,
            output,
            format,
        } => Ok(job_harness_core::presentation::render_document(
            root, &input, &output, &format,
        )?),
    }
}
