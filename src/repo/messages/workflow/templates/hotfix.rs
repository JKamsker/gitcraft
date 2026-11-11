pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        99,
        (
            "(?s)^(.*?) isn't a version to start a hotfix from$",
            "repo-workflows-hotfix-hotfix-act-06b55025",
            &[("tag", false)],
        ),
    ),
    (
        328,
        (
            "(?s)^(.*?) \\(stopped part\\-way\\)$",
            "repo-workflows-hotfix-hotfix-op-done-fbdef87e",
            &[("run-label-1", false)],
        ),
    ),
];
