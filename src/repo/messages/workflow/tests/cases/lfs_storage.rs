use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        128,
        Case {
            raw: "Could not set what LFS fetches: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-lfs_storage-fetch-only-e3be5d1f",
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
        202,
        Case {
            raw: "LFS now fetches only opaque-names-join-1/Stage1/日本語/00001234",
            id: "repo-workflows-lfs_storage-fetch-only-3f558a4c",
            args: &[(
                "names-join-1",
                Sample::Text(
                    "opaque-names-join-1/Stage1/日本語/00001234",
                    "opaque-names-join-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
];
