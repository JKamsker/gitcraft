//! Source-captured presentation receipts. Never inspected by runtime control or caches.
use super::display::{self, CleanupContext};

#[derive(Clone, Debug)]
pub enum Cleanup {
    BundleCompleted { primary: String, cleanup: String },
    ObservationFinish { primary: String, cleanup: String },
    WorkspaceTransfer { primary: String, cleanup: String },
    WorkspaceLeaseRelease { primary: String, release: String },
    BundleKeepRetained { primary: String, hash: String },
}
impl Cleanup {
    fn borrowed(&self) -> CleanupContext<'_> {
        match self {
            Self::BundleCompleted { primary, cleanup } => {
                CleanupContext::BundleCompleted { primary, cleanup }
            }
            Self::ObservationFinish { primary, cleanup } => {
                CleanupContext::ObservationFinish { primary, cleanup }
            }
            Self::WorkspaceTransfer { primary, cleanup } => {
                CleanupContext::WorkspaceTransfer { primary, cleanup }
            }
            Self::WorkspaceLeaseRelease { primary, release } => {
                CleanupContext::WorkspaceLeaseRelease { primary, release }
            }
            Self::BundleKeepRetained { primary, hash } => {
                CleanupContext::BundleKeepRetained { primary, hash }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Presentation {
    raw: String,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Cleanup(Cleanup),
    Completed {
        import: bool,
        cleanup: String,
        child: Option<Cleanup>,
    },
}
impl Presentation {
    pub(crate) fn capture(raw: String, context: Cleanup) -> Self {
        Self {
            raw,
            kind: Kind::Cleanup(context),
        }
    }
    pub(crate) fn completed(
        raw: String,
        import: bool,
        cleanup: String,
        child: Option<Self>,
    ) -> Self {
        // A completed-only wrapper may carry the close receipt; never a primary-error tree.
        let child = child.and_then(|p| p.into_close(&cleanup));
        Self {
            raw,
            kind: Kind::Completed {
                import,
                cleanup,
                child,
            },
        }
    }
    fn into_close(self, raw: &str) -> Option<Cleanup> {
        if self.raw != raw {
            return None;
        }
        match self.kind {
            Kind::Cleanup(c @ Cleanup::WorkspaceLeaseRelease { .. }) => Some(c),
            _ => None,
        }
    }
    pub(crate) fn matching(self, raw: &str) -> Option<Self> {
        (self.raw == raw).then_some(self)
    }
    pub(crate) fn render(&self, raw: &str) -> String {
        if self.raw != raw {
            return raw.into();
        }
        match &self.kind {
            Kind::Cleanup(c) => display::cleanup_diagnostic(raw, c.borrowed()),
            Kind::Completed {
                import,
                cleanup,
                child,
            } => {
                let detail = child.as_ref().map_or_else(
                    || cleanup.clone(),
                    |c| display::cleanup_diagnostic(cleanup, c.borrowed()),
                );
                crate::i18n::message(
                    if *import {
                        "interaction-bisect-publication-cleanup"
                    } else {
                        "interaction-bisect-verification-cleanup"
                    },
                    &[("detail", detail.into())],
                )
            }
        }
    }
}
/// Unknown errors remain opaque. A stale receipt cannot paint a different result.
pub(crate) fn error(raw: &str, presentation: Option<&Presentation>) -> String {
    presentation.map_or_else(|| raw.into(), |p| p.render(raw))
}
pub(crate) fn retain_error<T>(result: &Result<T, String>, presentation: &mut Option<Presentation>) {
    *presentation = result
        .as_ref()
        .err()
        .and_then(|raw| presentation.take().and_then(|p| p.matching(raw)));
}
#[derive(Clone, Debug)]
pub struct CleanupWarning {
    raw: String,
    machine: String,
    detail: String,
    child: Option<Cleanup>,
}
impl CleanupWarning {
    pub(crate) fn capture(
        raw: String,
        machine: String,
        detail: String,
        child: Option<Presentation>,
    ) -> Self {
        let child = child.and_then(|p| p.into_close(&detail));
        Self {
            raw,
            machine,
            detail,
            child,
        }
    }
    pub(crate) fn render(&self, raw: &str) -> String {
        if raw != self.raw {
            return raw.into();
        }
        crate::i18n::message(
            "interaction-bisect-worktrees-retained",
            &[
                ("machine", display::machine_location(&self.machine).into()),
                (
                    "detail",
                    self.child
                        .as_ref()
                        .map_or_else(
                            || self.detail.clone(),
                            |c| display::cleanup_diagnostic(&self.detail, c.borrowed()),
                        )
                        .into(),
                ),
            ],
        )
    }
}
