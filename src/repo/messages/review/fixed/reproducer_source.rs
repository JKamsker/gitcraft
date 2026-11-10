pub(super) const ROWS: &[(usize, (&str, &str))] = &[
    (
        104,
        (
            "Source export cancelled",
            "repo-workflows-review-raw-source-export-cancelled-49ca7d",
        ),
    ),
    (
        105,
        (
            "Raw source export exceeded its 60s preparation budget",
            "repo-workflows-review-raw-raw-source-export-exceeded-its-60s-preparation-4e4dc2",
        ),
    ),
    (
        106,
        (
            "Sandbox source exceeds 20,000 files or 128MiB",
            "repo-workflows-review-raw-sandbox-source-exceeds-20-000-files-or-128mib-9f7142",
        ),
    ),
    (
        107,
        (
            "Submodule nesting exceeds the eight-level source-export limit",
            "repo-workflows-review-raw-submodule-nesting-exceeds-the-eight-level-sour-148cdc",
        ),
    ),
    (
        108,
        (
            "Malformed raw tree record",
            "repo-workflows-review-raw-malformed-raw-tree-record-6ab124",
        ),
    ),
    (
        109,
        (
            "Invalid raw tree metadata",
            "repo-workflows-review-raw-invalid-raw-tree-metadata-6961a8",
        ),
    ),
    (
        110,
        (
            "Malformed raw tree metadata",
            "repo-workflows-review-raw-malformed-raw-tree-metadata-9cd416",
        ),
    ),
    (
        111,
        (
            "Raw tree contains an invalid object ID",
            "repo-workflows-review-raw-raw-tree-contains-an-invalid-object-id-b7eee8",
        ),
    ),
    (
        112,
        (
            "Sandbox source paths must be UTF-8",
            "repo-workflows-review-raw-sandbox-source-paths-must-be-utf-8-122183",
        ),
    ),
    (
        113,
        (
            "Unsupported raw tree object mode",
            "repo-workflows-review-raw-unsupported-raw-tree-object-mode-28f35d",
        ),
    ),
    (
        114,
        (
            "Invalid raw blob size",
            "repo-workflows-review-raw-invalid-raw-blob-size-6ea4b1",
        ),
    ),
    (
        115,
        (
            "Sandbox source exceeds 16MiB per blob or 128MiB total",
            "repo-workflows-review-raw-sandbox-source-exceeds-16mib-per-blob-or-128mi-fe4de2",
        ),
    ),
    (
        116,
        (
            "Raw blob size changed or output was truncated",
            "repo-workflows-review-raw-raw-blob-size-changed-or-output-was-truncated-a205c5",
        ),
    ),
    (
        117,
        (
            "Duplicate raw source path",
            "repo-workflows-review-raw-duplicate-raw-source-path-c61f88",
        ),
    ),
    (
        118,
        (
            "Sandbox symlink targets must be UTF-8",
            "repo-workflows-review-raw-sandbox-symlink-targets-must-be-utf-8-82700d",
        ),
    ),
    (
        119,
        (
            "Symlink points into Git metadata",
            "repo-workflows-review-raw-symlink-points-into-git-metadata-d47822",
        ),
    ),
    (
        120,
        (
            "Archive field overflow",
            "repo-workflows-review-raw-archive-field-overflow-a70fa1",
        ),
    ),
    (
        121,
        (
            "Archive header requires a PAX path record",
            "repo-workflows-review-raw-archive-header-requires-a-pax-path-record-c9450c",
        ),
    ),
];
