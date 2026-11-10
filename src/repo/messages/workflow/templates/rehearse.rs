pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        44,
        (
            "(?s)^Commit or stash your changes first: the rehearsal was of the commits alone$",
            "repo-workflows-rehearse-apply-rehearsal-b09a6f3f",
            &[],
        ),
    ),
    (
        93,
        (
            "(?s)^(.*?) is no longer checked out: rehearse again$",
            "repo-workflows-rehearse-apply-rehearsal-d63206ac",
            &[("run-branch-1", false)],
        ),
    ),
    (
        103,
        (
            "(?s)^Check out a branch to rehearse with it$",
            "repo-workflows-rehearse-start-rehearsal-252af634",
            &[],
        ),
    ),
    (
        135,
        (
            "(?s)^A rehearsal is going on already$",
            "repo-workflows-rehearse-start-rehearsal-bbfc5a1d",
            &[],
        ),
    ),
];
