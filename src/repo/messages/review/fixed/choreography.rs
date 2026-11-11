pub(super) const ROWS: &[(usize, (&str, &str))] = &[
    (
        0,
        (
            "Add 2–6 repositories to the release board",
            "repo-workflows-review-raw-add-2-6-repositories-to-the-release-board-4353ee",
        ),
    ),
    (
        1,
        (
            "Component names must be unique ASCII identifiers, ignoring case",
            "repo-workflows-review-raw-component-names-must-be-unique-ascii-identifie-fde8d3",
        ),
    ),
    (
        2,
        (
            "Use absolute repository paths or encoded SSH/WSL paths",
            "repo-workflows-review-raw-use-absolute-repository-paths-or-encoded-ssh-w-aef508",
        ),
    ),
    (
        3,
        (
            "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
            "repo-workflows-review-raw-rehearse-linked-repositories-on-the-same-machi-c515cc",
        ),
    ),
    (
        4,
        (
            "Too many combinations",
            "repo-workflows-review-raw-too-many-combinations-b95a07",
        ),
    ),
    (
        5,
        (
            "More than 256 in-between combinations: split this release into smaller boards",
            "repo-workflows-review-raw-more-than-256-in-between-combinations-split-th-3b3334",
        ),
    ),
    (
        6,
        (
            "A rule names an unknown dependent component",
            "repo-workflows-review-raw-a-rule-names-an-unknown-dependent-component-412c9a",
        ),
    ),
    (
        7,
        (
            "A rule names an unknown prerequisite component",
            "repo-workflows-review-raw-a-rule-names-an-unknown-prerequisite-component-4ae079",
        ),
    ),
    (
        8,
        (
            "Rule stage thresholds must name non-deployed stages present on their components",
            "repo-workflows-review-raw-rule-stage-thresholds-must-name-non-deployed-s-aff755",
        ),
    ),
    (
        9,
        (
            "Stopped before constructing the full combination",
            "repo-workflows-review-raw-stopped-before-constructing-the-full-combinati-a61d6f",
        ),
    ),
    (
        10,
        (
            "Enter and explicitly trust your compatibility command before rehearsal",
            "repo-workflows-review-raw-enter-and-explicitly-trust-your-compatibility-691732",
        ),
    ),
    (
        11,
        (
            "Not tested: rehearsal stopped",
            "repo-workflows-review-raw-not-tested-rehearsal-stopped-212f11",
        ),
    ),
];
