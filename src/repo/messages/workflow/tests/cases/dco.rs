use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        74,
        Case {
            raw: "Couldn't save Sign off for this repository (git config opaque-signoff-sign-off-key-1/Stage1/日本語/00001234): opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-dco-dco-act-10fe4a7c",
            args: &[
                (
                    "signoff-sign-off-key-1",
                    Sample::Text(
                        "opaque-signoff-sign-off-key-1/Stage1/日本語/00001234",
                        "opaque-signoff-sign-off-key-1/Stage1/日本語/00001234",
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
        124,
        Case {
            raw: "Push the signed-off commits of opaque-plan-branch-1/Stage1/日本語/00001234",
            id: "repo-workflows-dco-dco-op-done-55d2b32b",
            args: &[(
                "plan-branch-1",
                Sample::Text(
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                    "opaque-plan-branch-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
