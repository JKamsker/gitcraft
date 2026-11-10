//! Exact fixtures responsibilities.

#[cfg(test)]
pub(super) const EXACT: &[(&str, &str)] = &[
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
