use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        160,
        Case {
            raw: "Sign off the commits of opaque-plan-branch-1/Stage1/日本語/00001234 (here only)",
            id: "repo-core-operation-dco-here",
            args: &[(
                "branch",
                Sample::Text(
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        173,
        Case {
            raw: "Sign off the commits of opaque-plan-branch-1/Stage1/日本語/00001234",
            id: "repo-core-operation-dco",
            args: &[(
                "branch",
                Sample::Text(
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        263,
        Case {
            raw: "Cherry-pick",
            id: "interaction-operation-cherry-pick",
            args: &[],
        },
    ),
    (
        309,
        Case {
            raw: "Revert",
            id: "interaction-operation-revert",
            args: &[],
        },
    ),
    (
        312,
        Case {
            raw: "Commit",
            id: "interaction-operation-commit",
            args: &[],
        },
    ),
    (
        314,
        Case {
            raw: "Rebase",
            id: "interaction-operation-rebase",
            args: &[],
        },
    ),
    (
        317,
        Case {
            raw: "Amend",
            id: "interaction-operation-amend",
            args: &[],
        },
    ),
    (
        318,
        Case {
            raw: "Stash",
            id: "interaction-operation-stash",
            args: &[],
        },
    ),
    (
        322,
        Case {
            raw: "Undo Push",
            id: "interaction-operation-undo",
            args: &[("operation", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        323,
        Case {
            raw: "Redo Push",
            id: "interaction-operation-redo",
            args: &[("operation", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        324,
        Case {
            raw: "Push",
            id: "interaction-operation-push",
            args: &[],
        },
    ),
    (
        325,
        Case {
            raw: "Pull",
            id: "interaction-operation-pull",
            args: &[],
        },
    ),
];
