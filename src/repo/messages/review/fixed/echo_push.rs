pub(super) const ROWS: &[(usize, (&str, &str))] = &[
    (
        43,
        (
            "Reviewed push already has execution evidence; resume its retained operation",
            "repo-workflows-review-raw-reviewed-push-already-has-execution-evidence-r-85ecb3",
        ),
    ),
    (
        44,
        (
            "Push destination is not unambiguous; review its source manually",
            "repo-workflows-review-raw-push-destination-is-not-unambiguous-review-its-64ee34",
        ),
    ),
    (
        45,
        (
            "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
            "repo-workflows-review-raw-this-operation-changes-state-before-pushing-ru-716a82",
        ),
    ),
    (
        46,
        (
            "Invalid wildcard source",
            "repo-workflows-review-raw-invalid-wildcard-source-9e223c",
        ),
    ),
    (
        47,
        (
            "Unsupported push wildcard; review it explicitly",
            "repo-workflows-review-raw-unsupported-push-wildcard-review-it-explicitly-71983a",
        ),
    ),
    (
        48,
        (
            "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
            "repo-workflows-review-raw-reviewer-echo-cannot-inspect-context-changing-928514",
        ),
    ),
];
