pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        39,
        (
            "(?s)^Didn't check out (.*?): couldn't list the commits it would leave on no branch \\((.*?)\\)$",
            "repo-workflows-detached-leave-to-48b0ce4c",
            &[("leave-label-1", false), ("e", false)],
        ),
    ),
    (
        90,
        (
            "(?s)^Wait for (.*?) to finish, then check out again$",
            "repo-workflows-detached-detached-act-70dad4a3",
            &[("busy", false)],
        ),
    ),
    (
        104,
        (
            "(?s)^Didn't check out (.*?): (.*?) started meanwhile$",
            "repo-workflows-detached-leave-to-007a4c0e",
            &[("leave-label-1", false), ("busy", false)],
        ),
    ),
    (
        137,
        (
            "(?s)^Checkout (.*?): wait for (.*?) to finish$",
            "repo-workflows-detached-leave-to-28ae5898",
            &[("leave-label-1", false), ("busy", false)],
        ),
    ),
];
