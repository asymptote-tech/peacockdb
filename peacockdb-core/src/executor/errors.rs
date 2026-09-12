//! How the two run-time errors print, what a `RunError` becomes at the DataFusion
//! boundary, and what the sink says when the device's schema is not the declared one.

use std::fmt;

use datafusion::arrow::datatypes::Schema;
use datafusion::error::DataFusionError;

use super::{BackendError, RunError};

/// Every column whose exported type is not the declared one, as `{index} {name}: {declared}
/// vs {exported}` joined by `; `. This is what `RecordBatch::try_new`'s message lacks: it
/// names both types but not the column, and stops at the first mismatch, so a sink
/// carrying a string and a narrow decimal reads as one finding.
///
/// Types only, on purpose. `try_new` does not check nullability, so that difference never
/// reaches the sink, and a class in the survey's report that cannot occur is worse than
/// none (`llm-wiki/tasks/sink-divergence-survey.md`). `zip` truncating on a width mismatch
/// is fine: `try_new` has already refused on the count, and its message carries it.
// Its one caller is the GPU sink, which `rust-only` compiles out; the tests below remain.
#[cfg_attr(feature = "rust-only", allow(dead_code))]
pub(crate) fn schema_divergence(declared: &Schema, exported: &Schema) -> String {
    declared
        .fields()
        .iter()
        .zip(exported.fields())
        .enumerate()
        .filter(|(_, (d, e))| d.data_type() != e.data_type())
        .map(|(at, (d, e))| format!("{at} {}: {} vs {}", d.name(), d.data_type(), e.data_type()))
        .collect::<Vec<_>>()
        .join("; ")
}

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

#[cfg(test)]
mod tests;
