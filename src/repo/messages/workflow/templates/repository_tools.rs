pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        32,
        (
            "(?s)^Dependencies finished, but the parent repository, profile or sign\\-in changed\\. Prepare its push again\\.$",
            "repo-workflows-repository_tools-dependency-job-a7b1dc6d",
            &[],
        ),
    ),
    (
        40,
        (
            "(?s)^Go online and wait for the current operation before preparing an outbox push$",
            "repo-workflows-repository_tools-act-d64d1e25",
            &[],
        ),
    ),
    (
        48,
        (
            "(?s)^Prepared subtree profile/sign\\-in changed; prepare and approve again$",
            "repo-workflows-repository_tools-act-d021ec81",
            &[],
        ),
    ),
    (
        52,
        (
            "(?s)^Prepared outbox profile/sign\\-in changed; prepare and approve again$",
            "repo-workflows-repository_tools-start-send-628a48e6",
            &[],
        ),
    ),
    (
        77,
        (
            "(?s)^Convert legacy grafts to removable replace refs$",
            "repo-workflows-repository_tools-repository-tools-act-ad133eb8",
            &[],
        ),
    ),
    (
        83,
        (
            "(?s)^Choose one saved turn to preview its inverse$",
            "repo-workflows-repository_tools-act-6dc9a943",
            &[],
        ),
    ),
    (
        91,
        (
            "(?s)^Publish dependencies before parent review$",
            "repo-workflows-repository_tools-dependency-job-f48f9c31",
            &[],
        ),
    ),
    (
        96,
        (
            "(?s)^Check submodule publication dependencies$",
            "repo-workflows-repository_tools-dependency-job-57895189",
            &[],
        ),
    ),
    (
        106,
        (
            "(?s)^Change virtual branch materialization$",
            "repo-workflows-repository_tools-act-ce061d46",
            &[],
        ),
    ),
    (
        109,
        (
            "(?s)^Autosend opted\\-in plain outbox push$",
            "repo-workflows-repository_tools-start-send-e7b0c78d",
            &[],
        ),
    ),
    (
        113,
        (
            "(?s)^Join imported histories temporarily$",
            "repo-workflows-repository_tools-repository-tools-act-a7b0c90c",
            &[],
        ),
    ),
    (
        120,
        (
            "(?s)^Connection changed; reopen outbox$",
            "repo-workflows-repository_tools-act-ccd2fdda",
            &[],
        ),
    ),
    (
        121,
        (
            "(?s)^Commit virtual branch owned hunks$",
            "repo-workflows-repository_tools-act-de986fa8",
            &[],
        ),
    ),
    (
        132,
        (
            "(?s)^Reconcile exact replay receipts$",
            "repo-workflows-repository_tools-act-c08c65bd",
            &[],
        ),
    ),
    (
        139,
        (
            "(?s)^Undo accepted isolated replay$",
            "repo-workflows-repository_tools-act-346c061a",
            &[],
        ),
    ),
    (
        140,
        (
            "(?s)^Save virtual branch ownership$",
            "repo-workflows-repository_tools-act-901dccbe",
            &[],
        ),
    ),
    (
        141,
        (
            "(?s)^Undo virtual branch operation$",
            "repo-workflows-repository_tools-act-63403bc6",
            &[],
        ),
    ),
    (
        147,
        (
            "(?s)^Replay in isolated worktrees$",
            "repo-workflows-repository_tools-act-5bd57c48",
            &[],
        ),
    ),
    (
        153,
        (
            "(?s)^Publish exact subtree split$",
            "repo-workflows-repository_tools-act-494336ed",
            &[],
        ),
    ),
    (
        166,
        (
            "(?s)^Resolve submodule pointer$",
            "repo-workflows-repository_tools-repository-tools-act-66f98362",
            &[],
        ),
    ),
    (
        167,
        (
            "(?s)^Save pull and push routes$",
            "repo-workflows-repository_tools-repository-tools-act-82d14313",
            &[],
        ),
    ),
    (
        168,
        (
            "(?s)^Save remote configuration$",
            "repo-workflows-repository_tools-repository-tools-act-4e161156",
            &[],
        ),
    ),
    (
        174,
        (
            "(?s)^Continue isolated replay$",
            "repo-workflows-repository_tools-act-65d162a8",
            &[],
        ),
    ),
    (
        180,
        (
            "(?s)^Close isolated proposal$",
            "repo-workflows-repository_tools-act-08cd49af",
            &[],
        ),
    ),
    (
        181,
        (
            "(?s)^Unshelve one changelist$",
            "repo-workflows-repository_tools-act-551afaa9",
            &[],
        ),
    ),
    (
        188,
        (
            "(?s)^Accept isolated replay$",
            "repo-workflows-repository_tools-act-b64c091a",
            &[],
        ),
    ),
    (
        189,
        (
            "(?s)^Make clone independent$",
            "repo-workflows-repository_tools-repository-tools-act-07fb4089",
            &[],
        ),
    ),
    (
        190,
        (
            "(?s)^Save publication group$",
            "repo-workflows-repository_tools-repository-tools-act-3c1c7a3a",
            &[],
        ),
    ),
    (
        191,
        (
            "(?s)^Send exact queued push$",
            "repo-workflows-repository_tools-start-send-994ea6b0",
            &[],
        ),
    ),
    (
        199,
        (
            "(?s)^Commit one changelist$",
            "repo-workflows-repository_tools-act-c1eec3a9",
            &[],
        ),
    ),
    (
        200,
        (
            "(?s)^Shelve one changelist$",
            "repo-workflows-repository_tools-act-bea96408",
            &[],
        ),
    ),
    (
        201,
        (
            "(?s)^Revert one saved turn$",
            "repo-workflows-repository_tools-act-fc328635",
            &[],
        ),
    ),
    (
        213,
        (
            "(?s)^Migrate ref storage$",
            "repo-workflows-repository_tools-repository-tools-act-888e8b45",
            &[],
        ),
    ),
    (
        214,
        (
            "(?s)^Remove outbox entry$",
            "repo-workflows-repository_tools-act-7f694b62",
            &[],
        ),
    ),
    (
        219,
        (
            "(?s)^Remove replace ref$",
            "repo-workflows-repository_tools-repository-tools-act-69a2d9db",
            &[],
        ),
    ),
    (
        227,
        (
            "(?s)^(.*?) is still running$",
            "repo-workflows-repository_tools-repository-tool-mutation-988356ee",
            &[("busy", false)],
        ),
    ),
    (
        228,
        (
            "(?s)^Subtree operation$",
            "repo-workflows-repository_tools-act-88c16034",
            &[],
        ),
    ),
    (
        234,
        (
            "(?s)^Isolated restack$",
            "repo-workflows-repository_tools-show-9174163c",
            &[],
        ),
    ),
    (
        235,
        (
            "(?s)^Remote preview: (.*?)$",
            "repo-workflows-repository_tools-remote-config-preview-bba0304c",
            &[("e", false)],
        ),
    ),
    (
        236,
        (
            "(?s)^Queue exact push$",
            "repo-workflows-repository_tools-act-b967f21f",
            &[],
        ),
    ),
    (
        239,
        (
            "(?s)^Save changelist$",
            "repo-workflows-repository_tools-act-a3cbb206",
            &[],
        ),
    ),
    (
        256,
        (
            "(?s)^Publish group$",
            "repo-workflows-repository_tools-publish-group-3cc9a2c6",
            &[],
        ),
    ),
    (
        306,
        (
            "(?s)^Rebase (.*?)$",
            "repo-workflows-repository_tools-show-34b10b8d",
            &[("name", false)],
        ),
    ),
];
