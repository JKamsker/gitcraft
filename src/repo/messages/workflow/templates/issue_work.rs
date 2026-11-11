pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        75,
        (
            "(?s)^Tracker account changed; reload the issue source$",
            "repo-workflows-issue_work-start-issue-563140a4",
            &[],
        ),
    ),
    (
        110,
        (
            "(?s)^Couldn't read the branches' issues: (.*?)$",
            "repo-workflows-issue_work-take-issue-read-bbcbbb59",
            &[("e", false)],
        ),
    ),
    (
        143,
        (
            "(?s)^Forget the issue of (.*?) failed: (.*?)$",
            "repo-workflows-issue_work-unlink-issue-6df1900e",
            &[("branch", false), ("e", false)],
        ),
    ),
    (
        169,
        (
            "(?s)^Start work on (.*?): wait for (.*?)$",
            "repo-workflows-issue_work-start-issue-e2b80c1b",
            &[("issue-key-1", false), ("busy", false)],
        ),
    ),
    (
        237,
        (
            "(?s)^Start work on (.*?): (.*?)$",
            "repo-workflows-issue_work-start-issue-c14dd822",
            &[("issue-key-1", false), ("why", false)],
        ),
    ),
    (
        275,
        (
            "(?s)^(.*?): wait for (.*?)$",
            "repo-workflows-issue_work-refresh-issue-a0e9ad2a",
            &[("label", false), ("busy", false)],
        ),
    ),
];
