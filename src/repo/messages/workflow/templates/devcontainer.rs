pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        131,
        (
            "(?s)^Cancelled before terminal start$",
            "repo-workflows-devcontainer-terminal-f171832a",
            &[],
        ),
    ),
    (
        165,
        (
            "(?s)^Cancelled before starting$",
            "repo-workflows-devcontainer-devcontainer-act-84f8f92f",
            &[],
        ),
    ),
    (
        179,
        (
            "(?s)^Dev container operation$",
            "repo-workflows-devcontainer-devcontainer-act-519a489a",
            &[],
        ),
    ),
    (
        254,
        (
            "(?s)^Container (.*?) · (.*?)$",
            "repo-workflows-devcontainer-terminal-77e417ed",
            &[("binding-id-12-1", false), ("binding-folder-2", false)],
        ),
    ),
];
