use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        39,
        Case {
            raw: "Didn't check out opaque-leave-label-1/Stage1/日本語/00001234: couldn't list the commits it would leave on no branch (opaque-e/Stage1/日本語/00001234)",
            id: "repo-workflows-detached-leave-to-48b0ce4c",
            args: &[
                (
                    "leave-label-1",
                    Sample::Text(
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "e",
                    Sample::Text(
                        "opaque-e/Stage1/日本語/00001234",
                        "opaque-e/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        90,
        Case {
            raw: "Wait for Push to finish, then check out again",
            id: "repo-workflows-detached-detached-act-70dad4a3",
            args: &[("busy", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        104,
        Case {
            raw: "Didn't check out opaque-leave-label-1/Stage1/日本語/00001234: Push started meanwhile",
            id: "repo-workflows-detached-leave-to-007a4c0e",
            args: &[
                (
                    "leave-label-1",
                    Sample::Text(
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                    ),
                ),
                ("busy", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
    (
        137,
        Case {
            raw: "Checkout opaque-leave-label-1/Stage1/日本語/00001234: wait for Push to finish",
            id: "repo-workflows-detached-leave-to-28ae5898",
            args: &[
                (
                    "leave-label-1",
                    Sample::Text(
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                        "opaque-leave-label-1/Stage1/日本語/00001234",
                    ),
                ),
                ("busy", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
];
