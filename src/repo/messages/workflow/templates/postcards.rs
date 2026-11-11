pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        27,
        (
            "(?s)^Preview ready\\. The exported PNG contains exactly this composition; ellipses mark shortened or width\\-limited text\\.$",
            "repo-workflows-postcards-poll-postcards-5358001c",
            &[],
        ),
    ),
    (
        36,
        (
            "(?s)^Pinned release range loaded\\. Review the disclosure list, then compose the PNG preview\\.$",
            "repo-workflows-postcards-poll-postcards-60ccb94f",
            &[],
        ),
    ),
    (
        41,
        (
            "(?s)^Profile changed\\. Read the release range again before composing a postcard\\.$",
            "repo-workflows-postcards-poll-postcards-4773e588",
            &[],
        ),
    ),
    (
        134,
        (
            "(?s)^PNG saved as a new local file\\.$",
            "repo-workflows-postcards-poll-postcards-10867442",
            &[],
        ),
    ),
    (
        148,
        (
            "(?s)^Postcard rendering cancelled$",
            "repo-workflows-postcards-render-8e2ed023",
            &[],
        ),
    ),
    (
        170,
        (
            "(?s)^Postcard work cancelled\\.$",
            "repo-workflows-postcards-postcard-window-b6333aff",
            &[],
        ),
    ),
];
