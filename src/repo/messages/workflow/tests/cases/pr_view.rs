use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        79,
        Case {
            raw: "The pull request's branch is not available yet",
            id: "repo-workflows-pr_view-select-pr-b788526c",
            args: &[],
        },
    ),
    (
        208,
        Case {
            raw: "Comment relocation: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-pr_view-pr-files-arrived-20be6481",
            args: &[(
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
