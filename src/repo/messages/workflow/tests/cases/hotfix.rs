use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        99,
        Case {
            raw: "opaque-tag/Stage1/日本語/00001234 isn't a version to start a hotfix from",
            id: "repo-workflows-hotfix-hotfix-act-06b55025",
            args: &[(
                "tag",
                Sample::Text(
                    "opaque-tag/Stage1/日本語/00001234",
                    "opaque-tag/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        328,
        Case {
            raw: "Push (stopped part-way)",
            id: "repo-workflows-hotfix-hotfix-op-done-fbdef87e",
            args: &[("run-label-1", Sample::Text("Push", "Pushen"))],
        },
    ),
];
