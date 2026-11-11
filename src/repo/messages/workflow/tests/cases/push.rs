use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        67,
        Case {
            raw: "Push refused, and the remote could not be checked: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-push-remote-checked-c6e528c9",
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
        97,
        Case {
            raw: "This repository has no remote to push to",
            id: "repo-workflows-push-push-labelled-4933a6c9",
            args: &[],
        },
    ),
    (
        129,
        Case {
            raw: "Push failed, so nothing was pushed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-push-chain-step-ended-dfa32698",
            args: &[
                ("what", Sample::Text("Push", "Pushen")),
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
        175,
        Case {
            raw: "Sync: opaque-current/Stage1/日本語/00001234 pulled and pushed",
            id: "repo-workflows-push-sync-027fe2bd",
            args: &[(
                "current",
                Sample::Text(
                    "opaque-current/Stage1/日本語/00001234",
                    "opaque-current/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        176,
        Case {
            raw: "opaque-r-branch-1/Stage1/日本語/00001234 pulled from opaque-r-target-label-2/Stage1/日本語/00001234 and pushed",
            id: "repo-workflows-push-push-act-ebd6e3a6",
            args: &[
                (
                    "r-branch-1",
                    Sample::Text(
                        "opaque-r-branch-1/Stage1/日本語/00001234",
                        "opaque-r-branch-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "r-target-label-2",
                    Sample::Text(
                        "opaque-r-target-label-2/Stage1/日本語/00001234",
                        "opaque-r-target-label-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        192,
        Case {
            raw: "Push refused: opaque-target-label-1/Stage1/日本語/00001234 is gone",
            id: "repo-workflows-push-remote-checked-205a41fa",
            args: &[(
                "target-label-1",
                Sample::Text(
                    "opaque-target-label-1/Stage1/日本語/00001234",
                    "opaque-target-label-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        221,
        Case {
            raw: "Sync: opaque-current/Stage1/日本語/00001234 published (it had nothing to pull)",
            id: "repo-workflows-push-sync-5ca83b4b",
            args: &[(
                "current",
                Sample::Text(
                    "opaque-current/Stage1/日本語/00001234",
                    "opaque-current/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        246,
        Case {
            raw: "Push refused: origin/00001234 has 1234 commits you don't have",
            id: "repo-workflows-push-remote-checked-3bbec358",
            args: &[(
                "banner-headline-r-1",
                Sample::Text(
                    "origin/00001234 has 1,234 commits you don't have",
                    "origin/00001234 enthält 1.234 Commits, die Ihnen fehlen",
                ),
            )],
        },
    ),
    (
        285,
        Case {
            raw: "Push refused: opaque-out-trim-1/Stage1/日本語/00001234",
            id: "repo-workflows-push-push-op-ended-79357f2e",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                (
                    "out-trim-1",
                    Sample::Text(
                        "opaque-out-trim-1/Stage1/日本語/00001234",
                        "opaque-out-trim-1/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        291,
        Case {
            raw: "Checking opaque-what/Stage1/日本語/00001234",
            id: "repo-workflows-push-check-remote-9a7c53eb",
            args: &[(
                "what",
                Sample::Text(
                    "opaque-what/Stage1/日本語/00001234",
                    "opaque-what/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
