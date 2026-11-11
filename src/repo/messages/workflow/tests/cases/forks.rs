use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        26,
        Case {
            raw: "Approving runs the contributor's code on your CI: look through #opaque-number/Stage1/日本語/00001234's changes first, then choose Approve and run again",
            id: "repo-workflows-forks-approve-runs-a77eca22",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        33,
        Case {
            raw: "The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it (or abort it)",
            id: "repo-workflows-forks-forks-op-done-9c338e90",
            args: &[],
        },
    ),
    (
        49,
        Case {
            raw: "opaque-plan-local-1/Stage1/日本語/00001234 is rebased here, but not pushed to opaque-plan-fork-2/Stage1/日本語/00001234: Ctrl+Z puts it back as it was",
            id: "repo-workflows-forks-forks-op-done-8287ad6b",
            args: &[
                (
                    "plan-local-1",
                    Sample::Text(
                        "opaque-plan-local-1/Stage1/日本語/00001234",
                        "opaque-plan-local-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "plan-fork-2",
                    Sample::Text(
                        "opaque-plan-fork-2/Stage1/日本語/00001234",
                        "opaque-plan-fork-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        84,
        Case {
            raw: "This repository has no remote to rebase onto",
            id: "repo-workflows-forks-plan-fork-rebase-1abac341",
            args: &[],
        },
    ),
    (
        107,
        Case {
            raw: "Couldn't approve #opaque-number/Stage1/日本語/00001234's workflow runs: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-forks-approve-runs-282082c7",
            args: &[
                (
                    "number",
                    Sample::Text(
                        "opaque-number/Stage1/日本語/00001234",
                        "opaque-number/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "e",
                    Sample::Text(
                        "opaque-e/Stage1/日本語/00001234",
                        "opaque-e/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        114,
        Case {
            raw: "Where #opaque-number/Stage1/日本語/00001234 comes from isn't known yet",
            id: "repo-workflows-forks-checkout-fork-5ecb3bac",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        117,
        Case {
            raw: "Fetching #opaque-number/Stage1/日本語/00001234's branch and its base…",
            id: "repo-workflows-forks-plan-fork-rebase-ac71056e",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        133,
        Case {
            raw: "#opaque-number/Stage1/日本語/00001234's workflow runs are approved",
            id: "repo-workflows-forks-approve-runs-a151ad19",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        142,
        Case {
            raw: "Approving #opaque-number/Stage1/日本語/00001234's workflow runs…",
            id: "repo-workflows-forks-approve-runs-2f659b28",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        154,
        Case {
            raw: "Couldn't read the remotes: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-forks-plan-tidy-adf73b14",
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
        161,
        Case {
            raw: "Remove unused fork remotes",
            id: "repo-workflows-forks-plan-tidy-a7e148ae",
            args: &[],
        },
    ),
    (
        331,
        Case {
            raw: "Check out #1234 to push to raw-owner/ß",
            id: "repo-workflows-forks-checkout-fork-abaaf4c3",
            args: &[
                ("number", Sample::Text("1234", "1234")),
                ("info-owner-1", Sample::Text("raw-owner/ß", "raw-owner/ß")),
            ],
        },
    ),
];
