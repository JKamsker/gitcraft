//! Exact diagnostic bindings owned by authority behavior.
pub(super) const MESSAGES: &[(&str, &str, Option<u32>)] = &[
    (
        "workspace identity unavailable",
        "interaction-remote-workspace-identity-unavailable-5b5ae8",
        None,
    ),
    (
        "workspace namespace replaced",
        "interaction-remote-workspace-namespace-replaced-fca98a",
        None,
    ),
    (
        "stale workspace generation; no mutation authorized",
        "interaction-remote-stale-workspace-generation-no-mutation-authorized-1180d3",
        None,
    ),
    (
        "workspace file is not private single-link regular file",
        "interaction-remote-workspace-file-is-not-private-single-link-regular-file-ab1a80",
        None,
    ),
    (
        "workspace seek failed",
        "interaction-remote-workspace-seek-failed-ba7ade",
        None,
    ),
    (
        "workspace read failed",
        "interaction-remote-workspace-read-failed-1ea49d",
        None,
    ),
    (
        "oversized workspace file",
        "interaction-remote-oversized-workspace-file-924474",
        None,
    ),
    (
        "cannot retain workspace cwd",
        "interaction-remote-cannot-retain-workspace-cwd-3743a2",
        None,
    ),
    (
        "cannot pin workspace directory",
        "interaction-remote-cannot-pin-workspace-directory-43a7e0",
        None,
    ),
    (
        "cannot restore workspace cwd",
        "interaction-remote-cannot-restore-workspace-cwd-e3825d",
        None,
    ),
    (
        "workspace directory unavailable",
        "interaction-remote-workspace-directory-unavailable-6c98ec",
        None,
    ),
    (
        "workspace directory is not private",
        "interaction-remote-workspace-directory-is-not-private-ab9452",
        None,
    ),
    (
        "workspace anchor replaced or incomplete",
        "interaction-remote-workspace-anchor-replaced-or-incomplete-65878d",
        None,
    ),
    (
        "workspace root must be absolute",
        "interaction-remote-workspace-root-must-be-absolute-fb6376",
        None,
    ),
    (
        "workspace anchor unavailable",
        "interaction-remote-workspace-anchor-unavailable-3f243c",
        None,
    ),
    (
        "workspace namespace already exists or cannot initialize",
        "interaction-remote-workspace-namespace-already-exists-or-cannot-initialize-98837d",
        None,
    ),
    (
        "workspace initialization claim refused",
        "interaction-remote-workspace-initialization-claim-refused-4c0b07",
        None,
    ),
    (
        "workspace lease expired",
        "interaction-remote-workspace-lease-expired-a93d2b",
        None,
    ),
    (
        "workspace controller lost",
        "interaction-remote-workspace-controller-lost-3a3090",
        None,
    ),
    (
        "unexpected workspace controller input",
        "interaction-remote-unexpected-workspace-controller-input-a017ce",
        None,
    ),
    (
        "workspace fence locking unsupported",
        "interaction-remote-workspace-fence-locking-unsupported-8f302c",
        None,
    ),
    (
        "invalid workspace generation",
        "interaction-remote-invalid-workspace-generation-c2c7d3",
        None,
    ),
    (
        "another workspace claimant owns repository",
        "interaction-remote-another-workspace-claimant-owns-repository-429daf",
        None,
    ),
    (
        "generation seek failed",
        "interaction-remote-generation-seek-failed-27e896",
        None,
    ),
    (
        "generation truncate failed",
        "interaction-remote-generation-truncate-failed-fd4dd2",
        None,
    ),
    (
        "generation write failed",
        "interaction-remote-generation-write-failed-b88dc1",
        None,
    ),
    (
        "generation sync failed",
        "interaction-remote-generation-sync-failed-1398b3",
        None,
    ),
    (
        "fence release failed",
        "interaction-remote-fence-release-failed-b9305b",
        None,
    ),
    (
        "invalid workspace grant",
        "interaction-remote-invalid-workspace-grant-f6ff75",
        None,
    ),
    (
        "workspace primary owner no longer live",
        "interaction-remote-workspace-primary-owner-no-longer-live-1969f5",
        None,
    ),
    (
        "workspace primary ownership unknown",
        "interaction-remote-workspace-primary-ownership-unknown-293762",
        None,
    ),
    (
        "owner record outside workspace ledger",
        "interaction-remote-owner-record-outside-workspace-ledger-0dcb41",
        None,
    ),
    (
        "owner record generation/kind mismatch",
        "interaction-remote-owner-record-generation-kind-mismatch-3fa17c",
        None,
    ),
    (
        "oversized workspace owner",
        "interaction-remote-oversized-workspace-owner-88d9fa",
        None,
    ),
    (
        "invalid workspace lifetime",
        "interaction-remote-invalid-workspace-lifetime-714dc5",
        None,
    ),
    (
        "invalid bound workspace record",
        "interaction-remote-invalid-bound-workspace-record-56c144",
        None,
    ),
    (
        "claim controller missing",
        "interaction-remote-claim-controller-missing-0a8265",
        None,
    ),
    (
        "claim request removal failed",
        "interaction-remote-claim-request-removal-failed-686234",
        None,
    ),
    (
        "claim request directory removal failed",
        "interaction-remote-claim-request-directory-removal-failed-855736",
        None,
    ),
    (
        "request missing",
        "interaction-remote-request-missing-d3b49d",
        None,
    ),
    (
        "workspace lock namespace replaced; no authority",
        "interaction-remote-workspace-lock-namespace-replaced-no-authority-882d9e",
        None,
    ),
    (
        "workspace fence expired",
        "interaction-remote-workspace-fence-expired-dfcaa0",
        None,
    ),
    (
        "invalid workspace claim lifetime",
        "interaction-remote-invalid-workspace-claim-lifetime-4a943d",
        None,
    ),
    (
        "workspace owner binding mismatch",
        "interaction-remote-workspace-owner-binding-mismatch-ac9967",
        None,
    ),
    (
        "workspace record name/generation mismatch",
        "interaction-remote-workspace-record-name-generation-mismatch-974341",
        None,
    ),
    (
        "partial generation publication",
        "interaction-remote-partial-generation-publication-7197c4",
        None,
    ),
    (
        "owner record generation/kind/ledger mismatch",
        "interaction-remote-owner-record-generation-kind-ledger-mismatch-709511",
        None,
    ),
    (
        "oversized claim request",
        "interaction-remote-oversized-claim-request-1d1d3b",
        None,
    ),
    (
        "workspace owner record is outside its ledger",
        "interaction-remote-workspace-owner-record-is-outside-its-ledger-6f2a03",
        None,
    ),
    (
        "invalid workspace owner record name",
        "interaction-remote-invalid-workspace-owner-record-name-b301d4",
        None,
    ),
    (
        "invalid workspace owner nonce/generation",
        "interaction-remote-invalid-workspace-owner-nonce-generation-f4c9a5",
        None,
    ),
    (
        "invalid workspace owner binding",
        "interaction-remote-invalid-workspace-owner-binding-6d22b2",
        None,
    ),
    (
        "workspace owner record exceeds 2048 bytes",
        "interaction-remote-workspace-owner-record-exceeds-2048-bytes-f94a0a",
        Some(2048),
    ),
    (
        "workspace owner anchor/version mismatch",
        "interaction-remote-workspace-owner-anchor-version-mismatch-358fb3",
        None,
    ),
    (
        "oversized owner record",
        "interaction-remote-oversized-owner-record-f0a485",
        None,
    ),
    (
        "workspace claim cancelled or handshake expired",
        "interaction-remote-workspace-claim-cancelled-or-handshake-expired-ba9a12",
        None,
    ),
    (
        "workspace claim lost; mutation refused",
        "interaction-remote-workspace-claim-lost-mutation-refused-5ad937",
        None,
    ),
    (
        "workspace lease release unknown",
        "interaction-remote-workspace-lease-release-unknown-0b41d6",
        None,
    ),
    (
        "invalid workspace claim request",
        "interaction-remote-invalid-workspace-claim-request-a436df",
        None,
    ),
    (
        "invalid workspace claim lifetime/root",
        "interaction-remote-invalid-workspace-claim-lifetime-root-9d0439",
        None,
    ),
    (
        "workspace claim cancelled before acknowledgement",
        "interaction-remote-workspace-claim-cancelled-before-acknowledgement-d9b0d0",
        None,
    ),
];
