use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        62,
        Case {
            raw: "UnityYAMLMerge wasn't found: choose it in Unity set-up…",
            id: "repo-workflows-unity-smart-merge-conflict-5a10252d",
            args: &[],
        },
    ),
    (
        68,
        Case {
            raw: "UnityYAMLMerge merged opaque-path/Stage1/日本語/00001234: check the result, then save",
            id: "repo-workflows-unity-smart-merge-done-afe126d0",
            args: &[(
                "path",
                Sample::Text(
                    "opaque-path/Stage1/日本語/00001234",
                    "opaque-path/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        85,
        Case {
            raw: "UnityYAMLMerge wasn't found: choose it first",
            id: "repo-workflows-unity-register-driver-f6c55b48",
            args: &[],
        },
    ),
    (
        86,
        Case {
            raw: "UnityYAMLMerge left 1234 conflicts in opaque-path/Stage1/日本語/00001234 to choose",
            id: "repo-workflows-unity-smart-merge-done-7e870a3e",
            args: &[
                ("n", Sample::Number(1234)),
                (
                    "path",
                    Sample::Text(
                        "opaque-path/Stage1/日本語/00001234",
                        "opaque-path/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        116,
        Case {
            raw: "Taking generated folders out of git",
            id: "repo-workflows-unity-untrack-generated-b0c9a9c5",
            args: &[],
        },
    ),
    (
        136,
        Case {
            raw: "Adding lines to .gitattributes",
            id: "repo-workflows-unity-add-attributes-0b9b6295",
            args: &[],
        },
    ),
    (
        157,
        Case {
            raw: "Adding lines to .gitignore",
            id: "repo-workflows-unity-add-ignores-52181c32",
            args: &[],
        },
    ),
    (
        162,
        Case {
            raw: "Registering UnityYAMLMerge",
            id: "repo-workflows-unity-register-driver-5d4e5077",
            args: &[],
        },
    ),
    (
        204,
        Case {
            raw: "opaque-file-display-1/Stage1/日本語/00001234 isn't UnityYAMLMerge",
            id: "repo-workflows-unity-choose-tool-af82d6a2",
            args: &[(
                "file-display-1",
                Sample::Text(
                    "opaque-file-display-1/Stage1/日本語/00001234",
                    "opaque-file-display-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        231,
        Case {
            raw: "Saving the choice",
            id: "repo-workflows-unity-choose-tool-34706d9f",
            args: &[],
        },
    ),
];
