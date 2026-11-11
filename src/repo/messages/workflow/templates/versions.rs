pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        43,
        (
            "(?s)^What changed in \\#(.*?): it isn't in the list of this repository's pull requests$",
            "repo-workflows-versions-open-pr-since-1e0c2674",
            &[("number", false)],
        ),
    ),
    (
        47,
        (
            "(?s)^Mark as seen: \\#(.*?) isn't in the list of this repository's pull requests$",
            "repo-workflows-versions-mark-pr-seen-93fe69bf",
            &[("number", false)],
        ),
    ),
    (
        89,
        (
            "(?s)^Couldn't remember the version of \\#(.*?) seen: (.*?)$",
            "repo-workflows-versions-seen-marked-f9ed1e57",
            &[("number", false), ("e", false)],
        ),
    ),
    (
        100,
        (
            "(?s)^\\#(.*?) hasn't changed since you last looked$",
            "repo-workflows-versions-open-pr-since-f8a6567c",
            &[("number", false)],
        ),
    ),
    (
        211,
        (
            "(?s)^What changed in \\#(.*?): (.*?)$",
            "repo-workflows-versions-open-pr-since-e9aaff48",
            &[("number", false), ("e", false)],
        ),
    ),
    (
        222,
        (
            "(?s)^Compare versions: (.*?)$",
            "repo-workflows-versions-open-branch-versions-70c06512",
            &[("e", false)],
        ),
    ),
    (
        281,
        (
            "(?s)^Reading \\#(.*?)…$",
            "repo-workflows-versions-open-pr-since-60afdf08",
            &[("number", false)],
        ),
    ),
];
