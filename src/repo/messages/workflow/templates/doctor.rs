pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        18,
        (
            "(?s)^Clear the flag of (.*?)$",
            "repo-workflows-doctor-clear-7f76d2cf",
            &[("one-path-1", false)],
        ),
    ),
    (
        19,
        (
            "(?s)^Clear the flags of ([0-9]+) files$",
            "repo-workflows-doctor-clear-e318945b",
            &[("files-len-1", true)],
        ),
    ),
    (
        51,
        (
            "(?s)^Finish or close the conflict editor or the interactive rebase first$",
            "repo-workflows-doctor-open-doctor-745454bd",
            &[],
        ),
    ),
    (
        105,
        (
            "(?s)^Wait for the running operation to end$",
            "repo-workflows-doctor-doctor-act-9bab8d95",
            &[],
        ),
    ),
    (
        226,
        (
            "(?s)^Remove (.*?) \\((.*?)\\) from (.*?)$",
            "repo-workflows-doctor-remove-confirm-b0b26eef",
            &[
                ("origin-key-1", false),
                ("shortened-origin-shown-value-2", false),
                ("file", false),
            ],
        ),
    ),
    (
        255,
        (
            "(?s)^Set (.*?) to (.*?) for (.*?)$",
            "repo-workflows-doctor-set-row-921f564b",
            &[
                ("setting-key-1", false),
                ("shortened-value-2", false),
                ("scope-choice-scope-to-lowercase-3", false),
            ],
        ),
    ),
    (
        290,
        (
            "(?s)^Turn on: (.*?)$",
            "repo-workflows-doctor-turn-on-action-d8e3e02d",
            &[("s-title-to-lowercase-1", false)],
        ),
    ),
];
