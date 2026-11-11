use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        17,
        Case {
            raw: "Cut release raw-tag/1234",
            id: "repo-workflows-release-release-stopped-fbe3bbf7",
            args: &[(
                "run-plan-tag-1",
                Sample::Text("raw-tag/1234", "raw-tag/1234"),
            )],
        },
    ),
    (
        108,
        Case {
            raw: "Wait for the operation running to end",
            id: "repo-workflows-release-run-release-d9523bf2",
            args: &[],
        },
    ),
    (
        276,
        Case {
            raw: "Push: Push failed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-release-release-stopped-a4afd9be",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                ("step-label-1", Sample::Text("Push", "Pushen")),
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
        292,
        Case {
            raw: "Released opaque-run-plan-tag-1/Stage1/日本語/00001234",
            id: "repo-workflows-release-release-stopped-1904d12e",
            args: &[(
                "run-plan-tag-1",
                Sample::Text(
                    "opaque-run-plan-tag-1/Stage1/日本語/00001234",
                    "opaque-run-plan-tag-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        293,
        Case {
            raw: "Push: stopped",
            id: "repo-workflows-release-release-stopped-328c7ab5",
            args: &[("label", Sample::Text("Push", "Pushen"))],
        },
    ),
];
