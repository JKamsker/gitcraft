use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        27,
        Case {
            raw: "Preview ready. The exported PNG contains exactly this composition; ellipses mark shortened or width-limited text.",
            id: "repo-workflows-postcards-poll-postcards-5358001c",
            args: &[],
        },
    ),
    (
        36,
        Case {
            raw: "Pinned release range loaded. Review the disclosure list, then compose the PNG preview.",
            id: "repo-workflows-postcards-poll-postcards-60ccb94f",
            args: &[],
        },
    ),
    (
        41,
        Case {
            raw: "Profile changed. Read the release range again before composing a postcard.",
            id: "repo-workflows-postcards-poll-postcards-4773e588",
            args: &[],
        },
    ),
    (
        134,
        Case {
            raw: "PNG saved as a new local file.",
            id: "repo-workflows-postcards-poll-postcards-10867442",
            args: &[],
        },
    ),
    (
        148,
        Case {
            raw: "Postcard rendering cancelled",
            id: "repo-workflows-postcards-render-8e2ed023",
            args: &[],
        },
    ),
    (
        170,
        Case {
            raw: "Postcard work cancelled.",
            id: "repo-workflows-postcards-postcard-window-b6333aff",
            args: &[],
        },
    ),
];
