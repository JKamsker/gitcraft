use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        44,
        Case {
            raw: "Commit or stash your changes first: the rehearsal was of the commits alone",
            id: "repo-workflows-rehearse-apply-rehearsal-b09a6f3f",
            args: &[],
        },
    ),
    (
        93,
        Case {
            raw: "opaque-run-branch-1/Stage1/日本語/00001234 is no longer checked out: rehearse again",
            id: "repo-workflows-rehearse-apply-rehearsal-d63206ac",
            args: &[(
                "run-branch-1",
                Sample::Text(
                    "opaque-run-branch-1/Stage1/日本語/00001234",
                    "opaque-run-branch-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        103,
        Case {
            raw: "Check out a branch to rehearse with it",
            id: "repo-workflows-rehearse-start-rehearsal-252af634",
            args: &[],
        },
    ),
    (
        135,
        Case {
            raw: "A rehearsal is going on already",
            id: "repo-workflows-rehearse-start-rehearsal-bbfc5a1d",
            args: &[],
        },
    ),
];
