pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        74,
        (
            "(?s)^Couldn't save Sign off for this repository \\(git config (.*?)\\): (.*?)$",
            "repo-workflows-dco-dco-act-10fe4a7c",
            &[("signoff-sign-off-key-1", false), ("e", false)],
        ),
    ),
    (
        124,
        (
            "(?s)^Push the signed\\-off commits of (.*?)$",
            "repo-workflows-dco-dco-op-done-55d2b32b",
            &[("plan-branch-1", false)],
        ),
    ),
    (
        160,
        (
            "(?s)^Sign off the commits of (.*?) \\(here only\\)$",
            "repo-workflows-dco-dco-op-done-5d0c9ff1",
            &[("plan-branch-1", false)],
        ),
    ),
    (
        173,
        (
            "(?s)^Sign off the commits of (.*?)$",
            "repo-workflows-dco-fix-label-dfc76b8d",
            &[("plan-branch-1", false)],
        ),
    ),
];
