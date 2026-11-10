use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        76,
        Case {
            raw: "A parallel bisect is already running in this tab",
            id: "repo-workflows-parallel_bisect-start-parallel-3450e92d",
            args: &[],
        },
    ),
    (
        92,
        Case {
            raw: "Wait for the operation going on to finish",
            id: "repo-workflows-parallel_bisect-mark-parallel-result-e5e3ecc5",
            args: &[],
        },
    ),
    (
        155,
        Case {
            raw: "Mark in the bisect failed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-parallel_bisect-mark-parallel-result-48f3588b",
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
        207,
        Case {
            raw: "Marked in the bisect",
            id: "repo-workflows-parallel_bisect-mark-parallel-result-e05d54d7",
            args: &[],
        },
    ),
];
