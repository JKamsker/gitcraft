pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        53,
        (
            "(?s)^Distribute staged fixes: HEAD is detached, check out a branch first$",
            "repo-workflows-surgery-open-absorb-3373a6f7",
            &[],
        ),
    ),
    (
        54,
        (
            "(?s)^Distribute staged fixes: nothing is staged, stage the fixes first$",
            "repo-workflows-surgery-open-absorb-55d2e258",
            &[],
        ),
    ),
    (
        57,
        (
            "(?s)^Distribute staged fixes: close the editor that is open first$",
            "repo-workflows-surgery-open-absorb-2cddd12d",
            &[],
        ),
    ),
    (
        61,
        (
            "(?s)^Distribute staged fixes: another editor opened meanwhile$",
            "repo-workflows-surgery-open-absorb-55f28458",
            &[],
        ),
    ),
    (
        72,
        (
            "(?s)^Split commit: close the staged fixes editor first$",
            "repo-workflows-surgery-open-split-01ee6f3c",
            &[],
        ),
    ),
    (
        87,
        (
            "(?s)^Distribute staged fixes: a (.*?) is in progress$",
            "repo-workflows-surgery-open-absorb-edcc444b",
            &[("state-label-to-lowercase-1", false)],
        ),
    ),
    (
        94,
        (
            "(?s)^Move changes from (.*?) to the working changes$",
            "repo-workflows-surgery-run-move-afb8b0ad",
            &[("short-hash-1", false)],
        ),
    ),
    (
        123,
        (
            "(?s)^Distribute staged fixes: reading…$",
            "repo-workflows-surgery-open-absorb-5ebbefda",
            &[],
        ),
    ),
    (
        171,
        (
            "(?s)^Distribute staged fixes: (.*?)$",
            "repo-workflows-surgery-open-absorb-ed3b6cd2",
            &[("e", false)],
        ),
    ),
    (
        183,
        (
            "(?s)^Move changes: reading (.*?)…$",
            "repo-workflows-surgery-open-move-9a3c16a9",
            &[("short-hash-1", false)],
        ),
    ),
    (
        184,
        (
            "(?s)^Split commit: reading (.*?)…$",
            "repo-workflows-surgery-open-split-a472a361",
            &[("short-hash-1", false)],
        ),
    ),
    (
        196,
        (
            "(?s)^Move changes from (.*?) to (.*?)$",
            "repo-workflows-surgery-run-move-b5726a1d",
            &[("short-hash-1", false), ("short-t-hash-2", false)],
        ),
    ),
    (
        247,
        (
            "(?s)^Move changes: (.*?)$",
            "repo-workflows-surgery-open-move-9a352cd5",
            &[("why", false)],
        ),
    ),
    (
        248,
        (
            "(?s)^Split commit: (.*?)$",
            "repo-workflows-surgery-open-split-ad93af6b",
            &[("why", false)],
        ),
    ),
    (
        266,
        (
            "(?s)^Split (.*?) into ([0-9]+)$",
            "repo-workflows-surgery-run-split-ddd763c5",
            &[("short-hash-1", false), ("messages-len-2", true)],
        ),
    ),
];
