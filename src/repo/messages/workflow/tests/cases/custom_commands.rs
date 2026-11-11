use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[(
    138,
    Case {
        raw: "$opaque-name/Stage1/日本語/00001234 can't hold a NUL character",
        id: "repo-workflows-custom_commands-run-custom-44771367",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    },
)];
