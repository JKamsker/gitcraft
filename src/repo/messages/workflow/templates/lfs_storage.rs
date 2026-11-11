pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        128,
        (
            "(?s)^Could not set what LFS fetches: (.*?)$",
            "repo-workflows-lfs_storage-fetch-only-e3be5d1f",
            &[("e", false)],
        ),
    ),
    (
        202,
        (
            "(?s)^LFS now fetches only (.*?)$",
            "repo-workflows-lfs_storage-fetch-only-3f558a4c",
            &[("names-join-1", false)],
        ),
    ),
];
