pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        2,
        (
            "(?s)^Use dates such as 2026\\-11\\-07$",
            "repo-workflows-history_activity-requested-range-4d18caf5",
            &[],
        ),
    ),
    (
        3,
        (
            "(?s)^The start date must be on or before the end date$",
            "repo-workflows-history_activity-requested-range-25f6acd0",
            &[],
        ),
    ),
    (
        4,
        (
            "(?s)^Analysis cancelled\\. Refresh to try again\\.$",
            "repo-workflows-history_activity-cancel-ddfe1c43",
            &[],
        ),
    ),
];
