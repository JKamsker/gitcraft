use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        32,
        Case {
            raw: "Dependencies finished, but the parent repository, profile or sign-in changed. Prepare its push again.",
            id: "repo-workflows-repository_tools-dependency-job-a7b1dc6d",
            args: &[],
        },
    ),
    (
        40,
        Case {
            raw: "Go online and wait for the current operation before preparing an outbox push",
            id: "repo-workflows-repository_tools-act-d64d1e25",
            args: &[],
        },
    ),
    (
        48,
        Case {
            raw: "Prepared subtree profile/sign-in changed; prepare and approve again",
            id: "repo-workflows-repository_tools-act-d021ec81",
            args: &[],
        },
    ),
    (
        52,
        Case {
            raw: "Prepared outbox profile/sign-in changed; prepare and approve again",
            id: "repo-workflows-repository_tools-start-send-628a48e6",
            args: &[],
        },
    ),
    (
        77,
        Case {
            raw: "Convert legacy grafts to removable replace refs",
            id: "repo-workflows-repository_tools-repository-tools-act-ad133eb8",
            args: &[],
        },
    ),
    (
        83,
        Case {
            raw: "Choose one saved turn to preview its inverse",
            id: "repo-workflows-repository_tools-act-6dc9a943",
            args: &[],
        },
    ),
    (
        91,
        Case {
            raw: "Publish dependencies before parent review",
            id: "repo-workflows-repository_tools-dependency-job-f48f9c31",
            args: &[],
        },
    ),
    (
        96,
        Case {
            raw: "Check submodule publication dependencies",
            id: "repo-workflows-repository_tools-dependency-job-57895189",
            args: &[],
        },
    ),
    (
        106,
        Case {
            raw: "Change virtual branch materialization",
            id: "repo-workflows-repository_tools-act-ce061d46",
            args: &[],
        },
    ),
    (
        109,
        Case {
            raw: "Autosend opted-in plain outbox push",
            id: "repo-workflows-repository_tools-start-send-e7b0c78d",
            args: &[],
        },
    ),
    (
        113,
        Case {
            raw: "Join imported histories temporarily",
            id: "repo-workflows-repository_tools-repository-tools-act-a7b0c90c",
            args: &[],
        },
    ),
    (
        120,
        Case {
            raw: "Connection changed; reopen outbox",
            id: "repo-workflows-repository_tools-act-ccd2fdda",
            args: &[],
        },
    ),
    (
        121,
        Case {
            raw: "Commit virtual branch owned hunks",
            id: "repo-workflows-repository_tools-act-de986fa8",
            args: &[],
        },
    ),
    (
        132,
        Case {
            raw: "Reconcile exact replay receipts",
            id: "repo-workflows-repository_tools-act-c08c65bd",
            args: &[],
        },
    ),
    (
        139,
        Case {
            raw: "Undo accepted isolated replay",
            id: "repo-workflows-repository_tools-act-346c061a",
            args: &[],
        },
    ),
    (
        140,
        Case {
            raw: "Save virtual branch ownership",
            id: "repo-workflows-repository_tools-act-901dccbe",
            args: &[],
        },
    ),
    (
        141,
        Case {
            raw: "Undo virtual branch operation",
            id: "repo-workflows-repository_tools-act-63403bc6",
            args: &[],
        },
    ),
    (
        147,
        Case {
            raw: "Replay in isolated worktrees",
            id: "repo-workflows-repository_tools-act-5bd57c48",
            args: &[],
        },
    ),
    (
        153,
        Case {
            raw: "Publish exact subtree split",
            id: "repo-workflows-repository_tools-act-494336ed",
            args: &[],
        },
    ),
    (
        166,
        Case {
            raw: "Resolve submodule pointer",
            id: "repo-workflows-repository_tools-repository-tools-act-66f98362",
            args: &[],
        },
    ),
    (
        167,
        Case {
            raw: "Save pull and push routes",
            id: "repo-workflows-repository_tools-repository-tools-act-82d14313",
            args: &[],
        },
    ),
    (
        168,
        Case {
            raw: "Save remote configuration",
            id: "repo-workflows-repository_tools-repository-tools-act-4e161156",
            args: &[],
        },
    ),
    (
        174,
        Case {
            raw: "Continue isolated replay",
            id: "repo-workflows-repository_tools-act-65d162a8",
            args: &[],
        },
    ),
    (
        180,
        Case {
            raw: "Close isolated proposal",
            id: "repo-workflows-repository_tools-act-08cd49af",
            args: &[],
        },
    ),
    (
        181,
        Case {
            raw: "Unshelve one changelist",
            id: "repo-workflows-repository_tools-act-551afaa9",
            args: &[],
        },
    ),
    (
        188,
        Case {
            raw: "Accept isolated replay",
            id: "repo-workflows-repository_tools-act-b64c091a",
            args: &[],
        },
    ),
    (
        189,
        Case {
            raw: "Make clone independent",
            id: "repo-workflows-repository_tools-repository-tools-act-07fb4089",
            args: &[],
        },
    ),
    (
        190,
        Case {
            raw: "Save publication group",
            id: "repo-workflows-repository_tools-repository-tools-act-3c1c7a3a",
            args: &[],
        },
    ),
    (
        191,
        Case {
            raw: "Send exact queued push",
            id: "repo-workflows-repository_tools-start-send-994ea6b0",
            args: &[],
        },
    ),
    (
        199,
        Case {
            raw: "Commit one changelist",
            id: "repo-workflows-repository_tools-act-c1eec3a9",
            args: &[],
        },
    ),
    (
        200,
        Case {
            raw: "Shelve one changelist",
            id: "repo-workflows-repository_tools-act-bea96408",
            args: &[],
        },
    ),
    (
        201,
        Case {
            raw: "Revert one saved turn",
            id: "repo-workflows-repository_tools-act-fc328635",
            args: &[],
        },
    ),
    (
        213,
        Case {
            raw: "Migrate ref storage",
            id: "repo-workflows-repository_tools-repository-tools-act-888e8b45",
            args: &[],
        },
    ),
    (
        214,
        Case {
            raw: "Remove outbox entry",
            id: "repo-workflows-repository_tools-act-7f694b62",
            args: &[],
        },
    ),
    (
        219,
        Case {
            raw: "Remove replace ref",
            id: "repo-workflows-repository_tools-repository-tools-act-69a2d9db",
            args: &[],
        },
    ),
    (
        227,
        Case {
            raw: "Push is still running",
            id: "repo-workflows-repository_tools-repository-tool-mutation-988356ee",
            args: &[("busy", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        228,
        Case {
            raw: "Subtree operation",
            id: "repo-workflows-repository_tools-act-88c16034",
            args: &[],
        },
    ),
    (
        234,
        Case {
            raw: "Isolated restack",
            id: "repo-workflows-repository_tools-show-9174163c",
            args: &[],
        },
    ),
    (
        235,
        Case {
            raw: "Remote preview: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-repository_tools-remote-config-preview-bba0304c",
            args: &[(
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        236,
        Case {
            raw: "Queue exact push",
            id: "repo-workflows-repository_tools-act-b967f21f",
            args: &[],
        },
    ),
    (
        239,
        Case {
            raw: "Save changelist",
            id: "repo-workflows-repository_tools-act-a3cbb206",
            args: &[],
        },
    ),
    (
        256,
        Case {
            raw: "Publish group",
            id: "repo-workflows-repository_tools-publish-group-3cc9a2c6",
            args: &[],
        },
    ),
    (
        306,
        Case {
            raw: "Rebase opaque-name/Stage1/日本語/00001234",
            id: "repo-workflows-repository_tools-show-34b10b8d",
            args: &[(
                "name",
                Sample::Text(
                    "opaque-name/Stage1/日本語/00001234",
                    "opaque-name/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
