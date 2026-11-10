use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        218,
        Case {
            raw: "Content recovery: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-content_recovery-content-perform-9d57ebb2",
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
        238,
        Case {
            raw: "Recover content",
            id: "repo-workflows-content_recovery-content-perform-fbdf23f8",
            args: &[],
        },
    ),
];
