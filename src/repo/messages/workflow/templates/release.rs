pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        17,
        (
            "(?s)^Cut release (.*?)$",
            "repo-workflows-release-release-stopped-fbe3bbf7",
            &[("run-plan-tag-1", false)],
        ),
    ),
    (
        108,
        (
            "(?s)^Wait for the operation running to end$",
            "repo-workflows-release-run-release-d9523bf2",
            &[],
        ),
    ),
    (
        276,
        (
            "(?s)^(.*?): (.*?) failed: (.*?)$",
            "repo-workflows-release-release-stopped-a4afd9be",
            &[("label", false), ("step-label-1", false), ("e", false)],
        ),
    ),
    (
        292,
        (
            "(?s)^Released (.*?)$",
            "repo-workflows-release-release-stopped-1904d12e",
            &[("run-plan-tag-1", false)],
        ),
    ),
    (
        293,
        (
            "(?s)^(.*?): stopped$",
            "repo-workflows-release-release-stopped-328c7ab5",
            &[("label", false)],
        ),
    ),
];
