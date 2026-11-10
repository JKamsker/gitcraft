use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        75,
        Case {
            raw: "Tracker account changed; reload the issue source",
            id: "repo-workflows-issue_work-start-issue-563140a4",
            args: &[],
        },
    ),
    (
        110,
        Case {
            raw: "Couldn't read the branches' issues: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-issue_work-take-issue-read-bbcbbb59",
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
        143,
        Case {
            raw: "Forget the issue of opaque-branch/Stage1/日本語/00001234 failed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-issue_work-unlink-issue-6df1900e",
            args: &[
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
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
        169,
        Case {
            raw: "Start work on opaque-issue-key-1/Stage1/日本語/00001234: wait for Push",
            id: "repo-workflows-issue_work-start-issue-e2b80c1b",
            args: &[
                (
                    "issue-key-1",
                    Sample::Text(
                        "opaque-issue-key-1/Stage1/日本語/00001234",
                        "opaque-issue-key-1/Stage1/日本語/00001234",
                    ),
                ),
                ("busy", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
    (
        237,
        Case {
            raw: "Start work on opaque-issue-key-1/Stage1/日本語/00001234: opaque-why/Stage1/日本語/00001234",
            id: "repo-workflows-issue_work-start-issue-c14dd822",
            args: &[
                (
                    "issue-key-1",
                    Sample::Text(
                        "opaque-issue-key-1/Stage1/日本語/00001234",
                        "opaque-issue-key-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "why",
                    Sample::Text(
                        "opaque-why/Stage1/日本語/00001234",
                        "opaque-why/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        275,
        Case {
            raw: "Push: wait for Push",
            id: "repo-workflows-issue_work-refresh-issue-a0e9ad2a",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                ("busy", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
];
