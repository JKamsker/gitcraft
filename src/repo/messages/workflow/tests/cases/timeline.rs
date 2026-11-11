use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        37,
        Case {
            raw: "Push: that would skip “opaque-skipped/Stage1/日本語/00001234”, which moved no branch: step through it with opaque-cmd/Stage1/日本語/00001234+Z / opaque-cmd/Stage1/日本語/00001234+Y instead",
            id: "repo-workflows-timeline-jump-b77dfff9",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                (
                    "skipped",
                    Sample::Text(
                        "opaque-skipped/Stage1/日本語/00001234",
                        "opaque-skipped/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "cmd",
                    Sample::Text(
                        "opaque-cmd/Stage1/日本語/00001234",
                        "opaque-cmd/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        42,
        Case {
            raw: "Finish or close the conflict or the rebase editor first to see the timeline",
            id: "repo-workflows-timeline-toggle-timeline-d12d6347",
            args: &[],
        },
    ),
    (
        58,
        Case {
            raw: "Push: the repository couldn't be read again; reload it (F5) first",
            id: "repo-workflows-timeline-timeline-unsettled-3fd5b04c",
            args: &[("what", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        73,
        Case {
            raw: "Pick a name no branch has for the new branch (not “opaque-name/Stage1/日本語/00001234”)",
            id: "repo-workflows-timeline-run-remove-6fd826a1",
            args: &[(
                "name",
                Sample::Text(
                    "opaque-name/Stage1/日本語/00001234",
                    "opaque-name/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        81,
        Case {
            raw: "Push: waiting for the repository to be read again…",
            id: "repo-workflows-timeline-timeline-unsettled-21705388",
            args: &[("what", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        172,
        Case {
            raw: "Go to before the timeline",
            id: "repo-workflows-timeline-jump-target-bd63e54c",
            args: &[],
        },
    ),
    (
        185,
        Case {
            raw: "Before the oldest entry",
            id: "repo-workflows-timeline-tree-rows-6fa556f4",
            args: &[],
        },
    ),
    (
        210,
        Case {
            raw: "Remove “opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234” from opaque-plan-branch-2/Stage1/日本語/00001234, as opaque-name/Stage1/日本語/00001234",
            id: "repo-workflows-timeline-run-remove-73ccc5e0",
            args: &[
                (
                    "self-entry-label-plan-id-1",
                    Sample::Text(
                        "opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234",
                        "opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "plan-branch-2",
                    Sample::Text(
                        "opaque-plan-branch-2/Stage1/日本語/00001234",
                        "opaque-plan-branch-2/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "name",
                    Sample::Text(
                        "opaque-name/Stage1/日本語/00001234",
                        "opaque-name/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        240,
        Case {
            raw: "Branch opaque-c-name-1/Stage1/日本語/00001234 deleted",
            id: "repo-workflows-timeline-fallback-label-b5bd817d",
            args: &[(
                "c-name-1",
                Sample::Text(
                    "opaque-c-name-1/Stage1/日本語/00001234",
                    "opaque-c-name-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        241,
        Case {
            raw: "Nothing to undo",
            id: "repo-workflows-timeline-undo-83290078",
            args: &[],
        },
    ),
    (
        242,
        Case {
            raw: "Nothing to redo",
            id: "repo-workflows-timeline-redo-4aaf2710",
            args: &[],
        },
    ),
    (
        259,
        Case {
            raw: "Put opaque-branch/Stage1/日本語/00001234 back at opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
            id: "repo-workflows-timeline-put-branch-back-94acd0c3",
            args: &[
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "crate-repo-short-commit-1",
                    Sample::Text(
                        "opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
                        "opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        267,
        Case {
            raw: "Merge commit",
            id: "repo-workflows-timeline-describe-reflog-8e9d44a6",
            args: &[],
        },
    ),
    (
        268,
        Case {
            raw: "Branch opaque-c-name-1/Stage1/日本語/00001234 made",
            id: "repo-workflows-timeline-fallback-label-1ac54e67",
            args: &[(
                "c-name-1",
                Sample::Text(
                    "opaque-c-name-1/Stage1/日本語/00001234",
                    "opaque-c-name-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        277,
        Case {
            raw: "Push, and 1234 more",
            id: "repo-workflows-timeline-outside-label-d1d10f56",
            args: &[
                ("first", Sample::Text("Push", "Pushen")),
                ("rest-len-1", Sample::Number(1234)),
            ],
        },
    ),
    (
        278,
        Case {
            raw: "Branch made",
            id: "repo-workflows-timeline-describe-reflog-56bb1bb2",
            args: &[],
        },
    ),
    (
        279,
        Case {
            raw: "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)",
            id: "repo-workflows-timeline-show-receipt-e172bf9c",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                (
                    "receipt-sentence-1",
                    Sample::Text(
                        "Final files identical, authors kept, messages kept",
                        "Dateien am Ende identisch, Autoren beibehalten, Nachrichten beibehalten",
                    ),
                ),
            ],
        },
    ),
    (
        280,
        Case {
            raw: "Going there",
            id: "repo-workflows-timeline-jump-a5337838",
            args: &[],
        },
    ),
    (
        296,
        Case {
            raw: "Reset to opaque-to/Stage1/日本語/00001234",
            id: "repo-workflows-timeline-describe-reflog-32fa2259",
            args: &[(
                "to",
                Sample::Text(
                    "opaque-to/Stage1/日本語/00001234",
                    "opaque-to/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        302,
        Case {
            raw: "Checkout",
            id: "repo-workflows-timeline-describe-reflog-99e71f48",
            args: &[],
        },
    ),
    (
        303,
        Case {
            raw: "Discard opaque-branch/Stage1/日本語/00001234",
            id: "repo-workflows-timeline-discard-remove-66f54467",
            args: &[(
                "branch",
                Sample::Text(
                    "opaque-branch/Stage1/日本語/00001234",
                    "opaque-branch/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        304,
        Case {
            raw: "Go to “opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234”",
            id: "repo-workflows-timeline-jump-target-1e4e36b2",
            args: &[(
                "self-timeline-get-id-label-1",
                Sample::Text(
                    "opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234",
                    "opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        315,
        Case {
            raw: "Merge opaque-w-merge-len-1/Stage1/日本語/00001234",
            id: "repo-workflows-timeline-describe-reflog-43695d49",
            args: &[(
                "w-merge-len-1",
                Sample::Text(
                    "opaque-w-merge-len-1/Stage1/日本語/00001234",
                    "opaque-w-merge-len-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        316,
        Case {
            raw: "opaque-c-name-1/Stage1/日本語/00001234 moved",
            id: "repo-workflows-timeline-fallback-label-e48d463d",
            args: &[(
                "c-name-1",
                Sample::Text(
                    "opaque-c-name-1/Stage1/日本語/00001234",
                    "opaque-c-name-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        320,
        Case {
            raw: "Reset",
            id: "repo-workflows-timeline-describe-reflog-daee7606",
            args: &[],
        },
    ),
    (
        321,
        Case {
            raw: "Moved",
            id: "repo-workflows-timeline-describe-reflog-b11c9047",
            args: &[],
        },
    ),
    (
        326,
        Case {
            raw: "Undo",
            id: "repo-workflows-timeline-undo-a8283ade",
            args: &[],
        },
    ),
    (
        327,
        Case {
            raw: "Redo",
            id: "repo-workflows-timeline-redo-74273989",
            args: &[],
        },
    ),
    (
        329,
        Case {
            raw: "Push (stopped partway)",
            id: "repo-workflows-timeline-timeline-op-ended-be0297e1",
            args: &[("label", Sample::Text("Push", "Pushen"))],
        },
    ),
];
