//! Thin daily conveniences: the engine owns validation and all local state changes.
use clap::Subcommand;
use job_harness_core::{Engine, HarnessError};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub(crate) enum DailyCommand {
    /// Read saved progress and recruiting communications; fresh mail requires supplied host observations.
    ///
    /// With no JSON input, only saved workspace state is checked. This never scans an inbox,
    /// changes labels, applies for jobs or launches an agent. Output is a version 2 JSON envelope.
    Check {
        /// JSON tool arguments from a file. Omit to read stdin; empty input checks saved state only.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Supplied mail snapshot path relative to the private workspace; never a live inbox scan.
        #[arg(long)]
        observations: Option<String>,
        /// Expected SHA256 of that supplied snapshot, when the caller needs a freshness guard.
        #[arg(long)]
        source_sha256: Option<String>,
        /// Skip this many queue items; leave unset to use the JSON input or tool default.
        #[arg(long)]
        offset: Option<usize>,
        /// Maximum queue items; leave unset to use the JSON input or tool default.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Record explicitly supplied, reviewed local daily updates; never applies, sends or archives externally.
    ///
    /// Supply the daily_update tool arguments as JSON. The engine checks evidence and identity.
    /// Human-confirmed archive selection and verified host outcomes use the separate archive lifecycle.
    Update {
        /// Reviewed JSON tool arguments from a file. Omit to read stdin.
        #[arg(long)]
        input: Option<PathBuf>,
    },
}

pub(crate) fn execute(root: &Path, command: DailyCommand) -> anyhow::Result<Value> {
    let (tool, args) = match command {
        DailyCommand::Check {
            input,
            observations,
            source_sha256,
            offset,
            limit,
        } => {
            let mut args = super::input(input)?;
            for (field, value) in [
                ("mail_observations_path", observations.map(Value::String)),
                ("expected_source_sha256", source_sha256.map(Value::String)),
                ("offset", offset.map(|value| json!(value))),
                ("limit", limit.map(|value| json!(value))),
            ] {
                if let Some(value) = value {
                    if args.get(field).is_some_and(|existing| existing != &value) {
                        return Err(HarnessError::new(
                            "invalid_arguments",
                            format!("JSON input and command flag disagree for {field}."),
                        )
                        .into());
                    }
                    args[field] = value;
                }
            }
            ("daily_check", args)
        }
        DailyCommand::Update { input } => ("daily_update", super::input(input)?),
    };
    Ok(Engine::open(root)?.call(tool, &args))
}
