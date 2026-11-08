//! Paint-only adapters for this partition. Never feed these strings back into control,
//! protocol, cache, storage, command or worker-event identities. Unknown details stay opaque.
use crate::i18n;

#[cfg(test)]
const EXACT: &[(&str, &str)] = &[
    (
        "git listed a commit of the range twice",
        "interaction-bisect-dag-duplicate",
    ),
    (
        "could not find the home folder of this computer",
        "interaction-bisect-home-here",
    ),
    (
        "Frame output exceeds 2 MiB; no result cached",
        "interaction-bisect-frame-too-large",
    ),
    (
        "Text output is not UTF-8",
        "interaction-bisect-text-encoding",
    ),
    (
        "Benchmark output is not UTF-8",
        "interaction-bisect-benchmark-encoding",
    ),
    (
        "Benchmark must print one finite number",
        "interaction-bisect-benchmark-finite",
    ),
    (
        "A pinned full commit ID is required",
        "interaction-bisect-pinned-commit",
    ),
    (
        "Give a command of at most 8192 bytes",
        "interaction-bisect-command-size",
    ),
    (
        "Observation command ended by a signal",
        "interaction-bisect-observation-signal",
    ),
    (
        "Frame stderr exceeds 2 MiB; no result cached",
        "interaction-bisect-stderr-too-large",
    ),
    (
        "missing stop reason",
        "interaction-bisect-missing-stop-reason",
    ),
    (
        "the test ended without a result",
        "interaction-bisect-no-result",
    ),
    (
        "the bisect stopped on an error in gitcraft",
        "interaction-bisect-internal-error",
    ),
    (
        "type the command that tests a commit",
        "interaction-bisect-command-required",
    ),
    (
        "mark at least one good commit",
        "interaction-bisect-good-required",
    ),
    (
        "the good and bad commits have to be full commit hashes",
        "interaction-bisect-full-hashes",
    ),
    (
        "the same commit is marked good and bad",
        "interaction-bisect-conflicting-marks",
    ),
    ("Removing the worktrees…", "interaction-bisect-removing"),
    (
        "Reading the commits to bisect…",
        "interaction-bisect-reading",
    ),
    (
        "the bisect ran out of commits to test before it ended",
        "interaction-bisect-exhausted",
    ),
    (
        "workspace ownership uncertain; external recovery evidence retained",
        "interaction-bisect-ownership-uncertain",
    ),
    (
        "invalid cleanup owner name",
        "interaction-bisect-cleanup-owner",
    ),
    (
        "missing or mismatched cleaned receipt; external ledger retained",
        "interaction-bisect-cleaned-receipt",
    ),
    (
        "unsupported canonical repository path",
        "interaction-bisect-canonical-path",
    ),
    (
        "invalid or excessive transfer commit identifiers",
        "interaction-bisect-transfer-ids",
    ),
    (
        "invalid pack or history exceeds 256 MiB transfer limit; clone the repository there and name its folder",
        "interaction-bisect-transfer-size",
    ),
    (
        "This platform cannot establish enforced bundle memory limits. Import/unpack is supported on Linux (including SSH/WSL) and Windows; inspection and export remain available. No pack was opened.",
        "interaction-bisect-bundle-platform",
    ),
    (
        "The destination repository changed since inspection; reopen the bundle",
        "interaction-bisect-destination-inspection",
    ),
    (
        "The destination repository changed before quarantine setup",
        "interaction-bisect-destination-setup",
    ),
    (
        "Unconfirmed pack keep response; protection retained",
        "interaction-bisect-keep-response",
    ),
    (
        "Unconfirmed pack hash; protection retained",
        "interaction-bisect-keep-hash",
    ),
    (
        "No advertised bundle refs",
        "interaction-bisect-bundle-no-refs",
    ),
    (
        "Unsupported object format",
        "interaction-bisect-object-format",
    ),
    (
        "Bundle and destination object formats differ",
        "interaction-bisect-formats-differ",
    ),
    (
        "Object verification response was not UTF-8",
        "interaction-bisect-object-encoding",
    ),
    (
        "Incomplete object verification response",
        "interaction-bisect-object-response",
    ),
    (
        "The destination repository changed; refs were not published",
        "interaction-bisect-destination-publication",
    ),
    (
        "Object directory has unsupported control characters",
        "interaction-bisect-objects-path",
    ),
    (
        "Contained Git output exceeded 2 MiB; no partial success is accepted",
        "interaction-bisect-contained-output",
    ),
    (
        "Application path is not UTF-8",
        "interaction-bisect-application-path",
    ),
    (
        "Contained bundle import requires Python 3 on this Linux host; no pack was opened.",
        "interaction-bisect-python-required",
    ),
    (
        "gitcraft: stopped test log unavailable or not a regular file",
        "interaction-bisect-stopped-log",
    ),
    (
        "transferred bundle digest differs",
        "interaction-bisect-transfer-digest",
    ),
    (
        "gitcraft: cleanup base is outside claimed repository; external recovery ledger retained",
        "interaction-bisect-clear-cleanup-base",
    ),
    (
        "gitcraft: workspace or ledger became a symbolic link; external recovery ledger retained",
        "interaction-bisect-clear-workspace-ledger-link",
    ),
    (
        "gitcraft: unresolved legacy evidence in workspace base; external recovery ledger retained",
        "interaction-bisect-clear-legacy-evidence",
    ),
    (
        "gitcraft: missing generation proof; external recovery ledger retained",
        "interaction-bisect-clear-generation-missing",
    ),
    (
        "gitcraft: invalid generation proof; external recovery ledger retained",
        "interaction-bisect-clear-generation-proof",
    ),
    (
        "gitcraft: invalid generation count; external recovery ledger retained",
        "interaction-bisect-clear-generation-count",
    ),
    (
        "gitcraft: generation proof bound; external recovery ledger retained",
        "interaction-bisect-clear-generation-bound",
    ),
    (
        "gitcraft: truncated generation proof; external recovery ledger retained",
        "interaction-bisect-clear-generation-truncated",
    ),
    (
        "gitcraft: invalid generation length; external recovery ledger retained",
        "interaction-bisect-clear-generation-length",
    ),
    (
        "gitcraft: invalid generation; external recovery ledger retained",
        "interaction-bisect-clear-generation-invalid",
    ),
    (
        "gitcraft: current generation missing; external recovery ledger retained",
        "interaction-bisect-clear-current-generation",
    ),
    (
        "gitcraft: missing record proof; external recovery ledger retained",
        "interaction-bisect-clear-record-missing",
    ),
    (
        "gitcraft: invalid record proof; external recovery ledger retained",
        "interaction-bisect-clear-record-proof",
    ),
    (
        "gitcraft: invalid record count; external recovery ledger retained",
        "interaction-bisect-clear-record-count",
    ),
    (
        "gitcraft: record proof bound; external recovery ledger retained",
        "interaction-bisect-clear-record-bound",
    ),
    (
        "gitcraft: owner records changed; external recovery ledger retained",
        "interaction-bisect-clear-records-changed",
    ),
    (
        "gitcraft: owner record changed; external recovery ledger retained",
        "interaction-bisect-clear-record-changed",
    ),
    (
        "gitcraft: cannot inspect owner record; external recovery ledger retained",
        "interaction-bisect-clear-record-inspect",
    ),
    (
        "gitcraft: cannot encode owner record; external recovery ledger retained",
        "interaction-bisect-clear-record-encode",
    ),
    (
        "gitcraft: owner record replaced; external recovery ledger retained",
        "interaction-bisect-clear-record-replaced",
    ),
    (
        "gitcraft: owner record set changed; external recovery ledger retained",
        "interaction-bisect-clear-record-set",
    ),
    (
        "gitcraft: unexpected trailing recovery proof; external recovery ledger retained",
        "interaction-bisect-clear-proof-trailing",
    ),
    (
        "gitcraft: cannot list bisect refs; external recovery ledger retained",
        "interaction-bisect-clear-refs-list",
    ),
    (
        "gitcraft: cannot list worktrees; external recovery ledger retained",
        "interaction-bisect-clear-worktrees-list",
    ),
    (
        "gitcraft: quoted worktree path requires manual recovery; external recovery ledger retained",
        "interaction-bisect-clear-quoted-worktree",
    ),
    (
        "gitcraft: cannot verify remaining worktrees; external recovery ledger retained",
        "interaction-bisect-clear-worktrees-verify",
    ),
    (
        "gitcraft: a bisect worktree remains registered; external recovery ledger retained",
        "interaction-bisect-clear-worktree-registered",
    ),
    (
        "gitcraft: cannot resolve worktree metadata; external recovery ledger retained",
        "interaction-bisect-clear-metadata-resolve",
    ),
    (
        "gitcraft: cannot read worktree metadata; external recovery ledger retained",
        "interaction-bisect-clear-metadata-read",
    ),
    (
        "gitcraft: cannot remove bisect worktree metadata; external recovery ledger retained",
        "interaction-bisect-clear-metadata-remove",
    ),
    (
        "gitcraft: worktree metadata remains; external recovery ledger retained",
        "interaction-bisect-clear-metadata-remains",
    ),
    (
        "gitcraft: cannot verify remaining refs; external recovery ledger retained",
        "interaction-bisect-clear-refs-verify",
    ),
    (
        "gitcraft: bisect refs remain; external recovery ledger retained",
        "interaction-bisect-clear-refs-remain",
    ),
    (
        "gitcraft: workspace became a symbolic link; external recovery ledger retained",
        "interaction-bisect-clear-workspace-link",
    ),
    (
        "gitcraft: workspace remains after removal; external recovery ledger retained",
        "interaction-bisect-clear-workspace-remains",
    ),
];

#[cfg(test)]
const DETAIL: &[(&str, &str, &str, &str, &str)] = &[
    (
        "git listed a parent of ",
        " before it; the range can't be bisected",
        "interaction-bisect-dag-order",
        "hash",
        "git listed a parent of {hash} before it; the range can't be bisected",
    ),
    (
        "git printed a line the bisect can't read: ",
        "",
        "interaction-bisect-dag-line",
        "detail",
        "git printed a line the bisect can't read: {detail}",
    ),
    (
        "could not count the cores of ",
        "",
        "interaction-bisect-cores",
        "machine",
        "could not count the cores of {machine}",
    ),
    (
        "could not find the home folder on ",
        "",
        "interaction-bisect-home-there",
        "machine",
        "could not find the home folder on {machine}",
    ),
    (
        "PNG frame could not be decoded (maximum 1024×1024): ",
        "",
        "interaction-bisect-png-decode",
        "detail",
        "PNG frame could not be decoded (maximum 1024×1024): {detail}",
    ),
    (
        "Observation cleanup unknown: ",
        "",
        "interaction-bisect-observation-cleanup",
        "detail",
        "Observation cleanup unknown: {detail}",
    ),
    (
        "Observation checkout failed: ",
        "",
        "interaction-bisect-observation-checkout",
        "detail",
        "Observation checkout failed: {detail}",
    ),
    (
        "could not prepare bisect test log: ",
        "",
        "interaction-bisect-prepare-log",
        "detail",
        "could not prepare bisect test log: {detail}",
    ),
    (
        "test cleanup unknown; workspace preserved: ",
        "",
        "interaction-bisect-test-cleanup",
        "detail",
        "test cleanup unknown; workspace preserved: {detail}",
    ),
    (
        "could not read stopped test log: ",
        "",
        "interaction-bisect-read-log",
        "detail",
        "could not read stopped test log: {detail}",
    ),
    (
        "the test was ended by a signal\n",
        "",
        "interaction-bisect-test-signal",
        "detail",
        "the test was ended by a signal\n{detail}",
    ),
    (
        "",
        " is reachable from a good commit: nothing to bisect",
        "interaction-bisect-reachable",
        "hash",
        "{hash} is reachable from a good commit: nothing to bisect",
    ),
    (
        "Setting up on ",
        "…",
        "interaction-bisect-setting-up",
        "machine",
        "Setting up on {machine}…",
    ),
    (
        "Sending the commits to ",
        "…",
        "interaction-bisect-sending",
        "machine",
        "Sending the commits to {machine}…",
    ),
    (
        "could not create transfer refs: ",
        "",
        "interaction-bisect-transfer-refs",
        "detail",
        "could not create transfer refs: {detail}",
    ),
    (
        "workspace cleanup partial; external ledger retained: ",
        "",
        "interaction-bisect-workspace-cleanup",
        "detail",
        "workspace cleanup partial; external ledger retained: {detail}",
    ),
    (
        "could not pack transfer: ",
        "",
        "interaction-bisect-pack-transfer",
        "detail",
        "could not pack transfer: {detail}",
    ),
    (
        "Ref publication completed, but owned pack keep retirement failed (protection may remain): ",
        "",
        "interaction-bisect-keep-retirement",
        "detail",
        "Ref publication completed, but owned pack keep retirement failed (protection may remain): {detail}",
    ),
    (
        "Required object ",
        " is missing or has an unexpected type; no network fetch was allowed",
        "interaction-bisect-required-object",
        "oid",
        "Required object {oid} is missing or has an unexpected type; no network fetch was allowed",
    ),
    (
        "This pack already has another keep marker (pack-",
        ".keep); it was left untouched and no refs were published. Inspect its owner/recovery evidence before retrying.",
        "interaction-bisect-another-keep",
        "hash",
        "This pack already has another keep marker (pack-{hash}.keep); it was left untouched and no refs were published. Inspect its owner/recovery evidence before retrying.",
    ),
    (
        "",
        "; any created destination pack keep protection was retained",
        "interaction-bisect-protection-retained",
        "detail",
        "{detail}; any created destination pack keep protection was retained",
    ),
    (
        "Ref publication completed, but quarantine cleanup failed: ",
        "",
        "interaction-bisect-publication-cleanup",
        "detail",
        "Ref publication completed, but quarantine cleanup failed: {detail}",
    ),
    (
        "Verification completed, but quarantine cleanup failed: ",
        "",
        "interaction-bisect-verification-cleanup",
        "detail",
        "Verification completed, but quarantine cleanup failed: {detail}",
    ),
    (
        "gitcraft: ref has no recovered generation: ",
        "; external recovery ledger retained",
        "interaction-bisect-clear-ref-generation",
        "reference",
        "gitcraft: ref has no recovered generation: {reference}; external recovery ledger retained",
    ),
    (
        "gitcraft: cannot remove worktree ",
        "; external recovery ledger retained",
        "interaction-bisect-clear-worktree-remove",
        "path",
        "gitcraft: cannot remove worktree {path}; external recovery ledger retained",
    ),
    (
        "gitcraft: cannot remove unchanged ref ",
        "; external recovery ledger retained",
        "interaction-bisect-clear-ref-remove",
        "reference",
        "gitcraft: cannot remove unchanged ref {reference}; external recovery ledger retained",
    ),
    (
        "gitcraft: cannot remove workspace ",
        "; external recovery ledger retained",
        "interaction-bisect-clear-workspace-remove",
        "path",
        "gitcraft: cannot remove workspace {path}; external recovery ledger retained",
    ),
];

/// Only pass application-generated diagnostics from this partition, at paint time.
/// stdout/stderr, custom-command output and arbitrary provider text must bypass this API.
/// Nested detail captures are intentionally opaque; no recursive search/replacement.
/// A Result<String> from bundle/observe does not prove cleanup-combinator ownership.
/// Use cleanup_diagnostic only with separately retained source-bound fields; raw strings
/// alone cannot select that seam. This function leaves unknown cleanup/keep lookalikes raw.
#[cfg(test)]
pub(crate) fn diagnostic(raw: &str) -> String {
    let exact = raw.strip_suffix('\n').unwrap_or(raw);
    let shown = raw;
    if let Some((_, id)) = EXACT.iter().find(|(source, _)| *source == exact) {
        return i18n::text(id).to_owned();
    }
    if let Some(text) = envelope(shown) {
        return text;
    }
    for &(prefix, suffix, id, name, _) in DETAIL {
        if let Some(value) = if prefix.starts_with("gitcraft: ") {
            exact
        } else {
            shown
        }
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(suffix))
        {
            return i18n::message(
                id,
                &[(
                    name,
                    if name == "machine" {
                        machine_location(value).into()
                    } else {
                        value.into()
                    },
                )],
            );
        }
    }
    trailing_envelope(shown).unwrap_or_else(|| raw.to_owned())
}
/// Finite templates emitted by session/runner/observe/workspace/bundle/verdict.
/// Split only known outer envelopes; captured external errors and output remain raw.
#[cfg(test)]
fn envelope(raw: &str) -> Option<String> {
    let message = |id, args: &[(&str, i18n::Value<'_>)]| Some(i18n::message(id, args));
    if let Some(rest) = raw.strip_prefix("could not check out ")
        && let Some((hash, rest)) = rest.split_once(" in a worktree on ")
        && let Some((machine, detail)) = rest.split_once(": ")
    {
        return message(
            "interaction-bisect-checkout",
            &[
                ("hash", hash.into()),
                ("machine", machine_location(machine).into()),
                ("detail", detail.into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("The worktrees on ")
        && let Some((machine, rest)) = rest.split_once(" couldn't all be removed (")
        && let Some(detail) = rest.strip_suffix("); recovery evidence was retained. Resolve the reported ownership or removal failure before trying again.")
    {
        return message("interaction-bisect-worktrees-retained", &[("machine", machine_location(machine).into()), ("detail", detail.into())]);
    }
    if let Some(rest) = raw.strip_prefix("The test of ")
        && let Some((hash, rest)) = rest.split_once(" ended with exit code ")
        && let Some(code) = rest.strip_suffix(" (a crash or a signal): it counts as skipped.")
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-crash-skipped",
            &[("hash", hash.into()), ("code", code.into())],
        );
    }
    if let Some((hash, rest)) = raw.split_once(" ended its test with exit code ")
        && let Some((code, output)) = rest.split_once(
            "; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
        )
        && (output.is_empty() || output.starts_with('\n'))
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-end-test-failed",
            &[
                ("hash", hash.into()),
                ("code", code.into()),
                ("output", output.strip_prefix('\n').unwrap_or(output).into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("ended its test with exit code ")
        && let Some(code) = rest.strip_suffix(
            "; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
        )
        && let Ok(code) = code.parse::<i32>()
    {
        return message("interaction-bisect-interpret", &[("code", code.into())]);
    }
    // Observe's stderr may itself contain the separator; use the first outer stderr marker.
    if let Some((before, stderr)) = raw.split_once("); stderr: ")
        && let Some((detail, code)) = before.rsplit_once(" (exit ")
        && known_observation_detail(detail)
        && (!detail.starts_with("PNG frame ")
            || (raw.matches(" (exit ").count() == 1 && raw.matches("); stderr: ").count() == 1))
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-observation-exit",
            &[
                ("detail", observation_detail(detail).into()),
                ("code", code.into()),
                ("stderr", stderr.into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("could not make a repository for the bisect on ")
        && let Some((machine, detail)) = rest.split_once(": ")
    {
        return message(
            "interaction-bisect-repository-make",
            &[
                ("machine", machine_location(machine).into()),
                ("detail", detail.into()),
            ],
        );
    }
    if let Some(detail) = raw.strip_prefix("Contained Git failed (no exit code): ") {
        return message(
            "interaction-bisect-contained-no-code",
            &[("detail", detail.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("Contained Git failed (")
        && let Some((code, detail)) = rest.split_once("): ")
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-contained-failed",
            &[("code", code.into()), ("detail", detail.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("bisect shell ended with status ") {
        if rest == "None" {
            return message("interaction-bisect-shell-no-status", &[]);
        }
        if let Some(code) = rest.strip_prefix("Some(").and_then(|s| s.strip_suffix(')'))
            && let Ok(code) = code.parse::<i32>()
        {
            return message(
                "interaction-bisect-shell-status",
                &[("status", code.into())],
            );
        }
    }
    for (prefix, id) in [
        (
            "Ref publication completed, but quarantine cleanup failed: ",
            "interaction-bisect-publication-cleanup",
        ),
        (
            "Verification completed, but quarantine cleanup failed: ",
            "interaction-bisect-verification-cleanup",
        ),
    ] {
        if let Some(detail) = raw.strip_prefix(prefix) {
            return message(id, &[("detail", detail.into())]);
        }
    }

    None
}

/// A paint-only name. Machine::label remains the raw cache/session identity.
pub(crate) fn machine_label(machine: &super::Machine) -> String {
    match machine {
        super::Machine::Here => i18n::text("interaction-bisect-machine-here").to_owned(),
        super::Machine::There(host) => host.label(),
    }
}

/// Display a captured Machine::label without changing its cached identity.
#[cfg(test)]
pub(crate) fn machine_name(raw: &str) -> &str {
    if raw == "this computer" {
        i18n::text("interaction-bisect-machine-here")
    } else {
        raw
    }
}

/// Verdict::word and its consumers that classify/store results remain unchanged.
#[cfg(test)]
pub(crate) fn verdict(verdict: super::Verdict) -> &'static str {
    i18n::text(match verdict {
        super::Verdict::Good => "interaction-bisect-verdict-good",
        super::Verdict::Bad => "interaction-bisect-verdict-bad",
        super::Verdict::Skip => "interaction-bisect-verdict-skipped",
        super::Verdict::TimedOut => "interaction-bisect-verdict-timed-out",
    })
}

/// Supplied numeric benchmark value only; never use this to parse or cache worker output.
#[cfg(test)]
pub(crate) fn scalar(value: f64) -> String {
    i18n::message("interaction-bisect-scalar", &[("value", value.into())])
}

/// Session warnings may prefix an app-owned error with its short commit hash.
/// Do not pass worker output to this adapter.
#[cfg(test)]
pub(crate) fn warning(raw: &str) -> String {
    if let Some((hash, detail)) = raw.split_once(": ")
        && matches!(hash.len(), 8 | 40 | 64)
        && hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return i18n::message(
            "interaction-bisect-warning",
            &[("hash", hash.into()), ("detail", diagnostic(detail).into())],
        );
    }
    diagnostic(raw)
}

pub(crate) fn machine_location(raw: &str) -> &str {
    if raw == "this computer" {
        i18n::text("interaction-bisect-machine-here-location")
    } else {
        raw
    }
}

#[cfg(test)]
fn observation_detail(raw: &str) -> String {
    let id = match raw {
        "Frame output exceeds 2 MiB; no result cached" => "interaction-bisect-frame-too-large",
        "Text output is not UTF-8" => "interaction-bisect-text-encoding",
        "Benchmark output is not UTF-8" => "interaction-bisect-benchmark-encoding",
        "Benchmark must print one finite number" => "interaction-bisect-benchmark-finite",
        _ => {
            return raw
                .strip_prefix("PNG frame could not be decoded (maximum 1024×1024): ")
                .map_or_else(
                    || raw.to_owned(),
                    |detail| {
                        i18n::message(
                            "interaction-bisect-png-decode",
                            &[("detail", detail.into())],
                        )
                    },
                );
        }
    };
    i18n::text(id).to_owned()
}

// This source-owned repository-path form is separate from cleanup-combinator metadata.
// Cleanup/keep suffixes are never inferred here: lower-level errors can contain them.
#[cfg(test)]
fn trailing_envelope(raw: &str) -> Option<String> {
    let message = |id, args: &[(&str, i18n::Value<'_>)]| Some(i18n::message(id, args));
    if let Some(rest) = raw.strip_suffix(" is not a git repository")
        && let Some((path, machine)) = rest.rsplit_once(" on ")
    {
        return message(
            "interaction-bisect-not-repository",
            &[
                ("path", path.into()),
                ("machine", machine_location(machine).into()),
            ],
        );
    }
    None
}

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
fn known_observation_detail(raw: &str) -> bool {
    matches!(
        raw,
        "Frame output exceeds 2 MiB; no result cached"
            | "Text output is not UTF-8"
            | "Benchmark output is not UTF-8"
            | "Benchmark must print one finite number"
    ) || raw.starts_with("PNG frame could not be decoded (maximum 1024×1024): ")
}

#[cfg(test)]
mod execution_fluent_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn execution_catalogs_parse_pair_variables_and_render_every_complete_message()
    -> Result<(), String> {
        let sources = [
            (
                "en-US",
                include_str!("../../locales/en-US/interaction-execution.ftl"),
            ),
            (
                "de",
                include_str!("../../locales/de/interaction-execution.ftl"),
            ),
        ];
        let variables =
            regex::Regex::new(r"\$([a-zA-Z][a-zA-Z0-9_-]*)").map_err(|e| e.to_string())?;
        let mut contracts = Vec::new();
        for (locale, source) in sources {
            let mut entries: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
            let mut at = "";
            for line in source
                .lines()
                .filter(|s| !s.starts_with('#') && !s.is_empty())
            {
                if !line.starts_with(' ') {
                    at = line
                        .split_once(" = ")
                        .ok_or("expected top-level message")?
                        .0;
                    if entries.insert(at, BTreeSet::new()).is_some() {
                        return Err(format!("duplicate {at}"));
                    }
                }
                for capture in variables.captures_iter(line) {
                    entries
                        .get_mut(at)
                        .ok_or("missing message")?
                        .insert(capture.get(1).ok_or("missing name")?.as_str());
                }
            }
            let resource = fluent_bundle::FluentResource::try_new(source.to_owned())
                .map_err(|e| format!("{e:?}"))?;
            let mut bundle = fluent_bundle::concurrent::FluentBundle::new_concurrent(vec![
                locale.parse().map_err(|e| format!("{e:?}"))?,
            ]);
            bundle
                .add_resource(resource)
                .map_err(|e| format!("{e:?}"))?;
            for (id, names) in &entries {
                let mut args = fluent_bundle::FluentArgs::new();
                for &name in names {
                    if matches!(name, "code" | "status" | "count" | "value") {
                        args.set(name, 2345.25);
                    } else {
                        args.set(name, "opaque $HOME {count} · 01234567");
                    }
                }
                let pattern = bundle
                    .get_message(id)
                    .and_then(|m| m.value())
                    .ok_or_else(|| format!("missing {id}"))?;
                let mut errors = Vec::new();
                let rendered = bundle.format_pattern(pattern, Some(&args), &mut errors);
                assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
                assert!(!rendered.is_empty(), "{locale} {id}");
            }
            contracts.push(entries);
        }
        assert_eq!(contracts[0], contracts[1]);
        Ok(())
    }

    #[test]
    fn exact_and_single_capture_adapters_render_real_messages_without_touching_details() {
        let opaque = "provider $HOME {code} · refs/heads/me: input\nunknown payload\n";
        for locale in ["en-US", "de"] {
            i18n::set_language(locale);
            for &(raw, id) in EXACT {
                assert_eq!(diagnostic(raw), i18n::text(id));
            }
            for &(prefix, suffix, id, name, _) in DETAIL {
                let raw = format!("{prefix}{opaque}{suffix}");
                let shown = diagnostic(&raw);
                assert!(shown.contains(opaque), "{id}: {shown}");
                assert!(!shown.contains("interaction-bisect-"), "{id}");
                assert!(!shown.contains(&format!("${name}")), "{id}");
            }
            assert_eq!(diagnostic(opaque), opaque);
            assert_eq!(diagnostic("cancelled"), "cancelled");
            assert_eq!(
                diagnostic("process timed out and was stopped"),
                "process timed out and was stopped"
            );
            assert_eq!(
                diagnostic("stdout: Frame output exceeds 2 MiB; no result cached"),
                "stdout: Frame output exceeds 2 MiB; no result cached"
            );
        }
        i18n::set_language("en-US");
    }

    #[test]
    fn supplied_multi_value_diagnostics_preserve_opaque_payloads_and_use_numeric_values() {
        i18n::set_language("de");
        let detail = "provider {code} $HOME: ä\ntrailing output\n";
        let cases = [
            format!("could not check out 01234567 in a worktree on SSH · host: {detail}"),
            format!(
                "The worktrees on SSH · host couldn't all be removed ({detail}); recovery evidence was retained. Resolve the reported ownership or removal failure before trying again."
            ),
            format!("could not make a repository for the bisect on SSH · host: {detail}"),
            format!("Contained Git failed (2345): {detail}"),
            format!("Contained Git failed (no exit code): {detail}"),
            format!("Ref publication completed, but quarantine cleanup failed: {detail}"),
            format!("Verification completed, but quarantine cleanup failed: {detail}"),
        ];
        for raw in cases {
            let shown = diagnostic(&raw);
            assert!(shown.contains(detail), "{shown}");
            assert_ne!(shown, raw);
            assert!(!shown.contains("interaction-bisect-"));
        }
        let raw = format!("{detail}; owned pack-01234567.keep retained");
        let shown = cleanup_diagnostic(
            &raw,
            CleanupContext::BundleKeepRetained {
                primary: detail,
                hash: "01234567",
            },
        );
        assert!(shown.contains(detail), "{shown}");
        assert_ne!(shown, raw);
        assert!(!shown.contains("interaction-bisect-"));
        for raw in [
            "The test of 01234567 ended with exit code 2345 (a crash or a signal): it counts as skipped.",
            "ended its test with exit code 2345; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
            "bisect shell ended with status Some(2345)",
        ] {
            assert!(diagnostic(raw).contains("2.345"));
        }
        assert_eq!(
            diagnostic("bisect shell ended with status None"),
            "Die Bisektions-Shell wurde ohne Exit-Code beendet."
        );
        assert_eq!(scalar(1234.5), "1.234,5");
        assert_eq!(machine_name("this computer"), "dieser Computer");
        assert_eq!(
            machine_label(&super::super::Machine::Here),
            "dieser Computer"
        );
        assert_eq!(machine_name("SSH · host"), "SSH · host");
        assert!(diagnostic("Setting up on this computer…").contains("diesem Computer"));
        assert_eq!(verdict(super::super::Verdict::Good), "gut");
        assert_eq!(super::super::Verdict::Good.word(), "good");
        assert_eq!(super::super::Machine::Here.label(), "this computer");
        assert_eq!(
            diagnostic(
                "The test of a ended with exit code opaque (a crash or a signal): it counts as skipped."
            ),
            "The test of a ended with exit code opaque (a crash or a signal): it counts as skipped."
        );
        let observation = diagnostic(&format!(
            "Benchmark must print one finite number (exit 2345); stderr: {detail}"
        ));
        assert!(observation.contains("genau eine endliche Zahl"));
        assert!(observation.contains("2.345"));
        assert!(observation.contains(detail));
        let output = format!(
            "01234567 ended its test with exit code 2345; 128 and more (a crash or a signal) stops a bisect, as with git bisect run\n{detail}"
        );
        assert!(diagnostic(&output).contains(detail));
        let warning = warning("01234567: could not read stopped test log: native payload");
        assert!(
            warning.contains("01234567")
                && warning.contains("konnte nicht gelesen")
                && warning.contains("native payload")
        );
        i18n::set_language("en-US");
    }
}

#[cfg(test)]
mod execution_fluent_payload_tests {
    use super::*;

    #[test]
    fn anchored_details_keep_lookalike_cleanup_and_exit_phrases_opaque() {
        i18n::set_language("de");
        for detail in [
            "provider text; quarantine cleanup failed: provider detail",
            "provider text; workspace cleanup: provider detail",
            "provider text (exit 2345); stderr: provider detail",
            "provider/path on provider-host is not a git repository",
            "provider text; owned pack-01234567.keep retained",
            "provider text\ntrailing newline\n",
        ] {
            for prefix in [
                "could not read stopped test log: ",
                "Observation cleanup unknown: ",
            ] {
                let raw = format!("{prefix}{detail}");
                assert!(
                    diagnostic(&raw).contains(detail),
                    "{raw} -> {}",
                    diagnostic(&raw)
                );
            }
        }
        let decoder = "provider (exit 7); stderr: decoder raw data";
        let raw = format!(
            "PNG frame could not be decoded (maximum 1024×1024): {decoder} (exit 2345); stderr: actual stderr"
        );
        assert!(diagnostic(&raw).contains(decoder));
        assert_eq!(
            observation_detail(&format!(
                "PNG frame could not be decoded (maximum 1024×1024): {decoder}"
            )),
            format!("Der PNG-Frame konnte nicht dekodiert werden (maximal 1024×1024): {decoder}")
        );
        i18n::set_language("en-US");
    }
}

#[cfg(test)]
mod execution_fluent_r1_tests {
    use super::*;

    #[test]
    fn unwrapped_unknown_cleanup_and_keep_lookalikes_remain_opaque_in_both_languages() {
        for locale in ["en-US", "de"] {
            i18n::set_language(locale);
            for raw in [
                "provider text; workspace cleanup: provider detail",
                "provider text; quarantine cleanup failed: provider detail",
                "provider text; transfer cleanup: provider detail",
                "provider text; lease release: provider detail",
                "provider text; owned pack-01234567.keep retained",
                "provider text; owned pack-01234567.keep retained\n",
                "provider text; workspace cleanup: first; workspace cleanup: second (exit 7); stderr: raw $HOME {detail}\n",
            ] {
                assert_eq!(diagnostic(raw), raw);
                let prefixed = format!("01234567: {raw}");
                assert_eq!(warning(&prefixed), prefixed);
            }
        }
        i18n::set_language("en-US");
    }

    #[test]
    fn supplied_source_context_keeps_delimiters_framing_and_hashes_opaque() {
        let primary = "provider; transfer cleanup: inner; owned pack-opaque.keep retained\n(exit 7); stderr: $HOME {cleanup}";
        let cleanup = "provider; lease release: inner; workspace cleanup: nested\n";
        let hash = "0123456789012345678901234567890123456789";
        for (locale, transfer, lease, retained) in [
            ("en-US", "transfer cleanup", "lease release", "owned pack-"),
            (
                "de",
                "Bereinigung der Übertragung",
                "Freigabe der Besitzreservierung",
                "die eigene Schutzmarkierung pack-",
            ),
        ] {
            i18n::set_language(locale);
            let raw_transfer = format!("{primary}; transfer cleanup: {cleanup}");
            let transfer_shown = cleanup_diagnostic(
                &raw_transfer,
                CleanupContext::WorkspaceTransfer { primary, cleanup },
            );
            assert_eq!(transfer_shown, format!("{primary}; {transfer}: {cleanup}"));
            let raw_lease = format!("{primary}; lease release: {cleanup}");
            let lease_shown = cleanup_diagnostic(
                &raw_lease,
                CleanupContext::WorkspaceLeaseRelease {
                    primary,
                    release: cleanup,
                },
            );
            assert_eq!(lease_shown, format!("{primary}; {lease}: {cleanup}"));
            let raw_keep = format!("{primary}; owned pack-{hash}.keep retained");
            let keep_shown = cleanup_diagnostic(
                &raw_keep,
                CleanupContext::BundleKeepRetained { primary, hash },
            );
            let suffix = if locale == "de" {
                ".keep wurde aufbewahrt."
            } else {
                ".keep retained."
            };
            assert_eq!(keep_shown, format!("{primary}; {retained}{hash}{suffix}"));
            assert_eq!(diagnostic(&raw_transfer), raw_transfer);
            assert_eq!(diagnostic(&raw_lease), raw_lease);
            assert_eq!(diagnostic(&raw_keep), raw_keep);
        }
        i18n::set_language("en-US");
    }

    #[test]
    fn stale_supplied_context_cannot_replace_the_original_raw_error() {
        for locale in ["en-US", "de"] {
            i18n::set_language(locale);
            let raw = "original provider error; workspace cleanup: provider detail\n";
            for context in [
                CleanupContext::BundleCompleted {
                    primary: "other",
                    cleanup: "other",
                },
                CleanupContext::ObservationFinish {
                    primary: "other",
                    cleanup: "other",
                },
                CleanupContext::WorkspaceTransfer {
                    primary: "other",
                    cleanup: "other",
                },
                CleanupContext::WorkspaceLeaseRelease {
                    primary: "other",
                    release: "other",
                },
                CleanupContext::BundleKeepRetained {
                    primary: "other",
                    hash: "other",
                },
            ] {
                assert_eq!(cleanup_diagnostic(raw, context), raw);
            }
        }
        i18n::set_language("en-US");
    }
}
