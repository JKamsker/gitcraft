pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        5,
        (
            "(?s)^No supported forge remote; commit history remains available$",
            "repo-workflows-line_dossier-poll-line-dossier-45e16671",
            &[],
        ),
    ),
    (
        6,
        (
            "(?s)^Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history$",
            "repo-workflows-line_dossier-poll-line-dossier-1cd553a8",
            &[],
        ),
    ),
    (
        7,
        (
            "(?s)^Trace cancelled$",
            "repo-workflows-line_dossier-line-dossier-view-a8198076",
            &[],
        ),
    ),
    (
        21,
        (
            "(?s)^Revision appended to \\.git\\-blame\\-ignore\\-revs; staged contents preserved\\. Ordinary Git blame can use blame\\.ignoreRevsFile=\\.git\\-blame\\-ignore\\-revs\\.$",
            "repo-workflows-line_dossier-dossier-ignore-finished-6e1731ff",
            &[],
        ),
    ),
    (
        59,
        (
            "(?s)^Revision already present; ignore file and index unchanged$",
            "repo-workflows-line_dossier-dossier-ignore-finished-366a809a",
            &[],
        ),
    ),
    (
        63,
        (
            "(?s)^History summary cancelled or exceeded its output limit$",
            "repo-workflows-line_dossier-run-a9f1b338",
            &[],
        ),
    ),
    (
        118,
        (
            "(?s)^Appending reviewed ignore revision$",
            "repo-workflows-line_dossier-append-dossier-ignore-716419f1",
            &[],
        ),
    ),
    (
        229,
        (
            "(?s)^Ignore revision: (.*?)$",
            "repo-workflows-line_dossier-dossier-ignore-finished-4c50387c",
            &[("error", false)],
        ),
    ),
    (
        230,
        (
            "(?s)^Summary cancelled$",
            "repo-workflows-line_dossier-run-ae6f0623",
            &[],
        ),
    ),
];
