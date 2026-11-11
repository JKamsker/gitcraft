use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        53,
        Case {
            raw: "Distribute staged fixes: HEAD is detached, check out a branch first",
            id: "repo-workflows-surgery-open-absorb-3373a6f7",
            args: &[],
        },
    ),
    (
        54,
        Case {
            raw: "Distribute staged fixes: nothing is staged, stage the fixes first",
            id: "repo-workflows-surgery-open-absorb-55d2e258",
            args: &[],
        },
    ),
    (
        57,
        Case {
            raw: "Distribute staged fixes: close the editor that is open first",
            id: "repo-workflows-surgery-open-absorb-2cddd12d",
            args: &[],
        },
    ),
    (
        61,
        Case {
            raw: "Distribute staged fixes: another editor opened meanwhile",
            id: "repo-workflows-surgery-open-absorb-55f28458",
            args: &[],
        },
    ),
    (
        72,
        Case {
            raw: "Split commit: close the staged fixes editor first",
            id: "repo-workflows-surgery-open-split-01ee6f3c",
            args: &[],
        },
    ),
    (
        87,
        Case {
            raw: "Distribute staged fixes: a merge is in progress",
            id: "repo-workflows-surgery-open-absorb-edcc444b",
            args: &[(
                "state-label-to-lowercase-1",
                Sample::Text("Merge", "Zusammenführen"),
            )],
        },
    ),
    (
        94,
        Case {
            raw: "Move changes from opaque-short-hash-1/Stage1/日本語/00001234 to the working changes",
            id: "repo-workflows-surgery-run-move-afb8b0ad",
            args: &[(
                "short-hash-1",
                Sample::Text(
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        123,
        Case {
            raw: "Distribute staged fixes: reading…",
            id: "repo-workflows-surgery-open-absorb-5ebbefda",
            args: &[],
        },
    ),
    (
        171,
        Case {
            raw: "Distribute staged fixes: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-surgery-open-absorb-ed3b6cd2",
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
        183,
        Case {
            raw: "Move changes: reading opaque-short-hash-1/Stage1/日本語/00001234…",
            id: "repo-workflows-surgery-open-move-9a3c16a9",
            args: &[(
                "short-hash-1",
                Sample::Text(
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        184,
        Case {
            raw: "Split commit: reading opaque-short-hash-1/Stage1/日本語/00001234…",
            id: "repo-workflows-surgery-open-split-a472a361",
            args: &[(
                "short-hash-1",
                Sample::Text(
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        196,
        Case {
            raw: "Move changes from opaque-short-hash-1/Stage1/日本語/00001234 to opaque-short-t-hash-2/Stage1/日本語/00001234",
            id: "repo-workflows-surgery-run-move-b5726a1d",
            args: &[
                (
                    "short-hash-1",
                    Sample::Text(
                        "opaque-short-hash-1/Stage1/日本語/00001234",
                        "opaque-short-hash-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "short-t-hash-2",
                    Sample::Text(
                        "opaque-short-t-hash-2/Stage1/日本語/00001234",
                        "opaque-short-t-hash-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        247,
        Case {
            raw: "Move changes: opaque-why/Stage1/日本語/00001234",
            id: "repo-workflows-surgery-open-move-9a352cd5",
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
        248,
        Case {
            raw: "Split commit: opaque-why/Stage1/日本語/00001234",
            id: "repo-workflows-surgery-open-split-ad93af6b",
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
        266,
        Case {
            raw: "Split opaque-short-hash-1/Stage1/日本語/00001234 into 1234",
            id: "repo-workflows-surgery-run-split-ddd763c5",
            args: &[
                (
                    "short-hash-1",
                    Sample::Text(
                        "opaque-short-hash-1/Stage1/日本語/00001234",
                        "opaque-short-hash-1/Stage1/日本語/00001234",
                    ),
                ),
                ("messages-len-2", Sample::Number(1234)),
            ],
        },
    ),
];
