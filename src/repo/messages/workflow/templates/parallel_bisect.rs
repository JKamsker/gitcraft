pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        76,
        (
            "(?s)^A parallel bisect is already running in this tab$",
            "repo-workflows-parallel_bisect-start-parallel-3450e92d",
            &[],
        ),
    ),
    (
        92,
        (
            "(?s)^Wait for the operation going on to finish$",
            "repo-workflows-parallel_bisect-mark-parallel-result-e5e3ecc5",
            &[],
        ),
    ),
    (
        155,
        (
            "(?s)^Mark in the bisect failed: (.*?)$",
            "repo-workflows-parallel_bisect-mark-parallel-result-48f3588b",
            &[("e", false)],
        ),
    ),
    (
        207,
        (
            "(?s)^Marked in the bisect$",
            "repo-workflows-parallel_bisect-mark-parallel-result-e05d54d7",
            &[],
        ),
    ),
];
