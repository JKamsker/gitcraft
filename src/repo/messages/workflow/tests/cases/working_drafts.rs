use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        223,
        Case {
            raw: "Earlier versions: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-working_drafts-poll-working-drafts-7a49e8a4",
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
        249,
        Case {
            raw: "Saved manually",
            id: "repo-workflows-working_drafts-save-working-draft-3e48426a",
            args: &[],
        },
    ),
    (
        260,
        Case {
            raw: "Working draft",
            id: "repo-workflows-working_drafts-change-working-draft-278865c0",
            args: &[],
        },
    ),
];
