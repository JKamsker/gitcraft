//! Exact diagnostic bindings owned by ledger behavior.
pub(super) const MESSAGES: &[(&str, &str, Option<u32>)] = &[
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
        "workspace inventory bound; evidence retained",
        "interaction-remote-workspace-inventory-bound-evidence-retained-f139e7",
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
        "proof filename mismatch",
        "interaction-remote-proof-filename-mismatch-e0adae",
        None,
    ),
];
