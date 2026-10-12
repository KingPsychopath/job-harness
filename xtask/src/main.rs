//! Native development gates; no candidate workspace or provider credentials.
mod contracts;
mod licenses;
mod package;
mod release;
mod repository;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xtask", about = "Native Job Harness development checks")]
struct Args {
    /// Reusable source checkout; never a candidate workspace.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/.."))]
    root: PathBuf,
    #[command(subcommand)]
    command: Task,
}
#[derive(Subcommand)]
enum Task {
    /// Audit tracked and non-ignored generic source, links and resources.
    Repository,
    /// Generate TypeScript contracts from the authoritative JSON schemas.
    Contracts {
        /// Fail when the checked-in declarations need regeneration.
        #[arg(long)]
        check: bool,
    },
    /// Stage software-only native ZIP/tar.gz archives with checksums.
    ReleaseStage {
        #[arg(long, value_enum, default_value = "core")]
        component: release::Component,
        #[arg(long)]
        tag: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        alias_binary: Option<PathBuf>,
        #[arg(long, default_value = "dist")]
        output: PathBuf,
        /// Local experiment only; records source_dirty=true in BUILD.json.
        #[arg(long)]
        allow_dirty: bool,
    },
    /// Verify a copied native executable outside its source checkout.
    Package {
        #[arg(long, value_enum, default_value = "core")]
        component: release::Component,
        #[arg(long)]
        binary: PathBuf,
    },
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("xtask: {error:#}");
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let args = Args::parse();
    let root = args.root.canonicalize()?;
    match args.command {
        Task::Repository => println!("{}", repository::check(&root)?),
        Task::Contracts { check } => println!("{}", contracts::generate(&root, check)?),
        Task::ReleaseStage {
            component,
            tag,
            target,
            binary,
            alias_binary,
            output,
            allow_dirty,
        } => println!(
            "{}",
            release::stage(
                &root,
                component,
                &tag,
                &target,
                &binary,
                alias_binary.as_deref(),
                &output,
                allow_dirty
            )?
        ),
        Task::Package { component, binary } => {
            println!("{}", package::verify_component(&binary, component).await?)
        }
    }
    Ok(())
}
