pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        37,
        (
            "(?s)^(.*?): that would skip “(.*?)”, which moved no branch: step through it with (.*?)\\+Z / (.*?)\\+Y instead$",
            "repo-workflows-timeline-jump-b77dfff9",
            &[
                ("label", false),
                ("skipped", false),
                ("cmd", false),
                ("", false),
            ],
        ),
    ),
    (
        42,
        (
            "(?s)^Finish or close the conflict or the rebase editor first to see the timeline$",
            "repo-workflows-timeline-toggle-timeline-d12d6347",
            &[],
        ),
    ),
    (
        58,
        (
            "(?s)^(.*?): the repository couldn't be read again; reload it \\(F5\\) first$",
            "repo-workflows-timeline-timeline-unsettled-3fd5b04c",
            &[("what", false)],
        ),
    ),
    (
        73,
        (
            "(?s)^Pick a name no branch has for the new branch \\(not “(.*?)”\\)$",
            "repo-workflows-timeline-run-remove-6fd826a1",
            &[("name", false)],
        ),
    ),
    (
        81,
        (
            "(?s)^(.*?): waiting for the repository to be read again…$",
            "repo-workflows-timeline-timeline-unsettled-21705388",
            &[("what", false)],
        ),
    ),
    (
        172,
        (
            "(?s)^Go to before the timeline$",
            "repo-workflows-timeline-jump-target-bd63e54c",
            &[],
        ),
    ),
    (
        185,
        (
            "(?s)^Before the oldest entry$",
            "repo-workflows-timeline-tree-rows-6fa556f4",
            &[],
        ),
    ),
    (
        210,
        (
            "(?s)^Remove “(.*?)” from (.*?), as (.*?)$",
            "repo-workflows-timeline-run-remove-73ccc5e0",
            &[
                ("self-entry-label-plan-id-1", false),
                ("plan-branch-2", false),
                ("name", false),
            ],
        ),
    ),
    (
        240,
        (
            "(?s)^Branch (.*?) deleted$",
            "repo-workflows-timeline-fallback-label-b5bd817d",
            &[("c-name-1", false)],
        ),
    ),
    (
        241,
        (
            "(?s)^Nothing to undo$",
            "repo-workflows-timeline-undo-83290078",
            &[],
        ),
    ),
    (
        242,
        (
            "(?s)^Nothing to redo$",
            "repo-workflows-timeline-redo-4aaf2710",
            &[],
        ),
    ),
    (
        259,
        (
            "(?s)^Put (.*?) back at (.*?)$",
            "repo-workflows-timeline-put-branch-back-94acd0c3",
            &[("branch", false), ("crate-repo-short-commit-1", false)],
        ),
    ),
    (
        267,
        (
            "(?s)^Merge commit$",
            "repo-workflows-timeline-describe-reflog-8e9d44a6",
            &[],
        ),
    ),
    (
        268,
        (
            "(?s)^Branch (.*?) made$",
            "repo-workflows-timeline-fallback-label-1ac54e67",
            &[("c-name-1", false)],
        ),
    ),
    (
        277,
        (
            "(?s)^(.*?), and ([0-9]+) more$",
            "repo-workflows-timeline-outside-label-d1d10f56",
            &[("first", false), ("rest-len-1", true)],
        ),
    ),
    (
        278,
        (
            "(?s)^Branch made$",
            "repo-workflows-timeline-describe-reflog-56bb1bb2",
            &[],
        ),
    ),
    (
        279,
        (
            "(?s)^(.*?): done\\. (.*?) \\(receipt in the timeline, Ctrl\\+Shift\\+Z\\)$",
            "repo-workflows-timeline-show-receipt-e172bf9c",
            &[("label", false), ("receipt-sentence-1", false)],
        ),
    ),
    (
        280,
        (
            "(?s)^Going there$",
            "repo-workflows-timeline-jump-a5337838",
            &[],
        ),
    ),
    (
        296,
        (
            "(?s)^Reset to (.*?)$",
            "repo-workflows-timeline-describe-reflog-32fa2259",
            &[("to", false)],
        ),
    ),
    (
        302,
        (
            "(?s)^Checkout$",
            "repo-workflows-timeline-describe-reflog-99e71f48",
            &[],
        ),
    ),
    (
        303,
        (
            "(?s)^Discard (.*?)$",
            "repo-workflows-timeline-discard-remove-66f54467",
            &[("branch", false)],
        ),
    ),
    (
        304,
        (
            "(?s)^Go to “(.*?)”$",
            "repo-workflows-timeline-jump-target-1e4e36b2",
            &[("self-timeline-get-id-label-1", false)],
        ),
    ),
    (
        314,
        (
            "(?s)^Rebase$",
            "repo-workflows-timeline-describe-reflog-9eb5b275",
            &[],
        ),
    ),
    (
        315,
        (
            "(?s)^Merge (.*?)$",
            "repo-workflows-timeline-describe-reflog-43695d49",
            &[("w-merge-len-1", false)],
        ),
    ),
    (
        316,
        (
            "(?s)^(.*?) moved$",
            "repo-workflows-timeline-fallback-label-e48d463d",
            &[("c-name-1", false)],
        ),
    ),
    (
        320,
        (
            "(?s)^Reset$",
            "repo-workflows-timeline-describe-reflog-daee7606",
            &[],
        ),
    ),
    (
        321,
        (
            "(?s)^Moved$",
            "repo-workflows-timeline-describe-reflog-b11c9047",
            &[],
        ),
    ),
    (
        322,
        (
            "(?s)^Undo (.*?)$",
            "repo-workflows-timeline-undo-bf959af0",
            &[("entry-label-1", false)],
        ),
    ),
    (
        323,
        (
            "(?s)^Redo (.*?)$",
            "repo-workflows-timeline-redo-2b66f1cb",
            &[("entry-label-1", false)],
        ),
    ),
    (
        325,
        (
            "(?s)^Pull$",
            "repo-workflows-timeline-describe-reflog-b66d0441",
            &[],
        ),
    ),
    (
        326,
        ("(?s)^Undo$", "repo-workflows-timeline-undo-a8283ade", &[]),
    ),
    (
        327,
        ("(?s)^Redo$", "repo-workflows-timeline-redo-74273989", &[]),
    ),
    (
        329,
        (
            "(?s)^(.*?) \\(stopped partway\\)$",
            "repo-workflows-timeline-timeline-op-ended-be0297e1",
            &[("label", false)],
        ),
    ),
];
