use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        50,
        Case {
            raw: "The last scan is still ending on that machine; try again in a moment",
            id: "repo-workflows-reflog-start-scan-7a1d64fe",
            args: &[],
        },
    ),
    (
        270,
        Case {
            raw: "Apply stash opaque-short-pick-hash-1/Stage1/日本語/00001234",
            id: "repo-workflows-reflog-apply-stash-93a6ce40",
            args: &[(
                "short-pick-hash-1",
                Sample::Text(
                    "opaque-short-pick-hash-1/Stage1/日本語/00001234",
                    "opaque-short-pick-hash-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
