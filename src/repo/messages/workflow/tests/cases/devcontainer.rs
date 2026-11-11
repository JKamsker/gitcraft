use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        131,
        Case {
            raw: "Cancelled before terminal start",
            id: "repo-workflows-devcontainer-terminal-f171832a",
            args: &[],
        },
    ),
    (
        165,
        Case {
            raw: "Cancelled before starting",
            id: "repo-workflows-devcontainer-devcontainer-act-84f8f92f",
            args: &[],
        },
    ),
    (
        179,
        Case {
            raw: "Dev container operation",
            id: "repo-workflows-devcontainer-devcontainer-act-519a489a",
            args: &[],
        },
    ),
    (
        254,
        Case {
            raw: "Container opaque-binding-id-12-1/Stage1/日本語/00001234 · opaque-binding-folder-2/Stage1/日本語/00001234",
            id: "repo-workflows-devcontainer-terminal-77e417ed",
            args: &[
                (
                    "binding-id-12-1",
                    Sample::Text(
                        "opaque-binding-id-12-1/Stage1/日本語/00001234",
                        "opaque-binding-id-12-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "binding-folder-2",
                    Sample::Text(
                        "opaque-binding-folder-2/Stage1/日本語/00001234",
                        "opaque-binding-folder-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
];
