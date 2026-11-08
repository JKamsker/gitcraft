//! Finite review-family paint data from the accepted 204-entry source inventory.
//! Stage provenance and storyboard commands remain owned by their existing adapters.
//! Observation command/output and compare captions have no origin discriminator here:
//! never infer their meaning from spelling. The broader workflow map is owner supplied.

pub(super) const FIXED: &[(&str, &str)] = &[
    (
        "Add 2–6 repositories to the release board",
        "repo-workflows-review-raw-add-2-6-repositories-to-the-release-board-4353ee",
    ),
    (
        "Component names must be unique ASCII identifiers, ignoring case",
        "repo-workflows-review-raw-component-names-must-be-unique-ascii-identifie-fde8d3",
    ),
    (
        "Use absolute repository paths or encoded SSH/WSL paths",
        "repo-workflows-review-raw-use-absolute-repository-paths-or-encoded-ssh-w-aef508",
    ),
    (
        "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
        "repo-workflows-review-raw-rehearse-linked-repositories-on-the-same-machi-c515cc",
    ),
    (
        "Too many combinations",
        "repo-workflows-review-raw-too-many-combinations-b95a07",
    ),
    (
        "More than 256 in-between combinations: split this release into smaller boards",
        "repo-workflows-review-raw-more-than-256-in-between-combinations-split-th-3b3334",
    ),
    (
        "A rule names an unknown dependent component",
        "repo-workflows-review-raw-a-rule-names-an-unknown-dependent-component-412c9a",
    ),
    (
        "A rule names an unknown prerequisite component",
        "repo-workflows-review-raw-a-rule-names-an-unknown-prerequisite-component-4ae079",
    ),
    (
        "Rule stage thresholds must name non-deployed stages present on their components",
        "repo-workflows-review-raw-rule-stage-thresholds-must-name-non-deployed-s-aff755",
    ),
    (
        "Stopped before constructing the full combination",
        "repo-workflows-review-raw-stopped-before-constructing-the-full-combinati-a61d6f",
    ),
    (
        "Enter and explicitly trust your compatibility command before rehearsal",
        "repo-workflows-review-raw-enter-and-explicitly-trust-your-compatibility-691732",
    ),
    (
        "Not tested: rehearsal stopped",
        "repo-workflows-review-raw-not-tested-rehearsal-stopped-212f11",
    ),
    (
        "Changes moved since this page was loaded; reload before applying a hunk",
        "repo-workflows-review-raw-changes-moved-since-this-page-was-loaded-reloa-489eff",
    ),
    (
        "Review hunk",
        "repo-workflows-review-raw-review-hunk-dbb59a",
    ),
    (
        "Hunk submitted. Reload after the operation finishes.",
        "repo-workflows-review-raw-hunk-submitted-reload-after-the-operation-fini-ce9941",
    ),
    (
        "operation could not start",
        "repo-workflows-review-raw-operation-could-not-start-cf7a8f",
    ),
    (
        "Pull request is refreshing; reload after its current changes arrive",
        "repo-workflows-review-raw-pull-request-is-refreshing-reload-after-its-cu-51a0bf",
    ),
    (
        "Current pull request details and changes must finish loading before review",
        "repo-workflows-review-raw-current-pull-request-details-and-changes-must-1842eb",
    ),
    (
        "Working tree and index · current membership",
        "repo-workflows-review-raw-working-tree-and-index-current-membership-3e249d",
    ),
    (
        "working tree",
        "repo-workflows-review-raw-working-tree-29c60d",
    ),
    (
        "PR snapshot missing",
        "repo-workflows-review-raw-pr-snapshot-missing-879d8b",
    ),
    (
        "This document has more than 500 files; select a narrower comparison",
        "repo-workflows-review-raw-this-document-has-more-than-500-files-select-a-d334c6",
    ),
    (
        "Review loading cancelled",
        "repo-workflows-review-raw-review-loading-cancelled-41a476",
    ),
    (
        "Review exceeds 250,000 lines or 32 MiB; select a narrower comparison. No partial document was accepted.",
        "repo-workflows-review-raw-review-exceeds-250-000-lines-or-32-mib-select-df273f",
    ),
    (
        "Choose between 1 and 32 target revisions, one per line",
        "repo-workflows-review-raw-choose-between-1-and-32-target-revisions-one-p-00ba3c",
    ),
    (
        "Explicitly trust and enter the regression command first",
        "repo-workflows-review-raw-explicitly-trust-and-enter-the-regression-comm-c6ac97",
    ),
    (
        "Cancelled before patch application; behavior not tested",
        "repo-workflows-review-raw-cancelled-before-patch-application-behavior-no-b7230f",
    ),
    (
        "No personal data directory",
        "repo-workflows-review-raw-no-personal-data-directory-e96ca6",
    ),
    (
        "Rulebook writer unavailable",
        "repo-workflows-review-raw-rulebook-writer-unavailable-e833c4",
    ),
    (
        "No rulebook folder",
        "repo-workflows-review-raw-no-rulebook-folder-ae3e7b",
    ),
    (
        "The forge did not identify your signed-in account",
        "repo-workflows-review-raw-the-forge-did-not-identify-your-signed-in-acco-7805d7",
    ),
    (
        "Rule has no imported comment citation",
        "repo-workflows-review-raw-rule-has-no-imported-comment-citation-db59f1",
    ),
    (
        "Rule pattern is empty or too large",
        "repo-workflows-review-raw-rule-pattern-is-empty-or-too-large-24995a",
    ),
    (
        "Open a pull request to import your published review comments",
        "repo-workflows-review-raw-open-a-pull-request-to-import-your-published-r-81ce7f",
    ),
    (
        "Opt in before sending comments to your selected AI tool",
        "repo-workflows-review-raw-opt-in-before-sending-comments-to-your-selecte-7ec8bb",
    ),
    (
        "Wait for the current operation or reviewer echo before sending",
        "repo-workflows-review-raw-wait-for-the-current-operation-or-reviewer-ech-8b06c3",
    ),
    (
        "The prepared profile or identity policy changed; reopen push preflight",
        "repo-workflows-review-raw-the-prepared-profile-or-identity-policy-change-f71b3f",
    ),
    (
        "Reviewed send profile, sign-in or repository changed; prepare again",
        "repo-workflows-review-raw-reviewed-send-profile-sign-in-or-repository-ch-75d4b5",
    ),
    (
        "Send reviewed exact commit",
        "repo-workflows-review-raw-send-reviewed-exact-commit-ba6403",
    ),
    (
        "Review or cancel the pending reviewer-echo operation first",
        "repo-workflows-review-raw-review-or-cancel-the-pending-reviewer-echo-ope-299552",
    ),
    (
        "Push source or remote configuration changed during reviewer echo; start the push again",
        "repo-workflows-review-raw-push-source-or-remote-configuration-changed-du-c1b07a",
    ),
    (
        "Reviewer echo inputs or rulebook changed before execution; start the push again",
        "repo-workflows-review-raw-reviewer-echo-inputs-or-rulebook-changed-befor-7d6bd0",
    ),
    (
        "Push source or remote configuration changed after reviewer echo; start the push again",
        "repo-workflows-review-raw-push-source-or-remote-configuration-changed-af-fa269f",
    ),
    (
        "Reviewed push already has execution evidence; resume its retained operation",
        "repo-workflows-review-raw-reviewed-push-already-has-execution-evidence-r-85ecb3",
    ),
    (
        "Push destination is not unambiguous; review its source manually",
        "repo-workflows-review-raw-push-destination-is-not-unambiguous-review-its-64ee34",
    ),
    (
        "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
        "repo-workflows-review-raw-this-operation-changes-state-before-pushing-ru-716a82",
    ),
    (
        "Invalid wildcard source",
        "repo-workflows-review-raw-invalid-wildcard-source-9e223c",
    ),
    (
        "Unsupported push wildcard; review it explicitly",
        "repo-workflows-review-raw-unsupported-push-wildcard-review-it-explicitly-71983a",
    ),
    (
        "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
        "repo-workflows-review-raw-reviewer-echo-cannot-inspect-context-changing-928514",
    ),
    (
        "More than eight receipt revisions: further Rust dependency snapshots remain unchecked",
        "repo-workflows-review-raw-more-than-eight-receipt-revisions-further-rust-99db72",
    ),
    (
        "Configure your Git name and email before posting an intent",
        "repo-workflows-review-raw-configure-your-git-name-and-email-before-posti-7e823d",
    ),
    (
        "Give a relative code path and a short intent",
        "repo-workflows-review-raw-give-a-relative-code-path-and-a-short-intent-62aade",
    ),
    (
        "Invalid intent cache identity",
        "repo-workflows-review-raw-invalid-intent-cache-identity-6e8f31",
    ),
    (
        "Intent is outside the selected source",
        "repo-workflows-review-raw-intent-is-outside-the-selected-source-25b6e9",
    ),
    (
        "Invalid intent ref identity",
        "repo-workflows-review-raw-invalid-intent-ref-identity-27cd6c",
    ),
    (
        "Intent identity does not match its source ref",
        "repo-workflows-review-raw-intent-identity-does-not-match-its-source-ref-9f78a2",
    ),
    (
        "Intent fields exceed the short-note bounds or contain an unsafe path/control character",
        "repo-workflows-review-raw-intent-fields-exceed-the-short-note-bounds-or-f74001",
    ),
    (
        "Intent expiry must be within 24 hours of its creation",
        "repo-workflows-review-raw-intent-expiry-must-be-within-24-hours-of-its-c-0186cb",
    ),
    (
        "More than 5000 refs in the selected intent source; archive old notes before loading",
        "repo-workflows-review-raw-more-than-5000-refs-in-the-selected-intent-sou-f76132",
    ),
    (
        "Selected intent metadata exceeds the 4MiB import budget; read refused because more notes remain",
        "repo-workflows-review-raw-selected-intent-metadata-exceeds-the-4mib-impo-d6c281",
    ),
    (
        "Intent exceeds the metadata import budget",
        "repo-workflows-review-raw-intent-exceeds-the-metadata-import-budget-14f171",
    ),
    (
        "Malformed intent ref listing",
        "repo-workflows-review-raw-malformed-intent-ref-listing-35d962",
    ),
    (
        "Invalid intent blob size",
        "repo-workflows-review-raw-invalid-intent-blob-size-8faf38",
    ),
    (
        "Pick a configured remote",
        "repo-workflows-review-raw-pick-a-configured-remote-c8883c",
    ),
    (
        "You can only end your own local intents",
        "repo-workflows-review-raw-you-can-only-end-your-own-local-intents-5c8aa8",
    ),
    (
        "Intent no longer present",
        "repo-workflows-review-raw-intent-no-longer-present-718025",
    ),
    (
        "No captured sharing destination",
        "repo-workflows-review-raw-no-captured-sharing-destination-23cc03",
    ),
    (
        "No local intents to publish",
        "repo-workflows-review-raw-no-local-intents-to-publish-7ff0ec",
    ),
    (
        "Pin a nonempty, nonmerge fix patch",
        "repo-workflows-review-raw-pin-a-nonempty-nonmerge-fix-patch-d15c07",
    ),
    (
        "Give the pinned fix a regression command",
        "repo-workflows-review-raw-give-the-pinned-fix-a-regression-command-5ac9b9",
    ),
    (
        "Choose 1–32 release targets",
        "repo-workflows-review-raw-choose-1-32-release-targets-9c3f2b",
    ),
    (
        "Reload the pinned fixes",
        "repo-workflows-review-raw-reload-the-pinned-fixes-1f6af7",
    ),
    (
        "Explicitly trust the regression command before running it",
        "repo-workflows-review-raw-explicitly-trust-the-regression-command-before-576778",
    ),
    (
        "Reload pinned fixes",
        "repo-workflows-review-raw-reload-pinned-fixes-30976a",
    ),
    (
        "Generated result belongs to the earlier form. Your edits were preserved; inspect and explicitly use the result below if wanted.",
        "repo-workflows-review-raw-generated-result-belongs-to-the-earlier-form-y-f66314",
    ),
    (
        "Inspect the source and explicitly approve sandbox execution first",
        "repo-workflows-review-raw-inspect-the-source-and-explicitly-approve-sand-43f89f",
    ),
    (
        "The generated integration test must be 1–65536 bytes",
        "repo-workflows-review-raw-the-generated-integration-test-must-be-1-65536-05778c",
    ),
    (
        "Cleanup needs attention",
        "repo-workflows-review-raw-cleanup-needs-attention-af510d",
    ),
    (
        "Stop requested. Bounded cleanup and partial observations are preserved; load preserved observations after the worker finishes.",
        "repo-workflows-review-raw-stop-requested-bounded-cleanup-and-partial-obs-9dd93e",
    ),
    (
        "Select a smaller file diff before generating a reproducer",
        "repo-workflows-review-raw-select-a-smaller-file-diff-before-generating-a-95611a",
    ),
    (
        "The model produced more than 64KiB; narrow the objection",
        "repo-workflows-review-raw-the-model-produced-more-than-64kib-narrow-the-0838f1",
    ),
    (
        "Invalid immutable image/owner identity",
        "repo-workflows-review-raw-invalid-immutable-image-owner-identity-b4e384",
    ),
    (
        "Container ownership inspection was interrupted",
        "repo-workflows-review-raw-container-ownership-inspection-was-interrupted-2d8be0",
    ),
    (
        "Invalid container ownership response",
        "repo-workflows-review-raw-invalid-container-ownership-response-856d57",
    ),
    (
        "Invalid immutable container ownership response",
        "repo-workflows-review-raw-invalid-immutable-container-ownership-response-401976",
    ),
    (
        "Name belongs to another owner; cleanup refused",
        "repo-workflows-review-raw-name-belongs-to-another-owner-cleanup-refused-ab7729",
    ),
    (
        "Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.",
        "repo-workflows-review-raw-creation-outcome-is-unknown-no-owned-id-was-fo-c58831",
    ),
    (
        "No immutable container ID",
        "repo-workflows-review-raw-no-immutable-container-id-d21bb8",
    ),
    (
        "Invalid cleanup ID",
        "repo-workflows-review-raw-invalid-cleanup-id-118731",
    ),
    (
        "Container was already absent",
        "repo-workflows-review-raw-container-was-already-absent-b5a8e5",
    ),
    (
        "Immutable container owner/ID mismatch; cleanup refused",
        "repo-workflows-review-raw-immutable-container-owner-id-mismatch-cleanup-4998b4",
    ),
    (
        "Container cleanup",
        "repo-workflows-review-raw-container-cleanup-f7f59d",
    ),
    (
        "Owned container removed by immutable ID",
        "repo-workflows-review-raw-owned-container-removed-by-immutable-id-b62d78",
    ),
    (
        "Choose a preloaded Rust image by name or digest",
        "repo-workflows-review-raw-choose-a-preloaded-rust-image-by-name-or-diges-3b0d5b",
    ),
    (
        "Inspect preloaded image",
        "repo-workflows-review-raw-inspect-preloaded-image-0e97be",
    ),
    (
        "Invalid image ID",
        "repo-workflows-review-raw-invalid-image-id-5a0b85",
    ),
    (
        "Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled",
        "repo-workflows-review-raw-selected-image-is-unavailable-locally-or-did-n-8e6218",
    ),
    (
        "Create sandbox",
        "repo-workflows-review-raw-create-sandbox-6785cd",
    ),
    (
        "Invalid container creation response",
        "repo-workflows-review-raw-invalid-container-creation-response-ade99f",
    ),
    (
        "Docker did not return a complete immutable container ID",
        "repo-workflows-review-raw-docker-did-not-return-a-complete-immutable-con-41ab72",
    ),
    (
        "Created container ownership could not be established; execution refused",
        "repo-workflows-review-raw-created-container-ownership-could-not-be-estab-c0c4ba",
    ),
    (
        "Read sandbox exit status",
        "repo-workflows-review-raw-read-sandbox-exit-status-9152bf",
    ),
    (
        "Invalid sandbox exit status",
        "repo-workflows-review-raw-invalid-sandbox-exit-status-cc6e1c",
    ),
    (
        "Sandbox exit status is outside Docker's range",
        "repo-workflows-review-raw-sandbox-exit-status-is-outside-docker-s-range-867171",
    ),
    (
        "Source export cancelled",
        "repo-workflows-review-raw-source-export-cancelled-49ca7d",
    ),
    (
        "Raw source export exceeded its 60s preparation budget",
        "repo-workflows-review-raw-raw-source-export-exceeded-its-60s-preparation-4e4dc2",
    ),
    (
        "Sandbox source exceeds 20,000 files or 128MiB",
        "repo-workflows-review-raw-sandbox-source-exceeds-20-000-files-or-128mib-9f7142",
    ),
    (
        "Submodule nesting exceeds the eight-level source-export limit",
        "repo-workflows-review-raw-submodule-nesting-exceeds-the-eight-level-sour-148cdc",
    ),
    (
        "Malformed raw tree record",
        "repo-workflows-review-raw-malformed-raw-tree-record-6ab124",
    ),
    (
        "Invalid raw tree metadata",
        "repo-workflows-review-raw-invalid-raw-tree-metadata-6961a8",
    ),
    (
        "Malformed raw tree metadata",
        "repo-workflows-review-raw-malformed-raw-tree-metadata-9cd416",
    ),
    (
        "Raw tree contains an invalid object ID",
        "repo-workflows-review-raw-raw-tree-contains-an-invalid-object-id-b7eee8",
    ),
    (
        "Sandbox source paths must be UTF-8",
        "repo-workflows-review-raw-sandbox-source-paths-must-be-utf-8-122183",
    ),
    (
        "Unsupported raw tree object mode",
        "repo-workflows-review-raw-unsupported-raw-tree-object-mode-28f35d",
    ),
    (
        "Invalid raw blob size",
        "repo-workflows-review-raw-invalid-raw-blob-size-6ea4b1",
    ),
    (
        "Sandbox source exceeds 16MiB per blob or 128MiB total",
        "repo-workflows-review-raw-sandbox-source-exceeds-16mib-per-blob-or-128mi-fe4de2",
    ),
    (
        "Raw blob size changed or output was truncated",
        "repo-workflows-review-raw-raw-blob-size-changed-or-output-was-truncated-a205c5",
    ),
    (
        "Duplicate raw source path",
        "repo-workflows-review-raw-duplicate-raw-source-path-c61f88",
    ),
    (
        "Sandbox symlink targets must be UTF-8",
        "repo-workflows-review-raw-sandbox-symlink-targets-must-be-utf-8-82700d",
    ),
    (
        "Symlink points into Git metadata",
        "repo-workflows-review-raw-symlink-points-into-git-metadata-d47822",
    ),
    (
        "Archive field overflow",
        "repo-workflows-review-raw-archive-field-overflow-a70fa1",
    ),
    (
        "Archive header requires a PAX path record",
        "repo-workflows-review-raw-archive-header-requires-a-pax-path-record-c9450c",
    ),
    (
        "Non-UTF-8 or newline-containing Rust paths were not analyzed",
        "repo-workflows-review-raw-non-utf-8-or-newline-containing-rust-paths-wer-9e8a8d",
    ),
    (
        "More than 4096 Rust files; dependency analysis was not performed",
        "repo-workflows-review-raw-more-than-4096-rust-files-dependency-analysis-8964c0",
    ),
    (
        "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
        "repo-workflows-review-raw-rust-sources-exceed-the-32mib-analysis-budget-2f47c3",
    ),
    (
        "Incomplete Rust blob header",
        "repo-workflows-review-raw-incomplete-rust-blob-header-a8a0a0",
    ),
    (
        "Invalid Rust blob size",
        "repo-workflows-review-raw-invalid-rust-blob-size-7071a5",
    ),
    (
        "A Rust source is not a blob",
        "repo-workflows-review-raw-a-rust-source-is-not-a-blob-5f8d45",
    ),
    (
        "Incomplete Rust blob",
        "repo-workflows-review-raw-incomplete-rust-blob-7a9492",
    ),
    (
        "Incomplete Rust blob separator",
        "repo-workflows-review-raw-incomplete-rust-blob-separator-e86a4e",
    ),
    (
        "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
        "repo-workflows-review-raw-rust-syntax-evidence-does-not-resolve-cfg-feat-2c702b",
    ),
    (
        "Static call traversal exceeded 1024 symbols",
        "repo-workflows-review-raw-static-call-traversal-exceeded-1024-symbols-64479e",
    ),
    (
        "Dynamic, external, local-import, or macro-generated calls remain unchecked",
        "repo-workflows-review-raw-dynamic-external-local-import-or-macro-generat-5d8937",
    ),
    (
        "Duplicate/cfg-dependent symbol identities were not resolved",
        "repo-workflows-review-raw-duplicate-cfg-dependent-symbol-identities-were-6207ad",
    ),
    (
        "No unchanged uniquely identified Rust functions were analyzed",
        "repo-workflows-review-raw-no-unchanged-uniquely-identified-rust-function-bd0023",
    ),
    (
        "Review-artifact sharing requires exactly one fetch and one push URL",
        "repo-workflows-review-raw-review-artifact-sharing-requires-exactly-one-f-9de1d2",
    ),
    (
        "Sharing destination changed. A completed transfer may already have affected the captured destination; refresh before retrying",
        "repo-workflows-review-raw-sharing-destination-changed-a-completed-transf-c43578",
    ),
    (
        "Sharing remote changed; refresh repository remotes and try again",
        "repo-workflows-review-raw-sharing-remote-changed-refresh-repository-remo-c9eae9",
    ),
    (
        "Artifact cache update crossed its captured namespace",
        "repo-workflows-review-raw-artifact-cache-update-crossed-its-captured-nam-3749bb",
    ),
    (
        "Give the card its explanation",
        "repo-workflows-review-raw-give-the-card-its-explanation-5582b3",
    ),
    (
        "No test-lane result for this tree; run Test commits first",
        "repo-workflows-review-raw-no-test-lane-result-for-this-tree-run-test-com-5252d5",
    ),
    (
        "No test command recorded",
        "repo-workflows-review-raw-no-test-command-recorded-ef9455",
    ),
    (
        "Choose a repository-relative path",
        "repo-workflows-review-raw-choose-a-repository-relative-path-caa4f6",
    ),
    (
        "Line must be a positive number",
        "repo-workflows-review-raw-line-must-be-a-positive-number-53ab24",
    ),
    (
        "Line is outside this revision's file",
        "repo-workflows-review-raw-line-is-outside-this-revision-s-file-217c8d",
    ),
    (
        "Current tree differs from the pinned test result",
        "repo-workflows-review-raw-current-tree-differs-from-the-pinned-test-resu-be0479",
    ),
    (
        "The pinned commit has no replacement in this series",
        "repo-workflows-review-raw-the-pinned-commit-has-no-replacement-in-this-s-995683",
    ),
    (
        "The card's file changed; open the pinned original to review it",
        "repo-workflows-review-raw-the-card-s-file-changed-open-the-pinned-origin-1629a8",
    ),
    (
        "The card's file was removed or is unavailable",
        "repo-workflows-review-raw-the-card-s-file-was-removed-or-is-unavailable-498722",
    ),
    (
        "A shared storyboard supports at most 256 cards",
        "repo-workflows-review-raw-a-shared-storyboard-supports-at-most-256-cards-10e700",
    ),
    (
        "Shared storyboard exceeds the 2MiB metadata budget",
        "repo-workflows-review-raw-shared-storyboard-exceeds-the-2mib-metadata-bu-2f3b7b",
    ),
    (
        "Shared storyboard has more than 256 cards",
        "repo-workflows-review-raw-shared-storyboard-has-more-than-256-cards-ecd3f9",
    ),
    (
        "Shared storyboard revision is not an immutable commit ID",
        "repo-workflows-review-raw-shared-storyboard-revision-is-not-an-immutable-f33df1",
    ),
    (
        "Shared storyboard blob does not match its pinned source",
        "repo-workflows-review-raw-shared-storyboard-blob-does-not-match-its-pinn-48731f",
    ),
    (
        "Shared storyboard changed while preparing publication; fetch and retry",
        "repo-workflows-review-raw-shared-storyboard-changed-while-preparing-publ-20a0d1",
    ),
    (
        "Shared storyboard was not fetched",
        "repo-workflows-review-raw-shared-storyboard-was-not-fetched-c878b4",
    ),
    (
        "Choose a configured storyboard sharing remote",
        "repo-workflows-review-raw-choose-a-configured-storyboard-sharing-remote-183f4f",
    ),
    (
        "Pull request changed; refresh the storyboard",
        "repo-workflows-review-raw-pull-request-changed-refresh-the-storyboard-0b726d",
    ),
    (
        "Only the pull request author publishes its storyboard",
        "repo-workflows-review-raw-only-the-pull-request-author-publishes-its-sto-1ca8a4",
    ),
    (
        "Pull request files are refreshing; wait before arranging its storyboard",
        "repo-workflows-review-raw-pull-request-files-are-refreshing-wait-before-c0a60d",
    ),
    (
        "Load your local storyboard before editing",
        "repo-workflows-review-raw-load-your-local-storyboard-before-editing-0ae7c5",
    ),
    (
        "Only the pull request author arranges this storyboard",
        "repo-workflows-review-raw-only-the-pull-request-author-arranges-this-sto-cc573f",
    ),
    (
        "The storyboard selection is stale; reload it",
        "repo-workflows-review-raw-the-storyboard-selection-is-stale-reload-it-293295",
    ),
];

pub(super) fn fixed(raw: &str) -> Option<String> {
    FIXED
        .iter()
        .find(|(source, _)| *source == raw)
        .map(|(_, id)| crate::i18n::text(id).to_owned())
}

// Only these complete, application-owned envelopes enter the canonical regex map.
pub(super) const TEMPLATES: &[super::TemplateSpec] = &[
    (
        "(?s)^Choose the repository for (.*?)$",
        "repo-workflows-review-raw-choose-the-repository-for-3ff8de",
        &[("component", false)],
    ),
    (
        "(?s)^(.*?) needs 2–4 ordered stages: deployed, optional bridge, next$",
        "repo-workflows-review-raw-needs-2-4-ordered-stages-deployed-optional-bri-560f7e",
        &[("component", false)],
    ),
    (
        "(?s)^Hunk refused: (.*?)$",
        "repo-workflows-review-raw-hunk-refused-error-dbed1f",
        &[("detail", false)],
    ),
    (
        "(?s)^Commit (.*?)$",
        "repo-workflows-review-raw-commit-53bf7c",
        &[("commit", false)],
    ),
    (
        "(?s)^Comparison (.*?) → (.*?)$",
        "repo-workflows-review-raw-comparison-from-987fec",
        &[("from", false), ("to", false)],
    ),
    (
        "(?s)^Pull request \\#(.*?) · (.*?) → (.*?)$",
        "repo-workflows-review-raw-pull-request-number-from-to-c364ad",
        &[("number", false), ("from", false), ("to", false)],
    ),
    (
        "(?s)^Invalid rulebook from model: (.*?)$",
        "repo-workflows-review-raw-invalid-rulebook-from-model-e-133045",
        &[("detail", false)],
    ),
    (
        "(?s)^Reviewer echo cannot infer push option (.*?); review this operation explicitly$",
        "repo-workflows-review-raw-reviewer-echo-cannot-infer-push-option-word-re-fc6421",
        &[("option", false)],
    ),
    (
        "(?s)^Push mode (.*?) has no inferable source; review it explicitly$",
        "repo-workflows-review-raw-push-mode-mode-has-no-inferable-source-review-93d0aa",
        &[("mode", false)],
    ),
    (
        "(?s)^Push source (.*?) has no inspectable commit tree$",
        "repo-workflows-review-raw-push-source-src-has-no-inspectable-commit-tree-92780c",
        &[("source", false)],
    ),
    (
        "(?s)^Review receipt: (.*?)$",
        "repo-workflows-review-raw-review-receipt-e-4a5210",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid intent JSON: (.*?)$",
        "repo-workflows-review-raw-invalid-intent-json-e-96d3b1",
        &[("detail", false)],
    ),
    (
        "(?s)^Intent blob exceeds the ([0-9]+)\\-byte budget$",
        "repo-workflows-review-raw-intent-blob-exceeds-the-budget-byte-budget-259d7d",
        &[("budget", true)],
    ),
    (
        "(?s)^Latest independent cleanup (.*?) \\((.*?)\\) · (.*?)$",
        "repo-workflows-review-raw-latest-independent-cleanup-c07c47",
        &[("name", false), ("id", false), ("detail", false)],
    ),
    (
        "(?s)^Container ownership inspection failed: (.*?)$",
        "repo-workflows-review-raw-container-ownership-inspection-failed-b2504e",
        &[("detail", false)],
    ),
    (
        "(?s)^Cleanup needs attention: (.*?)$",
        "repo-workflows-review-raw-cleanup-needs-attention-e-9cf993",
        &[("detail", false)],
    ),
    (
        "(?s)^Could not preserve reproducer cleanup evidence for (.*?): (.*?); (.*?)$",
        "repo-workflows-review-raw-could-not-preserve-reproducer-cleanup-evidence-c910e4",
        &[("name", false), ("detail", false), ("result", false)],
    ),
    (
        "(?s)^Sandbox did not complete: (.*?)$",
        "repo-workflows-review-raw-sandbox-did-not-complete-error-95c251",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid preserved reproducer evidence: (.*?)$",
        "repo-workflows-review-raw-invalid-preserved-reproducer-evidence-e-4bd6de",
        &[("detail", false)],
    ),
    (
        "(?s)^Submodule (.*?) at (.*?): (.*?)\\. Its pinned objects must already be present locally; gitcraft never clones or checks it out\\.$",
        "repo-workflows-review-raw-submodule-path-at-oid-e-its-pinned-objects-mus-6a547c",
        &[("path", false), ("oid", false), ("detail", false)],
    ),
    (
        "(?s)^Unsafe or unsupported sandbox source path: (.*?)$",
        "repo-workflows-review-raw-unsafe-or-unsupported-sandbox-source-path-path-6890f5",
        &[("path", false)],
    ),
    (
        "(?s)^Unsafe symlink (.*?)$",
        "repo-workflows-review-raw-unsafe-symlink-path-f052a0",
        &[("path", false)],
    ),
    (
        "(?s)^Symlink (.*?) escapes the source root$",
        "repo-workflows-review-raw-symlink-path-escapes-the-source-root-fb2203",
        &[("path", false)],
    ),
    (
        "(?s)^Archive member (.*?) traverses a symlink$",
        "repo-workflows-review-raw-archive-member-path-traverses-a-symlink-d96181",
        &[("path", false)],
    ),
    (
        "(?s)^Invalid review evidence: (.*?)$",
        "repo-workflows-review-raw-invalid-review-evidence-e-d265ce",
        &[("detail", false)],
    ),
    (
        "(?s)^Invalid shared storyboard: (.*?)$",
        "repo-workflows-review-raw-invalid-shared-storyboard-e-94d7b6",
        &[("detail", false)],
    ),
    (
        "(?s)^([0-9]+) additional invalid records omitted$",
        "repo-workflows-review-raw-additional-invalid-records-omitted-f1c696",
        &[("count", true)],
    ),
];

/// Source-owned Rust dependency witnesses/limitations; never a generic diagnostic parser.
/// The only caller paints the report produced by rust_dependencies. Each value is opaque.
pub(super) fn dependency(raw: &str) -> String {
    if let Some((path, detail)) = raw
        .strip_suffix(')')
        .and_then(|s| s.split_once(": Rust syntax not parsed ("))
    {
        return crate::i18n::message(
            "repo-workflows-review-raw-path-rust-syntax-not-parsed-e-110a0e",
            &[("path", path.into()), ("detail", detail.into())],
        );
    }
    if let Some(chain) = raw.strip_suffix(": static call targets changed") {
        return crate::i18n::message(
            "repo-workflows-review-raw-static-call-targets-changed-8fc13b",
            &[("chain", chain.into())],
        );
    }
    if let Some((chain, dependency)) = raw
        .strip_suffix(" changed")
        .and_then(|s| s.rsplit_once(" → "))
    {
        return crate::i18n::message(
            "repo-workflows-review-raw-changed-41e60a",
            &[("chain", chain.into()), ("dependency", dependency.into())],
        );
    }
    fixed(raw).unwrap_or_else(|| raw.to_owned())
}

/// The quarantine vector is the producer's reference/error envelope (or omitted count).
/// Its inner diagnostics use only this review family's finite envelopes. Do not apply
/// operation recognition to refs, imported JSON, provider diagnostics or arbitrary commands.
pub(super) fn intent_warning(raw: &str) -> String {
    if let Some((reference, detail)) = raw.split_once(": ") {
        let detail = super::review_owned(detail);
        return crate::i18n::message(
            "repo-workflows-review-raw-reference-error-8c337e",
            &[("reference", reference.into()), ("detail", detail.into())],
        );
    }
    super::review_owned(raw)
}

#[cfg(test)]
mod tests;
