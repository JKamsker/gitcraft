//! Presentation only. Variants encode actual producer branches; unknown errors never get
//! parsed for nested messages. Keep these source values until paint, across locale changes.

use super::{Channel, Previous};
use crate::i18n::{message, text};

impl Channel {
    /// Disjoint display API for this base, whose canonical `label` still returns English.
    /// The interaction owner's newer `label` may already use its own shared message IDs.
    pub fn display_label(self) -> &'static str {
        text(match self {
            Self::Main => "interaction-update-channel-main",
            Self::Dev => "interaction-update-channel-dev",
        })
    }
}

impl Previous {
    /// Preserve known version strings exactly; only the application fallback is translated.
    /// `label` stays raw because rollback errors currently compose it at their source.
    pub fn display_label(&self) -> &str {
        self.version
            .as_deref()
            .unwrap_or_else(|| text("interaction-update-previous-kept"))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PreviousLabel<'a> {
    Version(&'a str),
    Kept,
}
impl<'a> From<&'a Previous> for PreviousLabel<'a> {
    fn from(previous: &'a Previous) -> Self {
        previous
            .version
            .as_deref()
            .map_or(Self::Kept, Self::Version)
    }
}
impl PreviousLabel<'_> {
    pub(crate) fn display(self) -> String {
        match self {
            Self::Version(version) => version.to_owned(),
            Self::Kept => text("interaction-update-previous-kept").to_owned(),
        }
    }
}

/// Only `download`'s own size guard creates `TooBig`; network/filesystem errors are opaque.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DownloadFailure<'a> {
    TooBig,
    Opaque(&'a str),
}
impl DownloadFailure<'_> {
    fn display(self) -> String {
        match self {
            Self::TooBig => text("interaction-update-download-too-big").to_owned(),
            Self::Opaque(detail) => detail.to_owned(),
        }
    }
}

/// Only the failed `ditto` status in `move_into` creates `CopyFailed`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PlacementFailure<'a> {
    CopyFailed,
    Opaque(&'a str),
}
impl PlacementFailure<'_> {
    fn display(self) -> String {
        match self {
            Self::CopyFailed => text("interaction-update-copy-new").to_owned(),
            Self::Opaque(detail) => detail.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Diagnostic<'a> {
    NotUpdated,
    NoBuild,
    NoDownloadFolder,
    AssetMissing {
        asset: &'a str,
    },
    Download {
        url: &'a str,
        failure: DownloadFailure<'a>,
    },
    Unpack {
        detail: &'a str,
    },
    CopyNew,
    NoPrevious,
    MoveAside {
        target: &'a str,
        detail: &'a str,
    },
    PutNew {
        failure: PlacementFailure<'a>,
    },
    RestorePrevious {
        previous: PreviousLabel<'a>,
        detail: &'a str,
    },
    RestoreBoth {
        previous: PreviousLabel<'a>,
        detail: &'a str,
        old: &'a str,
        target: &'a str,
    },
    Opaque(&'a str),
}
impl Diagnostic<'_> {
    pub(crate) fn display(self) -> String {
        match self {
            Self::NotUpdated => text("interaction-update-not-updated").to_owned(),
            Self::NoBuild => text("interaction-update-no-build").to_owned(),
            Self::NoDownloadFolder => text("interaction-update-no-download-folder").to_owned(),
            Self::AssetMissing { asset } => message(
                "interaction-update-asset-missing",
                &[("asset", asset.into())],
            ),
            Self::Download { url, failure } => message(
                "interaction-update-download",
                &[("url", url.into()), ("detail", failure.display().into())],
            ),
            Self::Unpack { detail } => {
                message("interaction-update-unpack", &[("detail", detail.into())])
            }
            Self::CopyNew => text("interaction-update-copy-new").to_owned(),
            Self::NoPrevious => text("interaction-update-no-previous").to_owned(),
            Self::MoveAside { target, detail } => message(
                "interaction-update-move-aside",
                &[("target", target.into()), ("detail", detail.into())],
            ),
            Self::PutNew { failure } => message(
                "interaction-update-put-new",
                &[("detail", failure.display().into())],
            ),
            Self::RestorePrevious { previous, detail } => message(
                "interaction-update-restore",
                &[
                    ("previous", previous.display().into()),
                    ("detail", detail.into()),
                ],
            ),
            Self::RestoreBoth {
                previous,
                detail,
                old,
                target,
            } => message(
                "interaction-update-restore-both",
                &[
                    ("previous", previous.display().into()),
                    ("detail", detail.into()),
                    ("old", old.into()),
                    ("target", target.into()),
                ],
            ),
            Self::Opaque(raw) => raw.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests;
