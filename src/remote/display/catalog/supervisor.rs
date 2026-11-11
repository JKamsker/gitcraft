//! Exact diagnostic bindings owned by supervisor behavior.
pub(super) const MESSAGES: &[(&str, &str, Option<u32>)] = &[
    (
        "invalid owner fields",
        "interaction-remote-invalid-owner-fields-74ffc5",
        None,
    ),
    (
        "invalid owner identity",
        "interaction-remote-invalid-owner-identity-6a09c1",
        None,
    ),
    (
        "invalid owner path",
        "interaction-remote-invalid-owner-path-b32461",
        None,
    ),
    (
        "secure randomness unavailable",
        "interaction-remote-secure-randomness-unavailable-20c90a",
        None,
    ),
    (
        "random read failed",
        "interaction-remote-random-read-failed-f00a1a",
        None,
    ),
    (
        "private write failed",
        "interaction-remote-private-write-failed-5e38ee",
        None,
    ),
    (
        "private flush failed",
        "interaction-remote-private-flush-failed-96226b",
        None,
    ),
    (
        "private sync failed",
        "interaction-remote-private-sync-failed-80274a",
        None,
    ),
    (
        "private close failed",
        "interaction-remote-private-close-failed-ce0f62",
        None,
    ),
    (
        "receipt rename failed",
        "interaction-remote-receipt-rename-failed-1c468a",
        None,
    ),
    (
        "receipt open failed",
        "interaction-remote-receipt-open-failed-8fbf2d",
        None,
    ),
    (
        "receipt is not private regular file",
        "interaction-remote-receipt-is-not-private-regular-file-1daae4",
        None,
    ),
    (
        "receipt read failed",
        "interaction-remote-receipt-read-failed-0a27e3",
        None,
    ),
    (
        "oversized receipt",
        "interaction-remote-oversized-receipt-935d09",
        None,
    ),
    (
        "oversized terminal proof",
        "interaction-remote-oversized-terminal-proof-1d7d2f",
        None,
    ),
    (
        "invalid receipt",
        "interaction-remote-invalid-receipt-e32a81",
        None,
    ),
    (
        "unconfirmed receipt",
        "interaction-remote-unconfirmed-receipt-a8916f",
        None,
    ),
    (
        "owner control timeout",
        "interaction-remote-owner-control-timeout-5d1ca1",
        None,
    ),
    (
        "owner endpoint unavailable",
        "interaction-remote-owner-endpoint-unavailable-ecac38",
        None,
    ),
    (
        "control write failed",
        "interaction-remote-control-write-failed-ee827e",
        None,
    ),
    (
        "no atomic acknowledgement",
        "interaction-remote-no-atomic-acknowledgement-c4afcc",
        None,
    ),
    (
        "close-on-exec failed",
        "interaction-remote-close-on-exec-failed-e24737",
        None,
    ),
    (
        "endpoint directory unavailable",
        "interaction-remote-endpoint-directory-unavailable-5f8966",
        None,
    ),
    (
        "endpoint directory is not private",
        "interaction-remote-endpoint-directory-is-not-private-26bb47",
        None,
    ),
    (
        "cannot retain current directory",
        "interaction-remote-cannot-retain-current-directory-44a000",
        None,
    ),
    (
        "cannot pin endpoint directory",
        "interaction-remote-cannot-pin-endpoint-directory-1581de",
        None,
    ),
    (
        "cannot restore current directory",
        "interaction-remote-cannot-restore-current-directory-6ec205",
        None,
    ),
    (
        "missing atomic retirement receipt",
        "interaction-remote-missing-atomic-retirement-receipt-83c75d",
        None,
    ),
    (
        "retirement unlink failed",
        "interaction-remote-retirement-unlink-failed-50bd94",
        None,
    ),
    (
        "owned anchor reap failed",
        "interaction-remote-owned-anchor-reap-failed-22c4e1",
        None,
    ),
    (
        "protocol emission deadline",
        "interaction-remote-protocol-emission-deadline-6b2564",
        None,
    ),
    (
        "protocol emission failed",
        "interaction-remote-protocol-emission-failed-15a7df",
        None,
    ),
    (
        "protocol emission closed",
        "interaction-remote-protocol-emission-closed-cd6e4d",
        None,
    ),
    (
        "ownership emission failed",
        "interaction-remote-ownership-emission-failed-ba5419",
        None,
    ),
    (
        "ownership emission closed",
        "interaction-remote-ownership-emission-closed-660bea",
        None,
    ),
    (
        "cleanup requires full mutation fence",
        "interaction-remote-cleanup-requires-full-mutation-fence-6b093f",
        None,
    ),
    (
        "workspace operations require durable standalone ownership",
        "interaction-remote-workspace-operations-require-durable-standalone-ownership-e082a9",
        None,
    ),
    (
        "invalid standalone capture bound",
        "interaction-remote-invalid-standalone-capture-bound-441dbe",
        None,
    ),
    (
        "invalid request deadline",
        "interaction-remote-invalid-request-deadline-1b116c",
        None,
    ),
    (
        "private owner directory failed",
        "interaction-remote-private-owner-directory-failed-d46a7d",
        None,
    ),
    (
        "private control socket failed",
        "interaction-remote-private-control-socket-failed-b6e253",
        None,
    ),
    (
        "input file failed",
        "interaction-remote-input-file-failed-c87967",
        None,
    ),
    (
        "invalid encoded input",
        "interaction-remote-invalid-encoded-input-c51596",
        None,
    ),
    (
        "input write failed",
        "interaction-remote-input-write-failed-24ba15",
        None,
    ),
    (
        "input seek failed",
        "interaction-remote-input-seek-failed-92e2a8",
        None,
    ),
    ("pipe failed", "interaction-remote-pipe-failed-ef38e3", None),
    (
        "anchor fork failed",
        "interaction-remote-anchor-fork-failed-54c867",
        None,
    ),
    (
        "anchor setsid failed",
        "interaction-remote-anchor-setsid-failed-2c7ed3",
        None,
    ),
    (
        "anchor ready failed",
        "interaction-remote-anchor-ready-failed-e5945b",
        None,
    ),
    (
        "command fork failed",
        "interaction-remote-command-fork-failed-13e1ef",
        None,
    ),
    (
        "command reap failed",
        "interaction-remote-command-reap-failed-c87a81",
        None,
    ),
    (
        "anchor readiness timed out",
        "interaction-remote-anchor-readiness-timed-out-31d10c",
        None,
    ),
    (
        "anchor readiness failed",
        "interaction-remote-anchor-readiness-failed-f1ab60",
        None,
    ),
    (
        "cleanup input reset failed",
        "interaction-remote-cleanup-input-reset-failed-b995a0",
        None,
    ),
    (
        "cleanup proof input failed",
        "interaction-remote-cleanup-proof-input-failed-8edb02",
        None,
    ),
    (
        "cleanup proof flush failed",
        "interaction-remote-cleanup-proof-flush-failed-d4d100",
        None,
    ),
    (
        "cleanup proof rewind failed",
        "interaction-remote-cleanup-proof-rewind-failed-844bfb",
        None,
    ),
    (
        "input close failed",
        "interaction-remote-input-close-failed-7d4966",
        None,
    ),
    (
        "workspace operation expired before dispatch",
        "interaction-remote-workspace-operation-expired-before-dispatch-a3e803",
        None,
    ),
    (
        "invalid command status",
        "interaction-remote-invalid-command-status-4c4bed",
        None,
    ),
    (
        "anchor lost before command status",
        "interaction-remote-anchor-lost-before-command-status-562088",
        None,
    ),
    (
        "anchor release failed",
        "interaction-remote-anchor-release-failed-32fe1b",
        None,
    ),
    (
        "oversized request frame",
        "interaction-remote-oversized-request-frame-30def4",
        None,
    ),
    (
        "incomplete request deadline",
        "interaction-remote-incomplete-request-deadline-781dd7",
        None,
    ),
    (
        "incomplete request frame",
        "interaction-remote-incomplete-request-frame-e578eb",
        None,
    ),
    (
        "controller missing",
        "interaction-remote-controller-missing-8734f3",
        None,
    ),
    (
        "controller frame deadline",
        "interaction-remote-controller-frame-deadline-5d4c27",
        None,
    ),
    (
        "controller read failed",
        "interaction-remote-controller-read-failed-eea8fc",
        None,
    ),
    (
        "invalid retirement acknowledgement",
        "interaction-remote-invalid-retirement-acknowledgement-dc5be6",
        None,
    ),
    (
        "supervisor fork failed",
        "interaction-remote-supervisor-fork-failed-75cde5",
        None,
    ),
    (
        "supervisor setsid failed",
        "interaction-remote-supervisor-setsid-failed-f3e8d5",
        None,
    ),
    (
        "unknown owner operation",
        "interaction-remote-unknown-owner-operation-463d32",
        None,
    ),
    (
        "invalid terminal receipt",
        "interaction-remote-invalid-terminal-receipt-a5d8cb",
        None,
    ),
    (
        "unconfirmed cleanup",
        "interaction-remote-unconfirmed-cleanup-d50ac5",
        None,
    ),
    (
        "owner control deadline",
        "interaction-remote-owner-control-deadline-036b1e",
        None,
    ),
    (
        "incomplete acknowledgement",
        "interaction-remote-incomplete-acknowledgement-bb13f8",
        None,
    ),
    (
        "POSIX ownership unavailable",
        "interaction-remote-posix-ownership-unavailable-ce3a2c",
        None,
    ),
    (
        "receipt is not a bounded private regular file",
        "interaction-remote-receipt-is-not-a-bounded-private-regular-file-6dce71",
        None,
    ),
    (
        "anchor did not establish ownership",
        "interaction-remote-anchor-did-not-establish-ownership-7ba3c2",
        None,
    ),
    (
        "anchor lost before status",
        "interaction-remote-anchor-lost-before-status-70db85",
        None,
    ),
];
