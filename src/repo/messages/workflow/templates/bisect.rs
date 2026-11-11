pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        187,
        (
            "(?s)^Can't start a bisect: (.*?)$",
            "repo-workflows-bisect-start-bisect-5c82e3e0",
            &[("why", false)],
        ),
    ),
    (
        198,
        (
            "(?s)^No bisect is going on$",
            "repo-workflows-bisect-mark-bisect-a1c6c771",
            &[],
        ),
    ),
    (
        233,
        (
            "(?s)^Skip this commit$",
            "repo-workflows-bisect-mark-bisect-15306802",
            &[],
        ),
    ),
    (
        253,
        (
            "(?s)^Finish bisect$",
            "repo-workflows-bisect-reset-bisect-1bdcc155",
            &[],
        ),
    ),
    (
        264,
        (
            "(?s)^Start bisect$",
            "repo-workflows-bisect-start-bisect-db936451",
            &[],
        ),
    ),
    (
        274,
        (
            "(?s)^Stop bisect$",
            "repo-workflows-bisect-reset-bisect-0ddac35c",
            &[],
        ),
    ),
    (
        284,
        (
            "(?s)^Bisect run$",
            "repo-workflows-bisect-start-bisect-run-d300d73e",
            &[],
        ),
    ),
    (
        288,
        (
            "(?s)^Mark (.*?) as (.*?)$",
            "repo-workflows-bisect-mark-bisect-22f6b91b",
            &[("short-rev-1", false), ("bisect-word-mark-2", false)],
        ),
    ),
    (
        289,
        (
            "(?s)^(.*?) failed: (.*?)$",
            "repo-workflows-bisect-bisect-step-done-8336fbfe",
            &[("label", false), ("e", false)],
        ),
    ),
    (
        300,
        (
            "(?s)^Mark as (.*?)$",
            "repo-workflows-bisect-mark-bisect-9b0f3138",
            &[("bisect-word-mark-1", false)],
        ),
    ),
];
