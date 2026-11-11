pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        232,
        (
            "(?s)^Move commit down$",
            "repo-workflows-agent-rewrite-2630fe5c",
            &[],
        ),
    ),
    (
        243,
        (
            "(?s)^Create branch (.*?)$",
            "repo-workflows-agent-create-branch-d2a49cac",
            &[("name", false)],
        ),
    ),
    (
        244,
        (
            "(?s)^Move commit up$",
            "repo-workflows-agent-rewrite-3d1ccb79",
            &[],
        ),
    ),
    (
        245,
        (
            "(?s)^Delete branch (.*?)$",
            "repo-workflows-agent-delete-branch-d32d800a",
            &[("name", false)],
        ),
    ),
    (
        263,
        (
            "(?s)^Cherry\\-pick$",
            "repo-workflows-agent-plan-4c49fdb6",
            &[],
        ),
    ),
    (
        272,
        (
            "(?s)^Rename (.*?) to (.*?)$",
            "repo-workflows-agent-rename-branch-5de8c7a7",
            &[("from", false), ("to", false)],
        ),
    ),
    (
        273,
        (
            "(?s)^Drop commit$",
            "repo-workflows-agent-rewrite-601e2a07",
            &[],
        ),
    ),
    (
        283,
        (
            "(?s)^Force push$",
            "repo-workflows-agent-push-1d502fc3",
            &[],
        ),
    ),
    (
        287,
        (
            "(?s)^Checkout (.*?)$",
            "repo-workflows-agent-checkout-734452f6",
            &[("branch", false)],
        ),
    ),
    (
        297,
        (
            "(?s)^(.*?): (.*?): done$",
            "repo-workflows-agent-agent-op-done-16d1e9b2",
            &[("running-agent-1", false), ("label", false)],
        ),
    ),
    (
        298,
        (
            "(?s)^Unstage (.*?)$",
            "repo-workflows-agent-unstage-eb003390",
            &[("paths-label-paths-1", false)],
        ),
    ),
    (
        309,
        ("(?s)^Revert$", "repo-workflows-agent-plan-0026c505", &[]),
    ),
    (
        310,
        (
            "(?s)^([0-9]+) paths$",
            "repo-workflows-agent-paths-label-26d36d06",
            &[("many-len-1", true)],
        ),
    ),
    (
        311,
        (
            "(?s)^Stage (.*?)$",
            "repo-workflows-agent-stage-3b4ddea7",
            &[("paths-label-paths-1", false)],
        ),
    ),
    (
        312,
        ("(?s)^Commit$", "repo-workflows-agent-commit-82a9c46f", &[]),
    ),
    (
        317,
        ("(?s)^Amend$", "repo-workflows-agent-commit-4475f515", &[]),
    ),
    (
        318,
        ("(?s)^Stash$", "repo-workflows-agent-stash-38bdfeaa", &[]),
    ),
    (
        324,
        ("(?s)^Push$", "repo-workflows-agent-push-731ce7ed", &[]),
    ),
];
