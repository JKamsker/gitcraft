//! Owned provenance from the updater's original guards. No message-text inference.
//! Raw identity survives worker/state retention; display borrows the accepted helpers.

use super::display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadFailure {
    TooBig { raw: String },
    Opaque(String),
}
impl DownloadFailure {
    pub fn raw(&self) -> &str {
        match self {
            Self::TooBig { raw } | Self::Opaque(raw) => raw,
        }
    }
    fn borrowed(&self) -> display::DownloadFailure<'_> {
        match self {
            Self::TooBig { .. } => display::DownloadFailure::TooBig,
            Self::Opaque(raw) => display::DownloadFailure::Opaque(raw),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementFailure {
    CopyFailed { raw: String },
    Opaque(String),
}
impl PlacementFailure {
    pub fn raw(&self) -> &str {
        match self {
            Self::CopyFailed { raw } | Self::Opaque(raw) => raw,
        }
    }
    pub fn into_raw(self) -> String {
        match self {
            Self::CopyFailed { raw } | Self::Opaque(raw) => raw,
        }
    }
    fn borrowed(&self) -> display::PlacementFailure<'_> {
        match self {
            Self::CopyFailed { .. } => display::PlacementFailure::CopyFailed,
            Self::Opaque(raw) => display::PlacementFailure::Opaque(raw),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Opaque(String),
    NotUpdated {
        raw: String,
    },
    NoBuild {
        raw: String,
    },
    NoDownloadFolder {
        raw: String,
    },
    AssetMissing {
        raw: String,
        asset: String,
    },
    Download {
        raw: String,
        url: String,
        failure: DownloadFailure,
    },
    Unpack {
        raw: String,
        detail: String,
    },
    NoPrevious {
        raw: String,
    },
    MoveAside {
        raw: String,
        target: String,
        detail: String,
    },
    PutNew {
        raw: String,
        failure: PlacementFailure,
    },
    RestorePrevious {
        raw: String,
        previous: Option<String>,
        detail: String,
    },
    RestoreBoth {
        raw: String,
        previous: Option<String>,
        detail: String,
        old: String,
        target: String,
    },
}
impl Failure {
    pub fn raw(&self) -> &str {
        match self {
            Self::Opaque(raw)
            | Self::NotUpdated { raw }
            | Self::NoBuild { raw }
            | Self::NoDownloadFolder { raw }
            | Self::AssetMissing { raw, .. }
            | Self::Download { raw, .. }
            | Self::Unpack { raw, .. }
            | Self::NoPrevious { raw }
            | Self::MoveAside { raw, .. }
            | Self::PutNew { raw, .. }
            | Self::RestorePrevious { raw, .. }
            | Self::RestoreBoth { raw, .. } => raw,
        }
    }
    pub fn into_raw(self) -> String {
        match self {
            Self::Opaque(raw)
            | Self::NotUpdated { raw }
            | Self::NoBuild { raw }
            | Self::NoDownloadFolder { raw }
            | Self::AssetMissing { raw, .. }
            | Self::Download { raw, .. }
            | Self::Unpack { raw, .. }
            | Self::NoPrevious { raw }
            | Self::MoveAside { raw, .. }
            | Self::PutNew { raw, .. }
            | Self::RestorePrevious { raw, .. }
            | Self::RestoreBoth { raw, .. } => raw,
        }
    }
    pub fn display(&self) -> String {
        let diagnostic = match self {
            Self::Opaque(raw) => display::Diagnostic::Opaque(raw),
            Self::NotUpdated { .. } => display::Diagnostic::NotUpdated,
            Self::NoBuild { .. } => display::Diagnostic::NoBuild,
            Self::NoDownloadFolder { .. } => display::Diagnostic::NoDownloadFolder,
            Self::AssetMissing { asset, .. } => display::Diagnostic::AssetMissing { asset },
            Self::Download { url, failure, .. } => display::Diagnostic::Download {
                url,
                failure: failure.borrowed(),
            },
            Self::Unpack { detail, .. } => display::Diagnostic::Unpack { detail },
            Self::NoPrevious { .. } => display::Diagnostic::NoPrevious,
            Self::MoveAside { target, detail, .. } => {
                display::Diagnostic::MoveAside { target, detail }
            }
            Self::PutNew { failure, .. } => display::Diagnostic::PutNew {
                failure: failure.borrowed(),
            },
            Self::RestorePrevious {
                previous, detail, ..
            } => display::Diagnostic::RestorePrevious {
                previous: previous_label(previous),
                detail,
            },
            Self::RestoreBoth {
                previous,
                detail,
                old,
                target,
                ..
            } => display::Diagnostic::RestoreBoth {
                previous: previous_label(previous),
                detail,
                old,
                target,
            },
        };
        diagnostic.display()
    }
}

fn previous_label(version: &Option<String>) -> display::PreviousLabel<'_> {
    version.as_deref().map_or(
        display::PreviousLabel::Kept,
        display::PreviousLabel::Version,
    )
}
