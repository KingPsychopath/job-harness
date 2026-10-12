//! Pure workflow vocabulary. No filesystem, network, connector or model dependency.
use serde::{Deserialize, Serialize};
pub mod attempt;
pub mod canonical;
pub mod identity;
pub mod time;
pub mod tracking;
pub use attempt::{SubmissionLifecycle, SubmissionStatus};

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct HarnessError {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, HarnessError>;
impl HarnessError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }
}
impl From<std::io::Error> for HarnessError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::new(
                "missing_resource",
                "Required local resource was not found; check the selected workspace and source path.",
            )
        } else {
            Self::new(
                "io_error",
                "Local storage operation failed; preserve data and inspect recovery state.",
            )
        }
    }
}
impl From<serde_json::Error> for HarnessError {
    fn from(_: serde_json::Error) -> Self {
        Self::validation("Invalid JSON document.")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationStage {
    Saved,
    Prepared,
    Applied,
    Introduction,
    Screening,
    Technical,
    Final,
    Offer,
    Closed,
    NeedsReview,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertainty_is_not_a_retry_ticket() {
        assert!(SubmissionStatus::OutcomeUnknown.blocks_retry());
        assert!(!SubmissionStatus::OutcomeUnknown.can_cancel());
        assert!(SubmissionStatus::Reserved.can_cancel());
    }
}
