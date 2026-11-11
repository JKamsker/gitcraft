use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[(
    299,
    Case {
        raw: "build · opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-bakeoff-build-c9238812",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    },
)];
