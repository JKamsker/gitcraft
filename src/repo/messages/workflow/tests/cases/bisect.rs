use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        187,
        Case {
            raw: "Can't start a bisect: opaque-why/Stage1/日本語/00001234",
            id: "repo-workflows-bisect-start-bisect-5c82e3e0",
            args: &[(
                "why",
                Sample::Text(
                    "opaque-why/Stage1/日本語/00001234",
                    "opaque-why/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        198,
        Case {
            raw: "No bisect is going on",
            id: "repo-workflows-bisect-mark-bisect-a1c6c771",
            args: &[],
        },
    ),
    (
        233,
        Case {
            raw: "Skip this commit",
            id: "repo-workflows-bisect-mark-bisect-15306802",
            args: &[],
        },
    ),
    (
        253,
        Case {
            raw: "Finish bisect",
            id: "repo-workflows-bisect-reset-bisect-1bdcc155",
            args: &[],
        },
    ),
    (
        264,
        Case {
            raw: "Start bisect",
            id: "repo-workflows-bisect-start-bisect-db936451",
            args: &[],
        },
    ),
    (
        274,
        Case {
            raw: "Stop bisect",
            id: "repo-workflows-bisect-reset-bisect-0ddac35c",
            args: &[],
        },
    ),
    (
        284,
        Case {
            raw: "Bisect run",
            id: "repo-workflows-bisect-start-bisect-run-d300d73e",
            args: &[],
        },
    ),
    (
        288,
        Case {
            raw: "Mark opaque-short-rev-1/Stage1/日本語/00001234 as opaque-bisect-word-mark-2/Stage1/日本語/00001234",
            id: "repo-workflows-bisect-mark-bisect-22f6b91b",
            args: &[
                (
                    "short-rev-1",
                    Sample::Text(
                        "opaque-short-rev-1/Stage1/日本語/00001234",
                        "opaque-short-rev-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "bisect-word-mark-2",
                    Sample::Text(
                        "opaque-bisect-word-mark-2/Stage1/日本語/00001234",
                        "opaque-bisect-word-mark-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        289,
        Case {
            raw: "Push failed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-bisect-bisect-step-done-8336fbfe",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
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
        300,
        Case {
            raw: "Mark as opaque-bisect-word-mark-1/Stage1/日本語/00001234",
            id: "repo-workflows-bisect-mark-bisect-9b0f3138",
            args: &[(
                "bisect-word-mark-1",
                Sample::Text(
                    "opaque-bisect-word-mark-1/Stage1/日本語/00001234",
                    "opaque-bisect-word-mark-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
