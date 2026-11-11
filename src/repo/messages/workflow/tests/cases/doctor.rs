use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        18,
        Case {
            raw: "Clear the flag of raw/path:1234",
            id: "repo-workflows-doctor-clear-7f76d2cf",
            args: &[("one-path-1", Sample::Text("raw/path:1234", "raw/path:1234"))],
        },
    ),
    (
        19,
        Case {
            raw: "Clear the flags of 1234 files",
            id: "repo-workflows-doctor-clear-e318945b",
            args: &[("files-len-1", Sample::Number(1234))],
        },
    ),
    (
        51,
        Case {
            raw: "Finish or close the conflict editor or the interactive rebase first",
            id: "repo-workflows-doctor-open-doctor-745454bd",
            args: &[],
        },
    ),
    (
        105,
        Case {
            raw: "Wait for the running operation to end",
            id: "repo-workflows-doctor-doctor-act-9bab8d95",
            args: &[],
        },
    ),
    (
        226,
        Case {
            raw: "Remove opaque-origin-key-1/Stage1/日本語/00001234 (opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234) from opaque-file/Stage1/日本語/00001234",
            id: "repo-workflows-doctor-remove-confirm-b0b26eef",
            args: &[
                (
                    "origin-key-1",
                    Sample::Text(
                        "opaque-origin-key-1/Stage1/日本語/00001234",
                        "opaque-origin-key-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "shortened-origin-shown-value-2",
                    Sample::Text(
                        "opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234",
                        "opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "file",
                    Sample::Text(
                        "opaque-file/Stage1/日本語/00001234",
                        "opaque-file/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        255,
        Case {
            raw: "Set opaque-setting-key-1/Stage1/日本語/00001234 to opaque-shortened-value-2/Stage1/日本語/00001234 for this repository",
            id: "repo-workflows-doctor-set-row-921f564b",
            args: &[
                (
                    "setting-key-1",
                    Sample::Text(
                        "opaque-setting-key-1/Stage1/日本語/00001234",
                        "opaque-setting-key-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "shortened-value-2",
                    Sample::Text(
                        "opaque-shortened-value-2/Stage1/日本語/00001234",
                        "opaque-shortened-value-2/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "scope-choice-scope-to-lowercase-3",
                    Sample::Text("This repository", "Dieses Repository"),
                ),
            ],
        },
    ),
    (
        290,
        Case {
            raw: "Turn on: commit-graph",
            id: "repo-workflows-doctor-turn-on-action-d8e3e02d",
            args: &[(
                "s-title-to-lowercase-1",
                Sample::Text("Commit-graph", "Commit-Graph"),
            )],
        },
    ),
];
