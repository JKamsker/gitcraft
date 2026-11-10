use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        5,
        Case {
            raw: "No supported forge remote; commit history remains available",
            id: "repo-workflows-line_dossier-poll-line-dossier-45e16671",
            args: &[],
        },
    ),
    (
        6,
        Case {
            raw: "Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history",
            id: "repo-workflows-line_dossier-poll-line-dossier-1cd553a8",
            args: &[],
        },
    ),
    (
        7,
        Case {
            raw: "Trace cancelled",
            id: "repo-workflows-line_dossier-line-dossier-view-a8198076",
            args: &[],
        },
    ),
    (
        21,
        Case {
            raw: "Revision appended to .git-blame-ignore-revs; staged contents preserved. Ordinary Git blame can use blame.ignoreRevsFile=.git-blame-ignore-revs.",
            id: "repo-workflows-line_dossier-dossier-ignore-finished-6e1731ff",
            args: &[],
        },
    ),
    (
        59,
        Case {
            raw: "Revision already present; ignore file and index unchanged",
            id: "repo-workflows-line_dossier-dossier-ignore-finished-366a809a",
            args: &[],
        },
    ),
    (
        63,
        Case {
            raw: "History summary cancelled or exceeded its output limit",
            id: "repo-workflows-line_dossier-run-a9f1b338",
            args: &[],
        },
    ),
    (
        118,
        Case {
            raw: "Appending reviewed ignore revision",
            id: "repo-workflows-line_dossier-append-dossier-ignore-716419f1",
            args: &[],
        },
    ),
    (
        229,
        Case {
            raw: "Ignore revision: opaque-error/Stage1/日本語/00001234",
            id: "repo-workflows-line_dossier-dossier-ignore-finished-4c50387c",
            args: &[(
                "error",
                Sample::Text(
                    "opaque-error/Stage1/日本語/00001234",
                    "opaque-error/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        230,
        Case {
            raw: "Summary cancelled",
            id: "repo-workflows-line_dossier-run-ae6f0623",
            args: &[],
        },
    ),
];
