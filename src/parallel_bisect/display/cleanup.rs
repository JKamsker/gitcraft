//! Cleanup responsibilities.
use super::*;

/// Separately supplied presentation context from the named producer's exact error arm.
/// These fields must be retained BEFORE the producer flattens its error. They must not
/// be recovered by splitting its Result<String>, or selected from provider text.
/// This is optional paint metadata only; the raw Result/worker/control state stays raw.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CleanupContext<'a> {
    /// bundle::completed: (Err(primary), Err(cleanup)).
    BundleCompleted { primary: &'a str, cleanup: &'a str },
    /// observe::finish: (Err(primary), Err(cleanup)).
    ObservationFinish { primary: &'a str, cleanup: &'a str },
    /// Workspace::fetch: (Err(primary), Err(cleanup)) after transfer removal.
    WorkspaceTransfer { primary: &'a str, cleanup: &'a str },
    /// Workspace::close: (Err(primary), Err(release)).
    WorkspaceLeaseRelease { primary: &'a str, release: &'a str },
    /// bundle::pipeline: failed publication/object check with an owned keep hash.
    BundleKeepRetained { primary: &'a str, hash: &'a str },
}

/// Render only the supplied outer cleanup/keep statement, retaining both details verbatim.
/// `raw` stays caller-owned for control, diagnostics and recovery. A mismatch returns it
/// unchanged, so stale/wrong presentation metadata cannot replace the actual error.
/// Production captures owned snapshots in presentation.rs at the named source branches
/// and carries them beside raw results/events. diagnostic(raw) never infers this context.
pub(crate) fn cleanup_diagnostic(raw: &str, context: CleanupContext<'_>) -> String {
    let (expected, id, primary, name, detail) = match context {
        CleanupContext::BundleCompleted { primary, cleanup } => (
            format!("{primary}; quarantine cleanup failed: {cleanup}"),
            "interaction-bisect-quarantine-cleanup",
            primary,
            "cleanup",
            cleanup,
        ),
        CleanupContext::ObservationFinish { primary, cleanup } => (
            format!("{primary}; workspace cleanup: {cleanup}"),
            "interaction-bisect-workspace-cleanup-pair",
            primary,
            "cleanup",
            cleanup,
        ),
        CleanupContext::WorkspaceTransfer { primary, cleanup } => (
            format!("{primary}; transfer cleanup: {cleanup}"),
            "interaction-bisect-transfer-cleanup",
            primary,
            "cleanup",
            cleanup,
        ),
        CleanupContext::WorkspaceLeaseRelease { primary, release } => (
            format!("{primary}; lease release: {release}"),
            "interaction-bisect-lease-release",
            primary,
            "cleanup",
            release,
        ),
        CleanupContext::BundleKeepRetained { primary, hash } => (
            format!("{primary}; owned pack-{hash}.keep retained"),
            "interaction-bisect-keep-retained",
            primary,
            "hash",
            hash,
        ),
    };
    if expected != raw {
        return raw.to_owned();
    }
    i18n::message(id, &[("detail", primary.into()), (name, detail.into())])
}

#[cfg(test)]
pub(super) fn known_observation_detail(raw: &str) -> bool {
    matches!(
        raw,
        "Frame output exceeds 2 MiB; no result cached"
            | "Text output is not UTF-8"
            | "Benchmark output is not UTF-8"
            | "Benchmark must print one finite number"
    ) || raw.starts_with("PNG frame could not be decoded (maximum 1024×1024): ")
}
