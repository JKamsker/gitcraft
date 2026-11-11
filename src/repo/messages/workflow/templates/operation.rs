pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        111,
        (
            "(?s)^Distribute staged fixes into ([0-9]+) commit(?:s?)$",
            "repo-workflows-operation-distribute-staged",
            &[("count", true)],
        ),
    ),
    (
        145,
        (
            "(?s)^Add selected SSH key to agent$",
            "repo-workflows-operation-ssh-14022",
            &[],
        ),
    ),
    (
        146,
        (
            "(?s)^Register public SSH key there$",
            "repo-workflows-operation-ssh-14048",
            &[],
        ),
    ),
    (
        158,
        (
            "(?s)^Test selected SSH key there$",
            "repo-workflows-operation-ssh-14038",
            &[],
        ),
    ),
    (
        164,
        (
            "(?s)^Add SSH key to agent there$",
            "repo-workflows-operation-ssh-14024",
            &[],
        ),
    ),
    (
        178,
        (
            "(?s)^Rebase \\#(.*?) onto (.*?) and push$",
            "repo-workflows-operation-rebase-fork-push",
            &[("number", false), ("base", false)],
        ),
    ),
    (
        197,
        (
            "(?s)^LFS: Prune old objects$",
            "repo-workflows-operation-lfs-prune",
            &[],
        ),
    ),
    (
        205,
        (
            "(?s)^Push the rebased (.*?) to (.*?)$",
            "repo-workflows-operation-push-rebased",
            &[("branch", false), ("remote", false)],
        ),
    ),
    (
        206,
        (
            "(?s)^Test selected SSH key$",
            "repo-workflows-operation-ssh-14036",
            &[],
        ),
    ),
    (
        212,
        (
            "(?s)^Create SSH key there$",
            "repo-workflows-operation-ssh-14019",
            &[],
        ),
    ),
    (
        217,
        (
            "(?s)^Force unlock ([0-9]+) files$",
            "repo-workflows-operation-force-unlock-count",
            &[("count", true)],
        ),
    ),
    (
        224,
        (
            "(?s)^Mark in the bisect$",
            "repo-workflows-operation-mark-bisect",
            &[],
        ),
    ),
    (
        250,
        (
            "(?s)^Finish hotfix (.*?)$",
            "repo-workflows-operation-finish-hotfix",
            &[("tag", false)],
        ),
    ),
    (
        251,
        (
            "(?s)^Start work on (.*?)$",
            "repo-workflows-operation-start-issue-work",
            &[("issue", false)],
        ),
    ),
    (
        252,
        (
            "(?s)^Create SSH key$",
            "repo-workflows-operation-ssh-14017",
            &[],
        ),
    ),
    (
        261,
        (
            "(?s)^Force unlock (.*?)$",
            "repo-workflows-operation-force-unlock",
            &[("path", false)],
        ),
    ),
    (
        262,
        (
            "(?s)^Track (.*?) in LFS$",
            "repo-workflows-operation-lfs-track",
            &[("pattern", false)],
        ),
    ),
    (
        271,
        (
            "(?s)^Push hotfix (.*?)$",
            "repo-workflows-operation-push-hotfix",
            &[("tag", false)],
        ),
    ),
    (
        305,
        ("(?s)^Snapshot$", "repo-workflows-operation-snapshot", &[]),
    ),
    (
        330,
        (
            "(?s)^(.*?) \\(here only\\)$",
            "repo-workflows-operation-here-only",
            &[("operation", false)],
        ),
    ),
];
