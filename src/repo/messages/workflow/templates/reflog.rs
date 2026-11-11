pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        50,
        (
            "(?s)^The last scan is still ending on that machine; try again in a moment$",
            "repo-workflows-reflog-start-scan-7a1d64fe",
            &[],
        ),
    ),
    (
        270,
        (
            "(?s)^Apply stash (.*?)$",
            "repo-workflows-reflog-apply-stash-93a6ce40",
            &[("short-pick-hash-1", false)],
        ),
    ),
];
