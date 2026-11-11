use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        43,
        Case {
            raw: "What changed in #opaque-number/Stage1/日本語/00001234: it isn't in the list of this repository's pull requests",
            id: "repo-workflows-versions-open-pr-since-1e0c2674",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        47,
        Case {
            raw: "Mark as seen: #opaque-number/Stage1/日本語/00001234 isn't in the list of this repository's pull requests",
            id: "repo-workflows-versions-mark-pr-seen-93fe69bf",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        89,
        Case {
            raw: "Couldn't remember the version of #opaque-number/Stage1/日本語/00001234 seen: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-versions-seen-marked-f9ed1e57",
            args: &[
                (
                    "number",
                    Sample::Text(
                        "opaque-number/Stage1/日本語/00001234",
                        "opaque-number/Stage1/日本語/00001234",
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
        100,
        Case {
            raw: "#opaque-number/Stage1/日本語/00001234 hasn't changed since you last looked",
            id: "repo-workflows-versions-open-pr-since-f8a6567c",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        211,
        Case {
            raw: "What changed in #opaque-number/Stage1/日本語/00001234: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-versions-open-pr-since-e9aaff48",
            args: &[
                (
                    "number",
                    Sample::Text(
                        "opaque-number/Stage1/日本語/00001234",
                        "opaque-number/Stage1/日本語/00001234",
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
        222,
        Case {
            raw: "Compare versions: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-versions-open-branch-versions-70c06512",
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
        281,
        Case {
            raw: "Reading #opaque-number/Stage1/日本語/00001234…",
            id: "repo-workflows-versions-open-pr-since-60afdf08",
            args: &[(
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
