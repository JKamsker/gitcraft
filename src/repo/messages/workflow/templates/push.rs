pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        67,
        (
            "(?s)^Push refused, and the remote could not be checked: (.*?)$",
            "repo-workflows-push-remote-checked-c6e528c9",
            &[("e", false)],
        ),
    ),
    (
        97,
        (
            "(?s)^This repository has no remote to push to$",
            "repo-workflows-push-push-labelled-4933a6c9",
            &[],
        ),
    ),
    (
        129,
        (
            "(?s)^(.*?) failed, so nothing was pushed: (.*?)$",
            "repo-workflows-push-chain-step-ended-dfa32698",
            &[("what", false), ("e", false)],
        ),
    ),
    (
        175,
        (
            "(?s)^Sync: (.*?) pulled and pushed$",
            "repo-workflows-push-sync-027fe2bd",
            &[("current", false)],
        ),
    ),
    (
        176,
        (
            "(?s)^(.*?) pulled from (.*?) and pushed$",
            "repo-workflows-push-push-act-ebd6e3a6",
            &[("r-branch-1", false), ("r-target-label-2", false)],
        ),
    ),
    (
        192,
        (
            "(?s)^Push refused: (.*?) is gone$",
            "repo-workflows-push-remote-checked-205a41fa",
            &[("target-label-1", false)],
        ),
    ),
    (
        221,
        (
            "(?s)^Sync: (.*?) published \\(it had nothing to pull\\)$",
            "repo-workflows-push-sync-5ca83b4b",
            &[("current", false)],
        ),
    ),
    (
        246,
        (
            "(?s)^Push refused: (.*?)$",
            "repo-workflows-push-remote-checked-3bbec358",
            &[("banner-headline-r-1", false)],
        ),
    ),
    (
        285,
        (
            "(?s)^(.*?) refused: (.*?)$",
            "repo-workflows-push-push-op-ended-79357f2e",
            &[("label", false), ("out-trim-1", false)],
        ),
    ),
    (
        291,
        (
            "(?s)^Checking (.*?)$",
            "repo-workflows-push-check-remote-9a7c53eb",
            &[("what", false)],
        ),
    ),
];
