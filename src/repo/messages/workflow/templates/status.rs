pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[(
    70,
    (
        "(?s)^(.*?): done, but its earlier version couldn't be kept: (.*?)$",
        "repo-workflows-status-rewrite-retention",
        &[("operation", false), ("detail", false)],
    ),
)];
