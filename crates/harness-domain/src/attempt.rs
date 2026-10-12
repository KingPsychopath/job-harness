//! Submission lifecycle: uncertainty is durable, never a timeout or retry grant.
use crate::{HarnessError as Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    Reserved,
    Submitting,
    OutcomeUnknown,
    Submitted,
    Cancelled,
}
impl SubmissionStatus {
    pub fn blocks_retry(self) -> bool {
        matches!(
            self,
            Self::Reserved | Self::Submitting | Self::OutcomeUnknown | Self::Submitted
        )
    }
    pub fn can_cancel(self) -> bool {
        matches!(self, Self::Reserved)
    }
}
/// Coordination distinguishes unresolved attempts from completed retry blockers.
pub trait SubmissionLifecycle: Sized {
    fn parse(value: &Value) -> Result<Self>;
    fn unresolved(self) -> bool;
    fn may_resolve(self) -> bool;
}
impl SubmissionLifecycle for SubmissionStatus {
    fn parse(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(|_| {
            Error::new(
                "invalid_state",
                "Unknown submission state; preserve coordinator state",
            )
        })
    }
    fn unresolved(self) -> bool {
        matches!(
            self,
            Self::Reserved | Self::Submitting | Self::OutcomeUnknown
        )
    }
    fn may_resolve(self) -> bool {
        matches!(self, Self::Submitting | Self::OutcomeUnknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertainty_and_resolution_are_distinct() {
        assert!(SubmissionStatus::OutcomeUnknown.unresolved());
        assert!(SubmissionStatus::OutcomeUnknown.may_resolve());
        assert!(!SubmissionStatus::Submitted.may_resolve());
        assert!(!SubmissionStatus::Submitted.unresolved());
        assert!(!SubmissionStatus::Cancelled.blocks_retry());
    }
}
