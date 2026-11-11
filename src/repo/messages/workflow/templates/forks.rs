pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        26,
        (
            "(?s)^Approving runs the contributor's code on your CI: look through \\#(.*?)'s changes first, then choose Approve and run again$",
            "repo-workflows-forks-approve-runs-a77eca22",
            &[("number", false)],
        ),
    ),
    (
        33,
        (
            "(?s)^The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it \\(or abort it\\)$",
            "repo-workflows-forks-forks-op-done-9c338e90",
            &[],
        ),
    ),
    (
        49,
        (
            "(?s)^(.*?) is rebased here, but not pushed to (.*?): Ctrl\\+Z puts it back as it was$",
            "repo-workflows-forks-forks-op-done-8287ad6b",
            &[("plan-local-1", false), ("plan-fork-2", false)],
        ),
    ),
    (
        84,
        (
            "(?s)^This repository has no remote to rebase onto$",
            "repo-workflows-forks-plan-fork-rebase-1abac341",
            &[],
        ),
    ),
    (
        107,
        (
            "(?s)^Couldn't approve \\#(.*?)'s workflow runs: (.*?)$",
            "repo-workflows-forks-approve-runs-282082c7",
            &[("number", false), ("e", false)],
        ),
    ),
    (
        114,
        (
            "(?s)^Where \\#(.*?) comes from isn't known yet$",
            "repo-workflows-forks-checkout-fork-5ecb3bac",
            &[("number", false)],
        ),
    ),
    (
        117,
        (
            "(?s)^Fetching \\#(.*?)'s branch and its base…$",
            "repo-workflows-forks-plan-fork-rebase-ac71056e",
            &[("number", false)],
        ),
    ),
    (
        133,
        (
            "(?s)^\\#(.*?)'s workflow runs are approved$",
            "repo-workflows-forks-approve-runs-a151ad19",
            &[("number", false)],
        ),
    ),
    (
        142,
        (
            "(?s)^Approving \\#(.*?)'s workflow runs…$",
            "repo-workflows-forks-approve-runs-2f659b28",
            &[("number", false)],
        ),
    ),
    (
        154,
        (
            "(?s)^Couldn't read the remotes: (.*?)$",
            "repo-workflows-forks-plan-tidy-adf73b14",
            &[("e", false)],
        ),
    ),
    (
        161,
        (
            "(?s)^Remove unused fork remotes$",
            "repo-workflows-forks-plan-tidy-a7e148ae",
            &[],
        ),
    ),
    (
        331,
        (
            "(?s)^Check out #([0-9]+) to push to (.*?)$",
            "repo-workflows-forks-checkout-fork-abaaf4c3",
            &[("number", false), ("info-owner-1", false)],
        ),
    ),
];
