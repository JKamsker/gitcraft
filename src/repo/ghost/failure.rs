//! Retained preview failure identity and localized presentation.

use super::*;

/// A retained failure's source; no String is ever inferred to be a backend-owned variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewFailure {
    Backend(crate::git::dry_run::Failure),
    RawPlan(String),
}

impl From<String> for PreviewFailure {
    fn from(raw: String) -> Self {
        Self::RawPlan(raw)
    }
}
impl From<crate::git::dry_run::Failure> for PreviewFailure {
    fn from(failure: crate::git::dry_run::Failure) -> Self {
        Self::Backend(failure)
    }
}
impl PreviewFailure {
    pub(crate) fn raw(&self) -> String {
        match self {
            Self::Backend(failure) => failure.raw(),
            Self::RawPlan(raw) => raw.clone(),
        }
    }
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Backend(failure) => failure.display(),
            Self::RawPlan(raw) => plan::display_preview_text(raw),
        }
    }
    pub(super) fn is_cancelled(&self) -> bool {
        match self {
            Self::Backend(failure) => failure.is_cancelled(),
            Self::RawPlan(raw) => raw == CANCELLED,
        }
    }
    /// Existing String callers retain the backend's display-at-call compatibility boundary.
    #[cfg(test)]
    pub(super) fn legacy(&self) -> String {
        match self {
            Self::Backend(failure) => failure.display(),
            Self::RawPlan(raw) => raw.clone(),
        }
    }
}
