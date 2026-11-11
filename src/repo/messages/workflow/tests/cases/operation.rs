use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        111,
        Case {
            raw: "Distribute staged fixes into 1234 commits",
            id: "repo-workflows-operation-distribute-staged",
            args: &[("count", Sample::Number(1234))],
        },
    ),
    (
        145,
        Case {
            raw: "Add selected SSH key to agent",
            id: "repo-workflows-operation-ssh-14022",
            args: &[],
        },
    ),
    (
        146,
        Case {
            raw: "Register public SSH key there",
            id: "repo-workflows-operation-ssh-14048",
            args: &[],
        },
    ),
    (
        158,
        Case {
            raw: "Test selected SSH key there",
            id: "repo-workflows-operation-ssh-14038",
            args: &[],
        },
    ),
    (
        164,
        Case {
            raw: "Add SSH key to agent there",
            id: "repo-workflows-operation-ssh-14024",
            args: &[],
        },
    ),
    (
        178,
        Case {
            raw: "Rebase #opaque-number/Stage1/日本語/00001234 onto opaque-base/Stage1/日本語/00001234 and push",
            id: "repo-workflows-operation-rebase-fork-push",
            args: &[
                (
                    "number",
                    Sample::Text(
                        "opaque-number/Stage1/日本語/00001234",
                        "opaque-number/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "base",
                    Sample::Text(
                        "opaque-base/Stage1/日本語/00001234",
                        "opaque-base/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        197,
        Case {
            raw: "LFS: Prune old objects",
            id: "repo-workflows-operation-lfs-prune",
            args: &[],
        },
    ),
    (
        205,
        Case {
            raw: "Push the rebased opaque-branch/Stage1/日本語/00001234 to opaque-remote/Stage1/日本語/00001234",
            id: "repo-workflows-operation-push-rebased",
            args: &[
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "remote",
                    Sample::Text(
                        "opaque-remote/Stage1/日本語/00001234",
                        "opaque-remote/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        206,
        Case {
            raw: "Test selected SSH key",
            id: "repo-workflows-operation-ssh-14036",
            args: &[],
        },
    ),
    (
        212,
        Case {
            raw: "Create SSH key there",
            id: "repo-workflows-operation-ssh-14019",
            args: &[],
        },
    ),
    (
        217,
        Case {
            raw: "Force unlock 1234 files",
            id: "repo-workflows-operation-force-unlock-count",
            args: &[("count", Sample::Number(1234))],
        },
    ),
    (
        224,
        Case {
            raw: "Mark in the bisect",
            id: "repo-workflows-operation-mark-bisect",
            args: &[],
        },
    ),
    (
        250,
        Case {
            raw: "Finish hotfix opaque-tag/Stage1/日本語/00001234",
            id: "repo-workflows-operation-finish-hotfix",
            args: &[(
                "tag",
                Sample::Text(
                    "opaque-tag/Stage1/日本語/00001234",
                    "opaque-tag/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        251,
        Case {
            raw: "Start work on opaque-issue/Stage1/日本語/00001234",
            id: "repo-workflows-operation-start-issue-work",
            args: &[(
                "issue",
                Sample::Text(
                    "opaque-issue/Stage1/日本語/00001234",
                    "opaque-issue/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        252,
        Case {
            raw: "Create SSH key",
            id: "repo-workflows-operation-ssh-14017",
            args: &[],
        },
    ),
    (
        261,
        Case {
            raw: "Force unlock opaque-path/Stage1/日本語/00001234",
            id: "repo-workflows-operation-force-unlock",
            args: &[(
                "path",
                Sample::Text(
                    "opaque-path/Stage1/日本語/00001234",
                    "opaque-path/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        262,
        Case {
            raw: "Track opaque-pattern/Stage1/日本語/00001234 in LFS",
            id: "repo-workflows-operation-lfs-track",
            args: &[(
                "pattern",
                Sample::Text(
                    "opaque-pattern/Stage1/日本語/00001234",
                    "opaque-pattern/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        271,
        Case {
            raw: "Push hotfix opaque-tag/Stage1/日本語/00001234",
            id: "repo-workflows-operation-push-hotfix",
            args: &[(
                "tag",
                Sample::Text(
                    "opaque-tag/Stage1/日本語/00001234",
                    "opaque-tag/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        305,
        Case {
            raw: "Snapshot",
            id: "repo-workflows-operation-snapshot",
            args: &[],
        },
    ),
    (
        330,
        Case {
            raw: "Push (here only)",
            id: "repo-workflows-operation-here-only",
            args: &[("operation", Sample::Text("Push", "Pushen"))],
        },
    ),
];
