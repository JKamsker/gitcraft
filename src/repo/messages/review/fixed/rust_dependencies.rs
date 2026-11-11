pub(super) const ROWS: &[(usize, (&str, &str))] = &[
    (
        122,
        (
            "Non-UTF-8 or newline-containing Rust paths were not analyzed",
            "repo-workflows-review-raw-non-utf-8-or-newline-containing-rust-paths-wer-9e8a8d",
        ),
    ),
    (
        123,
        (
            "More than 4096 Rust files; dependency analysis was not performed",
            "repo-workflows-review-raw-more-than-4096-rust-files-dependency-analysis-8964c0",
        ),
    ),
    (
        124,
        (
            "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
            "repo-workflows-review-raw-rust-sources-exceed-the-32mib-analysis-budget-2f47c3",
        ),
    ),
    (
        125,
        (
            "Incomplete Rust blob header",
            "repo-workflows-review-raw-incomplete-rust-blob-header-a8a0a0",
        ),
    ),
    (
        126,
        (
            "Invalid Rust blob size",
            "repo-workflows-review-raw-invalid-rust-blob-size-7071a5",
        ),
    ),
    (
        127,
        (
            "A Rust source is not a blob",
            "repo-workflows-review-raw-a-rust-source-is-not-a-blob-5f8d45",
        ),
    ),
    (
        128,
        (
            "Incomplete Rust blob",
            "repo-workflows-review-raw-incomplete-rust-blob-7a9492",
        ),
    ),
    (
        129,
        (
            "Incomplete Rust blob separator",
            "repo-workflows-review-raw-incomplete-rust-blob-separator-e86a4e",
        ),
    ),
    (
        130,
        (
            "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
            "repo-workflows-review-raw-rust-syntax-evidence-does-not-resolve-cfg-feat-2c702b",
        ),
    ),
    (
        131,
        (
            "Static call traversal exceeded 1024 symbols",
            "repo-workflows-review-raw-static-call-traversal-exceeded-1024-symbols-64479e",
        ),
    ),
    (
        132,
        (
            "Dynamic, external, local-import, or macro-generated calls remain unchecked",
            "repo-workflows-review-raw-dynamic-external-local-import-or-macro-generat-5d8937",
        ),
    ),
    (
        133,
        (
            "Duplicate/cfg-dependent symbol identities were not resolved",
            "repo-workflows-review-raw-duplicate-cfg-dependent-symbol-identities-were-6207ad",
        ),
    ),
    (
        134,
        (
            "No unchanged uniquely identified Rust functions were analyzed",
            "repo-workflows-review-raw-no-unchanged-uniquely-identified-rust-function-bd0023",
        ),
    ),
];
