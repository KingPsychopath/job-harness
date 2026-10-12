//! Durable workspace capabilities shared by CLI, MCP, ACP hosts and local views.
pub use job_harness_domain::{HarnessError, Result};
pub type Error = HarnessError;
pub mod bundle;
pub mod contracts;
pub mod cv;
pub mod domain;
pub mod engine;
pub mod index;
pub mod intake;
pub mod pipeline;
pub mod presentation;
pub mod semantic;
pub mod state;
pub mod storage;
pub mod workflow;
pub mod workspace;
pub use engine::Engine;
