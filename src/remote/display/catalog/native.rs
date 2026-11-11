//! Exact diagnostic bindings owned by native behavior.
pub(super) const MESSAGES: &[(&str, &str, Option<u32>)] = &[
    (
        "owner record has no parent",
        "interaction-remote-owner-record-has-no-parent-b051eb",
        None,
    ),
    (
        "invalid owner name",
        "interaction-remote-invalid-owner-name-63630a",
        None,
    ),
    (
        "missing, replaced or invalid owner record; cleanup unknown",
        "interaction-remote-missing-replaced-or-invalid-owner-record-cleanup-unknown-c64053",
        None,
    ),
    (
        "owner record changed during recovery; workspace preserved",
        "interaction-remote-owner-record-changed-during-recovery-workspace-preserved-793bcf",
        None,
    ),
    (
        "foreign native command encoding",
        "interaction-remote-foreign-native-command-encoding-396128",
        None,
    ),
    (
        "oversized native owner record",
        "interaction-remote-oversized-native-owner-record-8a88d0",
        None,
    ),
    (
        "invalid native owner endpoint",
        "interaction-remote-invalid-native-owner-endpoint-b89cc5",
        None,
    ),
    (
        "native terminal receipt unavailable",
        "interaction-remote-native-terminal-receipt-unavailable-540c51",
        None,
    ),
    (
        "unconfirmed native terminal receipt",
        "interaction-remote-unconfirmed-native-terminal-receipt-c21ac6",
        None,
    ),
    (
        "invalid cached native retirement proof",
        "interaction-remote-invalid-cached-native-retirement-proof-8215d9",
        None,
    ),
    (
        "native owner unavailable; cleanup unknown",
        "interaction-remote-native-owner-unavailable-cleanup-unknown-9ce063",
        None,
    ),
    (
        "native stop was not acknowledged",
        "interaction-remote-native-stop-was-not-acknowledged-58e1ce",
        None,
    ),
    (
        "native durable input exceeds 64 MiB",
        "interaction-remote-native-durable-input-exceeds-64-mib-95c31f",
        Some(64),
    ),
    (
        "native request exceeds 96 MiB",
        "interaction-remote-native-request-exceeds-96-mib-ebea84",
        Some(96),
    ),
    (
        "missing native completion endpoint",
        "interaction-remote-missing-native-completion-endpoint-2c2457",
        None,
    ),
    (
        "native command stopped but output delivery is incomplete",
        "interaction-remote-native-command-stopped-but-output-delivery-is-incomplete-56c745",
        None,
    ),
    (
        "native owner handshake missing; cleanup unknown",
        "interaction-remote-native-owner-handshake-missing-cleanup-unknown-8332db",
        None,
    ),
    (
        "native cleanup unconfirmed",
        "interaction-remote-native-cleanup-unconfirmed-0f9fee",
        None,
    ),
    (
        "invalid native request",
        "interaction-remote-invalid-native-request-2d03d0",
        None,
    ),
    (
        "invalid native request identity/deadline",
        "interaction-remote-invalid-native-request-identity-deadline-1861a9",
        None,
    ),
    (
        "cleanup requires full actual-machine mutation authority",
        "interaction-remote-cleanup-requires-full-actual-machine-mutation-authority-4fd7be",
        None,
    ),
    (
        "missing actual workspace fence",
        "interaction-remote-missing-actual-workspace-fence-62f872",
        None,
    ),
    (
        "private directory creation requires workspace authority",
        "interaction-remote-private-directory-creation-requires-workspace-authority-304012",
        None,
    ),
    (
        "private directory is outside claimed workspace",
        "interaction-remote-private-directory-is-outside-claimed-workspace-c8929b",
        None,
    ),
    (
        "private directory creation cancelled before mutation",
        "interaction-remote-private-directory-creation-cancelled-before-mutation-d9c6eb",
        None,
    ),
    (
        "missing directory fence",
        "interaction-remote-missing-directory-fence-83c3b2",
        None,
    ),
    (
        "cleanup authority missing",
        "interaction-remote-cleanup-authority-missing-fac2c0",
        None,
    ),
    (
        "oversized native input",
        "interaction-remote-oversized-native-input-16f983",
        None,
    ),
];
