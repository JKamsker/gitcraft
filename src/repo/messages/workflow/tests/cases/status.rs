use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[(
    70,
    Case {
        raw: "Push: done, but its earlier version couldn't be kept: opaque-detail/Stage1/日本語/00001234",
        id: "repo-workflows-status-rewrite-retention",
        args: &[
            ("operation", Sample::Text("Push", "Pushen")),
            (
                "detail",
                Sample::Text(
                    "opaque-detail/Stage1/日本語/00001234",
                    "opaque-detail/Stage1/日本語/00001234",
                ),
            ),
        ],
    },
)];
