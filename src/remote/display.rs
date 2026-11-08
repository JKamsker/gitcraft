//! Finite presentation adapters for raw remote diagnostics and queued notices.
//!
//! Call only at the error/notice paint boundary, after every raw protocol/control check.
//! Hosts (including Host::label), paths, commands and external details keep their bytes.
//! This module never launches a helper, reads a repository or changes a producer result.

use crate::i18n::{message, text};

// Exact application-owned diagnostics, source-bound in the handoff inventory. Shared
// helper spellings intentionally resolve to one ID; unknown external text stays opaque.
const KNOWN: &[(&str, &str, Option<u32>)] = &[
    (
        "no app to ask",
        "interaction-remote-no-app-to-ask-786f42",
        None,
    ),
    (
        "oversized protocol frame",
        "interaction-remote-oversized-protocol-frame-88ad94",
        None,
    ),
    (
        "incomplete protocol frame",
        "interaction-remote-incomplete-protocol-frame-167d04",
        None,
    ),
    (
        "protocol write stopped",
        "interaction-remote-protocol-write-stopped-0153bd",
        None,
    ),
    (
        "absolute protocol deadline expired",
        "interaction-remote-absolute-protocol-deadline-expired-89de09",
        None,
    ),
    ("not a file", "interaction-remote-not-a-file-2cd310", None),
    (
        "it is a symbolic link",
        "interaction-remote-it-is-a-symbolic-link-f4fd09",
        None,
    ),
    (
        "can't copy a file between two machines",
        "interaction-remote-can-t-copy-a-file-between-two-machines-a1542e",
        None,
    ),
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
    (
        "remote shell unavailable",
        "interaction-remote-remote-shell-unavailable-a6840b",
        None,
    ),
    (
        "missing output completion endpoint",
        "interaction-remote-missing-output-completion-endpoint-08c523",
        None,
    ),
    (
        "no endpoint to retire",
        "interaction-remote-no-endpoint-to-retire-43ee9a",
        None,
    ),
    (
        "no ownership handshake; cleanup remains unknown",
        "interaction-remote-no-ownership-handshake-cleanup-remains-unknown-01597e",
        None,
    ),
    (
        "the supervisor did not confirm its atomic stop/reap receipt",
        "interaction-remote-the-supervisor-did-not-confirm-its-atomic-stop-reap-receipt-cbce6c",
        None,
    ),
    (
        "private endpoint evidence retained",
        "interaction-remote-private-endpoint-evidence-retained-b5d669",
        None,
    ),
    (
        "owned command stopped but output delivery is incomplete or unconfirmed",
        "interaction-remote-owned-command-stopped-but-output-delivery-is-incomplete-or-unconfirmed-b6a082",
        None,
    ),
    (
        "it took too long; the owned supervisor confirmed stop and reap",
        "interaction-remote-it-took-too-long-the-owned-supervisor-confirmed-stop-and-reap-f892b4",
        None,
    ),
    (
        "it took too long; cleanup is unknown and recovery evidence was retained",
        "interaction-remote-it-took-too-long-cleanup-is-unknown-and-recovery-evidence-was-retained-a03af7",
        None,
    ),
    (
        "remote controller closed",
        "interaction-remote-remote-controller-closed-936232",
        None,
    ),
    (
        "the connection closed",
        "interaction-remote-the-connection-closed-fb422b",
        None,
    ),
    (
        "it didn't answer in time",
        "interaction-remote-it-didn-t-answer-in-time-7ec0bb",
        None,
    ),
    (
        "it answered out of turn; cleanup unknown",
        "interaction-remote-it-answered-out-of-turn-cleanup-unknown-0a6d04",
        None,
    ),
    (
        "invalid ownership handshake; cleanup unknown",
        "interaction-remote-invalid-ownership-handshake-cleanup-unknown-fd5e10",
        None,
    ),
    (
        "missing terminal receipt; cleanup unknown",
        "interaction-remote-missing-terminal-receipt-cleanup-unknown-43cde7",
        None,
    ),
    (
        "invalid output header",
        "interaction-remote-invalid-output-header-b2210f",
        None,
    ),
    (
        "owned command stopped but output delivery is incomplete",
        "interaction-remote-owned-command-stopped-but-output-delivery-is-incomplete-8da45f",
        None,
    ),
    (
        "controller lost; cleanup unknown",
        "interaction-remote-controller-lost-cleanup-unknown-e1e737",
        None,
    ),
    (
        "output delivery lost",
        "interaction-remote-output-delivery-lost-d05d87",
        None,
    ),
    (
        "oversized ownership record",
        "interaction-remote-oversized-ownership-record-42ba81",
        None,
    ),
    (
        "invalid ownership endpoint",
        "interaction-remote-invalid-ownership-endpoint-79c82e",
        None,
    ),
    (
        "invalid request nonce",
        "interaction-remote-invalid-request-nonce-777b5d",
        None,
    ),
    (
        "remote input exceeds 64 MiB",
        "interaction-remote-remote-input-exceeds-64-mib-de2fb3",
        Some(64),
    ),
    (
        "invalid remote execution deadline",
        "interaction-remote-invalid-remote-execution-deadline-94cb62",
        None,
    ),
    (
        "invalid standalone capture limit",
        "interaction-remote-invalid-standalone-capture-limit-879799",
        None,
    ),
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
        "record generation/name mismatch",
        "interaction-remote-record-generation-name-mismatch-e1f4e5",
        None,
    ),
    (
        "invalid bounded proof",
        "interaction-remote-invalid-bounded-proof-9e27e7",
        None,
    ),
    (
        "invalid proof base64",
        "interaction-remote-invalid-proof-base64-1b8911",
        None,
    ),
    (
        "changed ledger evidence",
        "interaction-remote-changed-ledger-evidence-47ddc4",
        None,
    ),
    (
        "workspace exceeds 4096 recovered owners",
        "interaction-remote-workspace-exceeds-4096-recovered-owners-f24536",
        Some(4096),
    ),
    (
        "ledger close failed",
        "interaction-remote-ledger-close-failed-5a8865",
        None,
    ),
    (
        "ledger publication bound",
        "interaction-remote-ledger-publication-bound-e1b032",
        None,
    ),
    (
        "owner set changed during recovery",
        "interaction-remote-owner-set-changed-during-recovery-461fb5",
        None,
    ),
    (
        "ledger entry changed before pruning",
        "interaction-remote-ledger-entry-changed-before-pruning-d9daae",
        None,
    ),
    (
        "ledger removal failed",
        "interaction-remote-ledger-removal-failed-9e792c",
        None,
    ),
    (
        "cannot inspect pinned artifacts; ledger retained",
        "interaction-remote-cannot-inspect-pinned-artifacts-ledger-retained-94158c",
        None,
    ),
    (
        "missing pinned terminal with present artifacts; ledger retained",
        "interaction-remote-missing-pinned-terminal-with-present-artifacts-ledger-retained-fa0ed6",
        None,
    ),
    (
        "pinned terminal differs from cached proof; ledger retained",
        "interaction-remote-pinned-terminal-differs-from-cached-proof-ledger-retained-49bdd7",
        None,
    ),
    (
        "proven retirement unlink failed",
        "interaction-remote-proven-retirement-unlink-failed-7cd0bb",
        None,
    ),
    (
        "ledger inventory unavailable",
        "interaction-remote-ledger-inventory-unavailable-b3e1fd",
        None,
    ),
    (
        "workspace inventory bound; preserved",
        "interaction-remote-workspace-inventory-bound-preserved-080187",
        None,
    ),
    (
        "ledger inventory close failed",
        "interaction-remote-ledger-inventory-close-failed-5d993f",
        None,
    ),
    (
        "unknown external ledger evidence",
        "interaction-remote-unknown-external-ledger-evidence-09e304",
        None,
    ),
    (
        "cleanup owner not indexed",
        "interaction-remote-cleanup-owner-not-indexed-a84680",
        None,
    ),
    (
        "invalid cleaned manifest",
        "interaction-remote-invalid-cleaned-manifest-63cfbf",
        None,
    ),
    (
        "owner changed after cleaned proof",
        "interaction-remote-owner-changed-after-cleaned-proof-ad0d75",
        None,
    ),
    (
        "proof name mismatch",
        "interaction-remote-proof-name-mismatch-1e4dfe",
        None,
    ),
    (
        "orphan or changed proof without cleaned manifest",
        "interaction-remote-orphan-or-changed-proof-without-cleaned-manifest-3ac43e",
        None,
    ),
    (
        "proof changed before pruning",
        "interaction-remote-proof-changed-before-pruning-e12d53",
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
        "unknown owner operation",
        "interaction-remote-unknown-owner-operation-463d32",
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
        "invalid bounded ledger proof",
        "interaction-remote-invalid-bounded-ledger-proof-b815f5",
        None,
    ),
    (
        "workspace recovery exceeds 4096 owners",
        "interaction-remote-workspace-recovery-exceeds-4096-owners-0dcb1b",
        Some(4096),
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
        "ledger evidence changed",
        "interaction-remote-ledger-evidence-changed-111481",
        None,
    ),
    (
        "ledger evidence exceeds bound",
        "interaction-remote-ledger-evidence-exceeds-bound-14f03d",
        None,
    ),
    (
        "ledger publication exceeds bound",
        "interaction-remote-ledger-publication-exceeds-bound-c1e603",
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
        "orphan or changed proof without completed manifest",
        "interaction-remote-orphan-or-changed-proof-without-completed-manifest-21e1d3",
        None,
    ),
    (
        "owner record changed during recovery",
        "interaction-remote-owner-record-changed-during-recovery-c84182",
        None,
    ),
    (
        "anchor did not establish ownership",
        "interaction-remote-anchor-did-not-establish-ownership-7ba3c2",
        None,
    ),
    (
        "oversized claim request",
        "interaction-remote-oversized-claim-request-1d1d3b",
        None,
    ),
    (
        "workspace inventory bound; evidence retained",
        "interaction-remote-workspace-inventory-bound-evidence-retained-f139e7",
        None,
    ),
    (
        "anchor lost before status",
        "interaction-remote-anchor-lost-before-status-70db85",
        None,
    ),
    (
        "SSH multiplexing is unavailable",
        "interaction-remote-ssh-multiplexing-is-unavailable-9c3e39",
        None,
    ),
    (
        "dynamic SSH Match condition",
        "interaction-remote-dynamic-ssh-match-condition-8b6012",
        None,
    ),
    (
        "relative system SSH Include",
        "interaction-remote-relative-system-ssh-include-b64630",
        None,
    ),
    (
        "SSH config root needs literal filename handling",
        "interaction-remote-ssh-config-root-needs-literal-filename-handling-0cd61a",
        None,
    ),
    (
        "SSH configuration path cannot be quoted",
        "interaction-remote-ssh-configuration-path-cannot-be-quoted-542feb",
        None,
    ),
    (
        "this portable copy of gitcraft doesn't use the system's credential store (Settings → General → Portable mode)",
        "interaction-remote-this-portable-copy-of-gitcraft-doesn-t-use-the-system-s-credential-store-set-10836a",
        None,
    ),
    (
        "no folder for this app's files",
        "interaction-remote-no-folder-for-this-app-s-files-15138c",
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
    (
        "oversized ledger entry",
        "interaction-remote-oversized-ledger-entry-ca8a88",
        None,
    ),
    (
        "oversized ledger proof",
        "interaction-remote-oversized-ledger-proof-b5739c",
        None,
    ),
    (
        "ledger receipt does not authenticate its exact owner",
        "interaction-remote-ledger-receipt-does-not-authenticate-its-exact-owner-661742",
        None,
    ),
    (
        "ledger evidence changed or collided",
        "interaction-remote-ledger-evidence-changed-or-collided-bf5019",
        None,
    ),
    (
        "workspace recovery exceeds 4096 owners; evidence retained",
        "interaction-remote-workspace-recovery-exceeds-4096-owners-evidence-retained-b69bde",
        Some(4096),
    ),
    (
        "non-text workspace ledger name",
        "interaction-remote-non-text-workspace-ledger-name-dbf02b",
        None,
    ),
    (
        "workspace ledger inventory limit; evidence retained",
        "interaction-remote-workspace-ledger-inventory-limit-evidence-retained-4296a4",
        None,
    ),
    (
        "cleanup owner is not durably indexed",
        "interaction-remote-cleanup-owner-is-not-durably-indexed-0a99b5",
        None,
    ),
    (
        "cleanup cancelled or expired; ledger retained",
        "interaction-remote-cleanup-cancelled-or-expired-ledger-retained-9b5347",
        None,
    ),
    (
        "invalid cleaned ledger manifest",
        "interaction-remote-invalid-cleaned-ledger-manifest-70a10f",
        None,
    ),
    (
        "owner changed after cleaned proof; evidence retained",
        "interaction-remote-owner-changed-after-cleaned-proof-evidence-retained-d8ed05",
        None,
    ),
    (
        "orphan owner proof without cleaned manifest; recovery unknown",
        "interaction-remote-orphan-owner-proof-without-cleaned-manifest-recovery-unknown-3b74ce",
        None,
    ),
    (
        "cached owner proof changed",
        "interaction-remote-cached-owner-proof-changed-a22114",
        None,
    ),
    (
        "cleanup cancelled; external evidence retained",
        "interaction-remote-cleanup-cancelled-external-evidence-retained-9a1d80",
        None,
    ),
    (
        "oversized authenticated owner proof",
        "interaction-remote-oversized-authenticated-owner-proof-b7dccb",
        None,
    ),
    (
        "external ledger directory replaced",
        "interaction-remote-external-ledger-directory-replaced-24d380",
        None,
    ),
    (
        "owner record set changed during recovery",
        "interaction-remote-owner-record-set-changed-during-recovery-23a825",
        None,
    ),
    (
        "cleaned manifest exceeds limit; external proofs retained",
        "interaction-remote-cleaned-manifest-exceeds-limit-external-proofs-retained-f98707",
        None,
    ),
    (
        "proof changed before pruning; manifest retained",
        "interaction-remote-proof-changed-before-pruning-manifest-retained-a0b8a3",
        None,
    ),
    (
        "gitcraft: invalid ownership handshake",
        "interaction-remote-gitcraft-invalid-ownership-handshake-2aa41f",
        None,
    ),
    (
        "gitcraft: oversized ownership handshake",
        "interaction-remote-gitcraft-oversized-ownership-handshake-f49fa3",
        None,
    ),
    (
        "gitcraft: safe execution needs Python 3 POSIX support or Perl with its core POSIX/JSON modules; install either on this machine",
        "interaction-remote-gitcraft-safe-execution-needs-python-3-posix-support-or-perl-with-its-core-p-89a11a",
        None,
    ),
    (
        "proof filename mismatch",
        "interaction-remote-proof-filename-mismatch-e0adae",
        None,
    ),
    ("cancelled", "interaction-remote-cancelled-8b4704", None),
];

// Only these complete envelopes may interpolate a single opaque value. Never replace
// words or substrings inside an external payload. Numeric slots must parse canonically.
const ENVELOPES: &[(&str, &str, &str, &str)] = &[
    ("exit status ", "", "interaction-remote-exit-status", "code"),
    (
        "Forgot ",
        " saved sign-ins.",
        "interaction-remote-forgot-many",
        "count",
    ),
    ("could not reach ", "", "interaction-remote-reach", "host"),
    (
        "invalid ownership record: ",
        "",
        "interaction-remote-invalid-record",
        "detail",
    ),
    (
        "cleanup partial; external ledger retained: ",
        "",
        "interaction-remote-cleanup-partial",
        "detail",
    ),
    (
        "gitcraft owned endpoint: ",
        "",
        "interaction-remote-owned-endpoint",
        "detail",
    ),
    (
        "workspace claim unavailable: ",
        "",
        "interaction-remote-claim",
        "detail",
    ),
    (
        "gitcraft workspace claim: ",
        "",
        "interaction-remote-claim-helper",
        "detail",
    ),
    (
        "oversized ledger entry ",
        "",
        "interaction-remote-ledger-entry",
        "name",
    ),
    (
        "unknown ledger evidence ",
        "; preserved",
        "interaction-remote-ledger-evidence",
        "name",
    ),
    (
        "",
        " isn't gitcraft's",
        "interaction-remote-not-owned",
        "path",
    ),
    (
        "",
        " isn't a scratch folder",
        "interaction-remote-not-scratch",
        "path",
    ),
    (
        "A folder on ",
        ": /home/me/folder or ~/folder",
        "interaction-remote-folder-example",
        "host",
    ),
    (
        "That folder is on ",
        ", the repository here",
        "interaction-remote-folder-here",
        "host",
    ),
    (
        "private file: ",
        "",
        "interaction-remote-private-file",
        "detail",
    ),
    (
        "workspace open failed: ",
        "",
        "interaction-remote-workspace-open",
        "detail",
    ),
    (
        "owned signal refused: ",
        "",
        "interaction-remote-owned-signal",
        "detail",
    ),
    (
        "",
        "; command delivery/cleanup unknown",
        "interaction-remote-delivery-unknown",
        "detail",
    ),
    (
        "gitcraft: no such folder: ",
        "",
        "interaction-remote-no-folder",
        "path",
    ),
];

fn known(raw: &str) -> Option<String> {
    let (_, id, limit) = KNOWN.iter().find(|(source, _, _)| *source == raw)?;
    Some(match limit {
        Some(limit) => message(id, &[("limit", (*limit).into())]),
        None => text(id).to_owned(),
    })
}

/// Render only the finite remote-owned error grammar. Pass original producer bytes;
/// never store this result or use it for retry, cleanup, authentication or selection.
#[must_use]
pub fn diagnostic(raw: &str) -> String {
    render(raw).unwrap_or_else(|| raw.to_owned())
}

fn render(raw: &str) -> Option<String> {
    if let Some(value) = known(raw) {
        return Some(value);
    }
    // Preserve the exact newline of the known marker/Perl control diagnostics.
    if let Some(line) = raw.strip_suffix('\n')
        && matches!(
            line,
            "gitcraft: invalid ownership handshake"
                | "gitcraft: oversized ownership handshake"
                | "workspace lease expired"
                | "workspace controller lost"
        )
    {
        return known(line).map(|line| format!("{line}\n"));
    }
    match raw {
        "No saved sign-ins to forget." => {
            return Some(text("interaction-remote-no-saved").to_owned());
        }
        "Forgot 1 saved sign-in." => {
            return Some(message(
                "interaction-remote-forgot-many",
                &[("count", 1.into())],
            ));
        }
        _ => {}
    }
    if let Some(value) = two_arguments(raw) {
        return Some(value);
    }
    for &(prefix, suffix, id, name) in ENVELOPES {
        if let Some(value) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
        {
            if name == "code" {
                let number = value.parse::<i32>().ok()?;
                if number.to_string() == value {
                    return Some(message(id, &[(name, number.into())]));
                }
            } else if name == "count" {
                let number = value.parse::<u32>().ok()?;
                if number.to_string() == value {
                    return Some(message(id, &[(name, number.into())]));
                }
            } else {
                return Some(message(id, &[(name, value.into())]));
            }
        }
    }
    None
}

fn pair(id: &str, first: (&str, &str), second: (&str, &str)) -> String {
    message(
        id,
        &[(first.0, first.1.into()), (second.0, second.1.into())],
    )
}

fn two_arguments(raw: &str) -> Option<String> {
    if let Some((host, detail)) = raw
        .strip_suffix(").")
        .and_then(|s| s.split_once(": the password couldn't be saved ("))
    {
        // vault errors are raw, and may include the exact portable-store diagnostic.
        let detail = known(detail).unwrap_or_else(|| detail.to_owned());
        return Some(pair(
            "interaction-remote-password-save",
            ("host", host),
            ("detail", &detail),
        ));
    }
    for (prefix, separator, id) in [
        ("could not reach ", ": ", "interaction-remote-reach-detail"),
        (
            "could not prepare ",
            " input: ",
            "interaction-remote-prepare-input",
        ),
        (
            "could not write a file on ",
            ": ",
            "interaction-remote-write-file",
        ),
        (
            "could not make a scratch folder on ",
            ": ",
            "interaction-remote-scratch-folder",
        ),
        (
            "lost the connection to ",
            ": ",
            "interaction-remote-lost-connection",
        ),
        (
            "That folder is on ",
            ", the repository on ",
            "interaction-remote-folder-there",
        ),
    ] {
        if let Some((host, detail)) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.split_once(separator))
        {
            if id == "interaction-remote-folder-there" {
                return Some(pair(id, ("host", host), ("repository_host", detail)));
            }
            if id == "interaction-remote-reach-detail" {
                for (suffix, id) in [
                    (
                        " - its host key wasn't trusted",
                        "interaction-remote-reach-untrusted",
                    ),
                    (
                        " - check the user, the key or the password",
                        "interaction-remote-reach-permission",
                    ),
                ] {
                    if let Some(detail) = detail.strip_suffix(suffix) {
                        return Some(pair(id, ("host", host), ("detail", detail)));
                    }
                }
            }
            return Some(pair(id, ("host", host), ("detail", detail)));
        }
    }
    if let Some(body) = raw.strip_suffix(": programs of this machine can't open it")
        && let Some((path, host)) = body.rsplit_once(" is on ")
    {
        return Some(if host == "another machine" {
            message(
                "interaction-remote-cannot-open-other",
                &[("path", path.into())],
            )
        } else {
            pair(
                "interaction-remote-cannot-open",
                ("path", path),
                ("host", host),
            )
        });
    }
    for (separator, id) in [
        (
            "; atomic cleanup proof unavailable: ",
            "interaction-remote-atomic-proof",
        ),
        (
            "; NotDispatched proof publication failed: ",
            "interaction-remote-not-dispatched-proof",
        ),
    ] {
        if let Some((detail, proof)) = raw.split_once(separator) {
            return Some(pair(id, ("detail", detail), ("proof", proof)));
        }
    }
    if let Some(body) = raw
        .strip_prefix("every connection to ")
        .and_then(|s| s.strip_suffix(" is busy; try again when what runs there is done"))
    {
        return Some(message(
            "interaction-remote-connections-busy",
            &[("host", body.into())],
        ));
    }
    if let Some((count, body)) = raw.split_once(" diff or merge tools are already open on ")
        && let Some(host) = body.strip_suffix(": close one of them first")
        && let Ok(count_value) = count.parse::<u32>()
        && count_value.to_string() == count
    {
        return Some(message(
            "interaction-remote-tools-busy",
            &[("count", count_value.into()), ("host", host.into())],
        ));
    }
    if let Some(command) = raw.strip_prefix("gitcraft asked another machine for ")
        .and_then(|s| s.strip_suffix(" from the window's thread, which must never wait for a network: please report this"))
    {
        return Some(message("interaction-remote-ui-thread", &[("command", command.into())]));
    }
    None
}

/// Session errors include an opaque command before a host. The caller must supply the
/// same command/host that produced the error; never infer a command from arbitrary stderr.
#[must_use]
pub fn execution_error(raw: &str, command: &str, host: &super::Host) -> String {
    let label = host.label();
    let prefix = format!("{command} on {label}: ");
    if let Some(detail) = raw.strip_prefix(&prefix) {
        return message(
            "interaction-remote-execution",
            &[
                ("command", command.into()),
                ("host", label.into()),
                ("detail", diagnostic(detail).into()),
            ],
        );
    }
    let prefix = format!("{command} didn't answer in time: ");
    if let Some(detail) = raw.strip_prefix(&prefix) {
        return pair(
            "interaction-remote-no-answer",
            ("command", command),
            ("detail", &diagnostic(detail)),
        );
    }
    diagnostic(raw)
}

/// Python's exception type and Perl's source location are diagnostic metadata, kept
/// verbatim. Only an exact known application message is translated; tracebacks and
/// unknown OS/JSON errors must be passed through unchanged by the caller.
#[must_use]
pub fn python_exception(kind: &str, detail: &str) -> String {
    let detail = known(detail).unwrap_or_else(|| detail.to_owned());
    pair(
        "interaction-remote-python-exception",
        ("kind", kind),
        ("detail", &detail),
    )
}

#[must_use]
pub fn perl_exception(detail: &str, location: &str) -> String {
    let detail = diagnostic(detail);
    pair(
        "interaction-remote-perl-exception",
        ("detail", &detail),
        ("location", location),
    )
}

/// Render the owned UI-thread debug envelope from supplied values only. The caller
/// retains the original backtrace payload and never uses this for control decisions.
#[must_use]
pub fn ui_thread_debug(command: &str, detail: &str) -> String {
    pair(
        "interaction-remote-ui-thread-debug",
        ("command", command),
        ("detail", detail),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure display-only tests: no App/RepoTab, environment discovery, subprocesses,
    // sockets, filesystem fixtures, credential store, Git, timers or native helpers.
    #[test]
    fn raw_failure_and_pool_producers_stay_canonical_in_both_languages() {
        let host = super::super::Host::Ssh("me@build:2222".into());
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            assert_eq!(host.label(), "SSH · me@build:2222");
            let confirmed = super::super::shell::Failure::TimedOut(true).message();
            assert_eq!(
                confirmed,
                "it took too long; the owned supervisor confirmed stop and reap"
            );
            let unknown = super::super::shell::Failure::TimedOut(false).message();
            assert_eq!(
                unknown,
                "it took too long; cleanup is unknown and recovery evidence was retained"
            );
            let busy = super::super::pool::Lane::Tool.busy(&host);
            assert_eq!(
                busy,
                "4 diff or merge tools are already open on SSH · me@build:2222: close one of them first"
            );
            if locale == "de" {
                assert_eq!(
                    diagnostic(&confirmed),
                    "Der Vorgang dauerte zu lange; der eigene Supervisor hat das Beenden und Einlesen des Prozessstatus bestätigt."
                );
                assert_eq!(
                    diagnostic(&unknown),
                    "Der Vorgang dauerte zu lange; der Bereinigungsstatus ist unbekannt und die Wiederherstellungsnachweise bleiben erhalten."
                );
                assert_eq!(
                    diagnostic(&busy),
                    "Auf SSH · me@build:2222 sind bereits 4 Diff- oder Merge-Werkzeuge geöffnet: Schließen Sie zuerst eines davon."
                );
            } else {
                assert_eq!(diagnostic(&confirmed), confirmed);
                assert_eq!(diagnostic(&unknown), unknown);
                assert_eq!(diagnostic(&busy), busy);
            }
        }
    }

    #[test]
    fn notices_and_typed_numeric_values_use_actual_fluent_rendering() {
        crate::i18n::set_language("de");
        assert_eq!(
            diagnostic("No saved sign-ins to forget."),
            "Es gibt keine gespeicherten Anmeldungen zum Vergessen."
        );
        assert_eq!(
            diagnostic("Forgot 1 saved sign-in."),
            "Eine gespeicherte Anmeldung wurde vergessen."
        );
        assert_eq!(
            diagnostic("Forgot 2345 saved sign-ins."),
            "2.345 gespeicherte Anmeldungen wurden vergessen."
        );
        assert_eq!(
            diagnostic("workspace recovery exceeds 4096 owners"),
            "Die Wiederherstellung des Arbeitsbereichs überschreitet die Grenze von 4.096 Eigentümern."
        );
        assert_eq!(diagnostic("exit status 125"), "Exit-Status 125");
        for malformed in [
            "Forgot 01 saved sign-ins.",
            "Forgot -1 saved sign-ins.",
            "exit status 001",
            "exit status NaN",
        ] {
            assert_eq!(diagnostic(malformed), malformed);
        }
        crate::i18n::set_language("en-US");
        assert_eq!(
            diagnostic("Forgot 2345 saved sign-ins."),
            "Forgot 2,345 saved sign-ins."
        );
    }

    #[test]
    fn known_envelopes_keep_external_payloads_and_identities_opaque() {
        crate::i18n::set_language("de");
        let detail = "permission denied: /tmp/ä { $count } --force\nunknown device error";
        let raw = format!("could not reach SSH · me@build:2222: {detail}");
        assert_eq!(
            diagnostic(&raw),
            format!(
                "Die Verbindung zu SSH · me@build:2222 konnte nicht hergestellt werden: {detail}"
            )
        );
        assert_eq!(diagnostic(detail), detail);
        let external = "Traceback (most recent call last):\nValueError: invalid owner fields";
        assert_eq!(diagnostic(external), external);
        assert_eq!(
            diagnostic("user said invalid owner fields"),
            "user said invalid owner fields"
        );
        assert_eq!(
            diagnostic("gitcraft: no such folder: /home/ä/--force"),
            "gitcraft: Dieser Ordner ist nicht vorhanden: /home/ä/--force"
        );
        assert_eq!(
            diagnostic("gitcraft: invalid ownership handshake\n"),
            "gitcraft: Der Eigentümer-Handshake ist ungültig.\n"
        );
        assert_eq!(
            python_exception("ValueError", "invalid owner fields"),
            "ValueError: Die Felder des Eigentümers sind ungültig."
        );
        assert_eq!(
            python_exception("OSError", detail),
            format!("OSError: {detail}")
        );
        assert_eq!(
            perl_exception("invalid owner fields", "-e line 19."),
            "Die Felder des Eigentümers sind ungültig. at -e line 19."
        );
        let host = super::super::Host::Ssh("build".into());
        let command = "git -c x=' on y: z' status";
        let raw = format!("{command} on SSH · build: remote shell unavailable");
        assert_eq!(
            execution_error(&raw, command, &host),
            format!("{command} auf SSH · build: Die Remote-Shell ist nicht verfügbar.")
        );
        assert_eq!(execution_error(&raw, "wrong", &host), raw);
    }

    #[test]
    fn every_known_message_and_envelope_has_valid_paired_contracts_without_fallback()
    -> Result<(), String> {
        use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
        use std::collections::{BTreeMap, BTreeSet};
        let mut contracts = Vec::new();
        for (locale, source) in [
            (
                "en-US",
                include_str!("../../locales/en-US/interaction-remote.ftl"),
            ),
            (
                "de",
                include_str!("../../locales/de/interaction-remote.ftl"),
            ),
        ] {
            let resource =
                FluentResource::try_new(source.to_owned()).map_err(|(_, e)| format!("{e:?}"))?;
            let mut bundle =
                FluentBundle::new_concurrent(vec![locale.parse().map_err(|e| format!("{e}"))?]);
            bundle.set_use_isolating(false);
            bundle
                .add_resource(resource)
                .map_err(|e| format!("{e:?}"))?;
            let mut variables = BTreeMap::<&str, BTreeSet<&str>>::new();
            let mut current = "";
            for line in source.lines() {
                if line.starts_with("interaction-remote-") {
                    current = line.split_once('=').ok_or("missing equals")?.0.trim();
                    if variables.insert(current, BTreeSet::new()).is_some() {
                        return Err(format!("duplicate ID {current}"));
                    }
                }
                for after in line.split('$').skip(1) {
                    let end = after
                        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .unwrap_or(after.len());
                    variables.entry(current).or_default().insert(&after[..end]);
                }
            }
            for (id, names) in &variables {
                let mut args = FluentArgs::new();
                for name in names {
                    if ["count", "limit", "code"].contains(name) {
                        args.set(*name, 2345);
                    } else {
                        args.set(*name, "/opaque/ä --force { $count }");
                    }
                }
                let mut errors = Vec::new();
                let pattern = bundle
                    .get_message(id)
                    .and_then(|m| m.value())
                    .ok_or_else(|| format!("missing {id}"))?;
                let rendered = bundle.format_pattern(pattern, Some(&args), &mut errors);
                assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
                assert!(!rendered.is_empty());
            }
            for &(raw, id, limit) in KNOWN {
                let expected: BTreeSet<_> = if limit.is_some() {
                    BTreeSet::from(["limit"])
                } else {
                    BTreeSet::new()
                };
                assert_eq!(variables.get(id), Some(&expected), "{raw}");
            }
            for &(_, _, id, name) in ENVELOPES {
                assert_eq!(variables.get(id), Some(&BTreeSet::from([name])));
            }
            contracts.push(variables);
        }
        assert_eq!(contracts[0], contracts[1]);
        Ok(())
    }
}
