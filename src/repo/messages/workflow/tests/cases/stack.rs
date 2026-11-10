use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        25,
        Case {
            raw: "Restack aborted: every branch is back where it was but opaque-kept-join-1/Stage1/日本語/00001234, which got commits after it was restacked and is left as it is",
            id: "repo-workflows-stack-restack-done-c27578ee",
            args: &[(
                "kept-join-1",
                Sample::Text(
                    "opaque-kept-join-1/Stage1/日本語/00001234",
                    "opaque-kept-join-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        35,
        Case {
            raw: "1234 of 1234 done, then opaque-branch/Stage1/日本語/00001234 failed: opaque-e/Stage1/日本語/00001234. Publish again to go on: what was done stays, no pull request is opened twice.",
            id: "repo-workflows-stack-publish-done-6ffb5e8c",
            args: &[
                ("report-done-len-1", Sample::Number(1234)),
                ("total", Sample::Number(1234)),
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "e",
                    Sample::Text(
                        "opaque-e/Stage1/日本語/00001234",
                        "opaque-e/Stage1/日本語/00001234",
                    ),
                ),
                ("noun", Sample::Text("pull request", "Pull Request")),
            ],
        },
    ),
    (
        38,
        Case {
            raw: "This repository isn't on a forge gitcraft knows: there is nowhere to publish to.",
            id: "repo-workflows-stack-open-publish-a23e7f8b",
            args: &[],
        },
    ),
    (
        45,
        Case {
            raw: "Restack stopped at opaque-branch/Stage1/日本語/00001234 on conflicts: resolve them in its worktree at opaque/Stage1/日本語/00001234, then Continue restack (or Abort restack)",
            id: "repo-workflows-stack-restack-done-33b4b547",
            args: &[
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "place",
                    Sample::Text(
                        " in its worktree at opaque/Stage1/日本語/00001234",
                        " in seinem Arbeitsverzeichnis unter opaque/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        60,
        Case {
            raw: "Nothing to restack: every layer is on its parent's tip.",
            id: "repo-workflows-stack-open-restack-419e0d8e",
            args: &[],
        },
    ),
    (
        66,
        Case {
            raw: "Restack: every branch was already on its parent's tip",
            id: "repo-workflows-stack-restack-done-1fc5c0ba",
            args: &[],
        },
    ),
    (
        69,
        Case {
            raw: "Restack aborted: every branch is back where it was",
            id: "repo-workflows-stack-restack-done-a7222989",
            args: &[],
        },
    ),
    (
        71,
        Case {
            raw: "Restack: 1234 branches moved · Ctrl+Z puts them back",
            id: "repo-workflows-stack-restack-done-7f6cce25",
            args: &[("n", Sample::Number(1234))],
        },
    ),
    (
        80,
        Case {
            raw: "Restack: 1 branch moved · Ctrl+Z puts it back",
            id: "repo-workflows-stack-restack-done-dbfb6ce0",
            args: &[],
        },
    ),
    (
        98,
        Case {
            raw: "opaque-layer-parent-1/Stage1/日本語/00001234 is gone: pick what opaque-layer-branch-2/Stage1/日本語/00001234 is based on first.",
            id: "repo-workflows-stack-open-publish-3e534e60",
            args: &[
                (
                    "layer-parent-1",
                    Sample::Text(
                        "opaque-layer-parent-1/Stage1/日本語/00001234",
                        "opaque-layer-parent-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "layer-branch-2",
                    Sample::Text(
                        "opaque-layer-branch-2/Stage1/日本語/00001234",
                        "opaque-layer-branch-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        115,
        Case {
            raw: "Publish: run it again once Push is done",
            id: "repo-workflows-stack-start-publish-e0c26cc9",
            args: &[("busy", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        149,
        Case {
            raw: "Push: run it again once Push is done",
            id: "repo-workflows-stack-run-stack-op-95c7214f",
            args: &[
                ("label", Sample::Text("Push", "Pushen")),
                ("busy", Sample::Text("Push", "Pushen")),
            ],
        },
    ),
    (
        163,
        Case {
            raw: "Couldn't read the stacks: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-stack-remove-parent-4272623a",
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
        177,
        Case {
            raw: "Publish pull requests: nothing to do",
            id: "repo-workflows-stack-summary-47eeff3f",
            args: &[("noun", Sample::Text("pull request", "Pull Request"))],
        },
    ),
    (
        195,
        Case {
            raw: "Remove opaque-branch/Stage1/日本語/00001234 from its stack",
            id: "repo-workflows-stack-remove-parent-e6733ec1",
            args: &[(
                "branch",
                Sample::Text(
                    "opaque-branch/Stage1/日本語/00001234",
                    "opaque-branch/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        209,
        Case {
            raw: " in its worktree at opaque-wt-display-1/Stage1/日本語/00001234",
            id: "repo-workflows-stack-restack-done-6cecc855",
            args: &[(
                "wt-display-1",
                Sample::Text(
                    "opaque-wt-display-1/Stage1/日本語/00001234",
                    "opaque-wt-display-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        257,
        Case {
            raw: "Published pull requests: opaque-parts-join-1/Stage1/日本語/00001234",
            id: "repo-workflows-stack-summary-7f75368f",
            args: &[
                ("noun", Sample::Text("pull request", "Pull Request")),
                (
                    "parts-join-1",
                    Sample::Text(
                        "opaque-parts-join-1/Stage1/日本語/00001234",
                        "opaque-parts-join-1/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        258,
        Case {
            raw: "Abort restack",
            id: "repo-workflows-stack-abort-restack-4fe4c0de",
            args: &[],
        },
    ),
    (
        294,
        Case {
            raw: "Base opaque-branch/Stage1/日本語/00001234 on opaque-parent/Stage1/日本語/00001234",
            id: "repo-workflows-stack-set-parent-81fef295",
            args: &[
                (
                    "branch",
                    Sample::Text(
                        "opaque-branch/Stage1/日本語/00001234",
                        "opaque-branch/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "parent",
                    Sample::Text(
                        "opaque-parent/Stage1/日本語/00001234",
                        "opaque-parent/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        295,
        Case {
            raw: "Publish pull requests",
            id: "repo-workflows-stack-start-publish-c4ffa0c1",
            args: &[("noun", Sample::Text("pull request", "Pull Request"))],
        },
    ),
    (
        307,
        Case {
            raw: "Restack",
            id: "repo-workflows-stack-start-restack-9fbc2915",
            args: &[],
        },
    ),
];
