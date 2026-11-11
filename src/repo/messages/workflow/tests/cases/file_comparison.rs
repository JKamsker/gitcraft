use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        8,
        Case {
            raw: "Repository changed. Refresh the file view.",
            id: "repo-workflows-file_comparison-invalidate-81f92ce0",
            args: &[],
        },
    ),
    (
        9,
        Case {
            raw: "Invalid comparison sides",
            id: "repo-workflows-file_comparison-prepare-da3978aa",
            args: &[],
        },
    ),
    (
        10,
        Case {
            raw: "More than 20,000 lines in a version; use the normal patch view",
            id: "repo-workflows-file_comparison-boundary-e44610d7",
            args: &[],
        },
    ),
    (
        11,
        Case {
            raw: "Git could not align these raw versions",
            id: "repo-workflows-file_comparison-boundary-b861cbb0",
            args: &[],
        },
    ),
    (
        12,
        Case {
            raw: "The raw patch is not UTF-8",
            id: "repo-workflows-file_comparison-boundary-5421cc1b",
            args: &[],
        },
    ),
    (
        13,
        Case {
            raw: "Git could not produce a raw text patch for these differing bytes; use the normal patch view",
            id: "repo-workflows-file_comparison-boundary-6732715f",
            args: &[],
        },
    ),
    (
        14,
        Case {
            raw: "Comparison cancelled. Refresh to try again.",
            id: "repo-workflows-file_comparison-file-comparison-view-725513de",
            args: &[],
        },
    ),
    (
        64,
        Case {
            raw: "Moved-code analysis cancelled or exceeded 40 seconds",
            id: "repo-workflows-file_comparison-read-431f5828",
            args: &[],
        },
    ),
    (
        126,
        Case {
            raw: "Moved-code comparison cancelled",
            id: "repo-workflows-file_comparison-assemble-56abde67",
            args: &[],
        },
    ),
    (
        127,
        Case {
            raw: "Finish or close the editor first",
            id: "repo-workflows-file_comparison-open-file-comparison-a6cb5dc1",
            args: &[],
        },
    ),
    (
        215,
        Case {
            raw: "Stage opaque-snapshot-spec-path-2/Stage1/日本語/00001234 from staging lens",
            id: "repo-workflows-file_comparison-comparison-pick-28dc27ab",
            args: &[
                ("pick-verb-1", Sample::Text("Stage", "Vormerken")),
                (
                    "snapshot-spec-path-2",
                    Sample::Text(
                        "opaque-snapshot-spec-path-2/Stage1/日本語/00001234",
                        "opaque-snapshot-spec-path-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
];
