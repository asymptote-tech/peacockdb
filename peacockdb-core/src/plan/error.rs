//! How a refusal prints. The variants are `mod.rs`'s; these are their two trait impls.

use std::fmt;

use super::PlanError;

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Invalid(what) => write!(f, "invalid plan: {what}"),
        }
    }
}

impl std::error::Error for PlanError {}
