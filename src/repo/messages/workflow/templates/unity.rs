pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        62,
        (
            "(?s)^UnityYAMLMerge wasn't found: choose it in Unity set\\-up…$",
            "repo-workflows-unity-smart-merge-conflict-5a10252d",
            &[],
        ),
    ),
    (
        68,
        (
            "(?s)^UnityYAMLMerge merged (.*?): check the result, then save$",
            "repo-workflows-unity-smart-merge-done-afe126d0",
            &[("path", false)],
        ),
    ),
    (
        85,
        (
            "(?s)^UnityYAMLMerge wasn't found: choose it first$",
            "repo-workflows-unity-register-driver-f6c55b48",
            &[],
        ),
    ),
    (
        86,
        (
            "(?s)^UnityYAMLMerge left ([0-9]+) conflicts in (.*?) to choose$",
            "repo-workflows-unity-smart-merge-done-7e870a3e",
            &[("n", true), ("path", false)],
        ),
    ),
    (
        116,
        (
            "(?s)^Taking generated folders out of git$",
            "repo-workflows-unity-untrack-generated-b0c9a9c5",
            &[],
        ),
    ),
    (
        136,
        (
            "(?s)^Adding lines to \\.gitattributes$",
            "repo-workflows-unity-add-attributes-0b9b6295",
            &[],
        ),
    ),
    (
        157,
        (
            "(?s)^Adding lines to \\.gitignore$",
            "repo-workflows-unity-add-ignores-52181c32",
            &[],
        ),
    ),
    (
        162,
        (
            "(?s)^Registering UnityYAMLMerge$",
            "repo-workflows-unity-register-driver-5d4e5077",
            &[],
        ),
    ),
    (
        204,
        (
            "(?s)^(.*?) isn't UnityYAMLMerge$",
            "repo-workflows-unity-choose-tool-af82d6a2",
            &[("file-display-1", false)],
        ),
    ),
    (
        231,
        (
            "(?s)^Saving the choice$",
            "repo-workflows-unity-choose-tool-34706d9f",
            &[],
        ),
    ),
];
