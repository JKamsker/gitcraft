use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        2,
        Case {
            raw: "Use dates such as 2026-11-07",
            id: "repo-workflows-history_activity-requested-range-4d18caf5",
            args: &[],
        },
    ),
    (
        3,
        Case {
            raw: "The start date must be on or before the end date",
            id: "repo-workflows-history_activity-requested-range-25f6acd0",
            args: &[],
        },
    ),
    (
        4,
        Case {
            raw: "Analysis cancelled. Refresh to try again.",
            id: "repo-workflows-history_activity-cancel-ddfe1c43",
            args: &[],
        },
    ),
];
