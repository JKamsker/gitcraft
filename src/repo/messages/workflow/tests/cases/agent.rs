use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        232,
        Case {
            raw: "Move commit down",
            id: "repo-workflows-agent-rewrite-2630fe5c",
            args: &[],
        },
    ),
    (
        243,
        Case {
            raw: "Create branch opaque-name/Stage1/日本語/00001234",
            id: "repo-workflows-agent-create-branch-d2a49cac",
            args: &[(
                "name",
                Sample::Text(
                    "opaque-name/Stage1/日本語/00001234",
                    "opaque-name/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        244,
        Case {
            raw: "Move commit up",
            id: "repo-workflows-agent-rewrite-3d1ccb79",
            args: &[],
        },
    ),
    (
        245,
        Case {
            raw: "Delete branch opaque-name/Stage1/日本語/00001234",
            id: "repo-workflows-agent-delete-branch-d32d800a",
            args: &[(
                "name",
                Sample::Text(
                    "opaque-name/Stage1/日本語/00001234",
                    "opaque-name/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        272,
        Case {
            raw: "Rename opaque-from/Stage1/日本語/00001234 to opaque-to/Stage1/日本語/00001234",
            id: "repo-workflows-agent-rename-branch-5de8c7a7",
            args: &[
                (
                    "from",
                    Sample::Text(
                        "opaque-from/Stage1/日本語/00001234",
                        "opaque-from/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "to",
                    Sample::Text(
                        "opaque-to/Stage1/日本語/00001234",
                        "opaque-to/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        273,
        Case {
            raw: "Drop commit",
            id: "repo-workflows-agent-rewrite-601e2a07",
            args: &[],
        },
    ),
    (
        283,
        Case {
            raw: "Force push",
            id: "repo-workflows-agent-push-1d502fc3",
            args: &[],
        },
    ),
    (
        287,
        Case {
            raw: "Checkout opaque-branch/Stage1/日本語/00001234",
            id: "repo-workflows-agent-checkout-734452f6",
            args: &[(
                "branch",
                Sample::Text(
                    "opaque-branch/Stage1/日本語/00001234",
                    "opaque-branch/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        297,
        Case {
            raw: "opaque-running-agent-1/Stage1/日本語/00001234: Push: done",
            id: "repo-workflows-agent-agent-op-done-16d1e9b2",
            args: &[
                (
                    "running-agent-1",
                    Sample::Text(
                        "opaque-running-agent-1/Stage1/日本語/00001234",
                        "opaque-running-agent-1/Stage1/日本語/00001234",
                    ),
                ),
                ("label", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
    (
        298,
        Case {
            raw: "Unstage opaque-paths-label-paths-1/Stage1/日本語/00001234",
            id: "repo-workflows-agent-unstage-eb003390",
            args: &[(
                "paths-label-paths-1",
                Sample::Text(
                    "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                    "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        310,
        Case {
            raw: "1234 paths",
            id: "repo-workflows-agent-paths-label-26d36d06",
            args: &[("many-len-1", Sample::Number(1234))],
        },
    ),
    (
        311,
        Case {
            raw: "Stage opaque-paths-label-paths-1/Stage1/日本語/00001234",
            id: "repo-workflows-agent-stage-3b4ddea7",
            args: &[(
                "paths-label-paths-1",
                Sample::Text(
                    "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                    "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
