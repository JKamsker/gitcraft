pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        25,
        (
            "(?s)^Restack aborted: every branch is back where it was but (.*?), which got commits after it was restacked and is left as it is$",
            "repo-workflows-stack-restack-done-c27578ee",
            &[("kept-join-1", false)],
        ),
    ),
    (
        35,
        (
            "(?s)^([0-9]+) of ([0-9]+) done, then (.*?) failed: (.*?)\\. Publish again to go on: what was done stays, no (.*?) is opened twice\\.$",
            "repo-workflows-stack-publish-done-6ffb5e8c",
            &[
                ("report-done-len-1", true),
                ("total", true),
                ("branch", false),
                ("e", false),
                ("noun", false),
            ],
        ),
    ),
    (
        38,
        (
            "(?s)^This repository isn't on a forge gitcraft knows: there is nowhere to publish to\\.$",
            "repo-workflows-stack-open-publish-a23e7f8b",
            &[],
        ),
    ),
    (
        45,
        (
            "(?s)^Restack stopped at (.*?) on conflicts: resolve them(.*?), then Continue restack \\(or Abort restack\\)$",
            "repo-workflows-stack-restack-done-33b4b547",
            &[("branch", false), ("place", false)],
        ),
    ),
    (
        60,
        (
            "(?s)^Nothing to restack: every layer is on its parent's tip\\.$",
            "repo-workflows-stack-open-restack-419e0d8e",
            &[],
        ),
    ),
    (
        66,
        (
            "(?s)^Restack: every branch was already on its parent's tip$",
            "repo-workflows-stack-restack-done-1fc5c0ba",
            &[],
        ),
    ),
    (
        69,
        (
            "(?s)^Restack aborted: every branch is back where it was$",
            "repo-workflows-stack-restack-done-a7222989",
            &[],
        ),
    ),
    (
        71,
        (
            "(?s)^Restack: ([0-9]+) branches moved · Ctrl\\+Z puts them back$",
            "repo-workflows-stack-restack-done-7f6cce25",
            &[("n", true)],
        ),
    ),
    (
        80,
        (
            "(?s)^Restack: 1 branch moved · Ctrl\\+Z puts it back$",
            "repo-workflows-stack-restack-done-dbfb6ce0",
            &[],
        ),
    ),
    (
        98,
        (
            "(?s)^(.*?) is gone: pick what (.*?) is based on first\\.$",
            "repo-workflows-stack-open-publish-3e534e60",
            &[("layer-parent-1", false), ("layer-branch-2", false)],
        ),
    ),
    (
        115,
        (
            "(?s)^Publish: run it again once (.*?) is done$",
            "repo-workflows-stack-start-publish-e0c26cc9",
            &[("busy", false)],
        ),
    ),
    (
        149,
        (
            "(?s)^(.*?): run it again once (.*?) is done$",
            "repo-workflows-stack-run-stack-op-95c7214f",
            &[("label", false), ("busy", false)],
        ),
    ),
    (
        163,
        (
            "(?s)^Couldn't read the stacks: (.*?)$",
            "repo-workflows-stack-remove-parent-4272623a",
            &[("e", false)],
        ),
    ),
    (
        177,
        (
            "(?s)^Publish (.*?)s: nothing to do$",
            "repo-workflows-stack-summary-47eeff3f",
            &[("noun", false)],
        ),
    ),
    (
        195,
        (
            "(?s)^Remove (.*?) from its stack$",
            "repo-workflows-stack-remove-parent-e6733ec1",
            &[("branch", false)],
        ),
    ),
    (
        209,
        (
            "(?s)^ in its worktree at (.*?)$",
            "repo-workflows-stack-restack-done-6cecc855",
            &[("wt-display-1", false)],
        ),
    ),
    (
        257,
        (
            "(?s)^Published (.*?)s: (.*?)$",
            "repo-workflows-stack-summary-7f75368f",
            &[("noun", false), ("parts-join-1", false)],
        ),
    ),
    (
        258,
        (
            "(?s)^Abort restack$",
            "repo-workflows-stack-abort-restack-4fe4c0de",
            &[],
        ),
    ),
    (
        294,
        (
            "(?s)^Base (.*?) on (.*?)$",
            "repo-workflows-stack-set-parent-81fef295",
            &[("branch", false), ("parent", false)],
        ),
    ),
    (
        295,
        (
            "(?s)^Publish (.*?)s$",
            "repo-workflows-stack-start-publish-c4ffa0c1",
            &[("noun", false)],
        ),
    ),
    (
        307,
        (
            "(?s)^Restack$",
            "repo-workflows-stack-start-restack-9fbc2915",
            &[],
        ),
    ),
];
