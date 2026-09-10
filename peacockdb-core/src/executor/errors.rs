//! How the two run-time errors print, and what a `RunError` becomes at the DataFusion
//! boundary.

use std::fmt;

use datafusion::error::DataFusionError;

use super::{BackendError, RunError};

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BackendError {}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded { message, .. } => write!(f, "{message}"),
            Self::Protocol(what) => write!(f, "protocol violation: {what}"),
            Self::CallFailed(what) => write!(f, "call failed: {what}"),
            Self::Backend(inner) => write!(f, "{inner}"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<RunError> for DataFusionError {
    fn from(error: RunError) -> Self {
        match error {
            // The class an exhausted budget has always raised, so a caller distinguishing an
            // over-budget query from a broken one keeps doing it by the same match.
            RunError::BudgetExceeded { message, .. } => {
                DataFusionError::ResourcesExhausted(message)
            }
            RunError::Protocol(_) | RunError::CallFailed(_) => {
                DataFusionError::Execution(error.to_string())
            }
            RunError::Backend(_) => DataFusionError::Plan(error.to_string()),
        }
    }
}
