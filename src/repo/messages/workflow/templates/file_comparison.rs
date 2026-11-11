pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        8,
        (
            "(?s)^Repository changed\\. Refresh the file view\\.$",
            "repo-workflows-file_comparison-invalidate-81f92ce0",
            &[],
        ),
    ),
    (
        9,
        (
            "(?s)^Invalid comparison sides$",
            "repo-workflows-file_comparison-prepare-da3978aa",
            &[],
        ),
    ),
    (
        10,
        (
            "(?s)^More than 20,000 lines in a version; use the normal patch view$",
            "repo-workflows-file_comparison-boundary-e44610d7",
            &[],
        ),
    ),
    (
        11,
        (
            "(?s)^Git could not align these raw versions$",
            "repo-workflows-file_comparison-boundary-b861cbb0",
            &[],
        ),
    ),
    (
        12,
        (
            "(?s)^The raw patch is not UTF\\-8$",
            "repo-workflows-file_comparison-boundary-5421cc1b",
            &[],
        ),
    ),
    (
        13,
        (
            "(?s)^Git could not produce a raw text patch for these differing bytes; use the normal patch view$",
            "repo-workflows-file_comparison-boundary-6732715f",
            &[],
        ),
    ),
    (
        14,
        (
            "(?s)^Comparison cancelled\\. Refresh to try again\\.$",
            "repo-workflows-file_comparison-file-comparison-view-725513de",
            &[],
        ),
    ),
    (
        64,
        (
            "(?s)^Moved\\-code analysis cancelled or exceeded 40 seconds$",
            "repo-workflows-file_comparison-read-431f5828",
            &[],
        ),
    ),
    (
        126,
        (
            "(?s)^Moved\\-code comparison cancelled$",
            "repo-workflows-file_comparison-assemble-56abde67",
            &[],
        ),
    ),
    (
        127,
        (
            "(?s)^Finish or close the editor first$",
            "repo-workflows-file_comparison-open-file-comparison-a6cb5dc1",
            &[],
        ),
    ),
    (
        215,
        (
            "(?s)^(.*?) (.*?) from staging lens$",
            "repo-workflows-file_comparison-comparison-pick-28dc27ab",
            &[("pick-verb-1", false), ("snapshot-spec-path-2", false)],
        ),
    ),
];
