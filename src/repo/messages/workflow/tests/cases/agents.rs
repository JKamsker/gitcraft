use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        28,
        Case {
            raw: "No agent terminal in this worktree to send opaque-what/Stage1/日本語/00001234 to: start the agent (Start agent task…), or pick its terminal next to Send to agent",
            id: "repo-workflows-agents-send-to-agent-a6f80242",
            args: &[(
                "what",
                Sample::Text(
                    "opaque-what/Stage1/日本語/00001234",
                    "opaque-what/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        78,
        Case {
            raw: "The agent checkpoint hooks weren't installed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-agents-after-worktree-add-9fd23075",
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
        95,
        Case {
            raw: "Wait for the running operation to finish",
            id: "repo-workflows-agents-start-agent-task-f80416ff",
            args: &[],
        },
    ),
    (
        101,
        Case {
            raw: "The worktree setup recipe wasn't run: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-agents-after-worktree-add-a2ffe669",
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
        112,
        Case {
            raw: "The agent hooks weren't installed: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-agents-install-hooks-a8f2778c",
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
        119,
        Case {
            raw: "Couldn't send opaque-what/Stage1/日本語/00001234 to the Push terminal: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-agents-send-to-agent-4e9f0022",
            args: &[
                (
                    "what",
                    Sample::Text(
                        "opaque-what/Stage1/日本語/00001234",
                        "opaque-what/Stage1/日本語/00001234",
                    ),
                ),
                ("label", Sample::Text("Push", "Pushen")),
                (
                    "e",
                    Sample::Text(
                        "opaque-e/Stage1/日本語/00001234",
                        "opaque-e/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        150,
        Case {
            raw: "Worktree setup recipe saved",
            id: "repo-workflows-agents-save-recipe-97381b52",
            args: &[],
        },
    ),
    (
        151,
        Case {
            raw: "Installing the agent hooks…",
            id: "repo-workflows-agents-install-hooks-2f5d6dc4",
            args: &[],
        },
    ),
    (
        152,
        Case {
            raw: "The turns weren't deleted: opaque-e/Stage1/日本語/00001234",
            id: "repo-workflows-agents-forget-turns-1d1d391e",
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
        225,
        Case {
            raw: "Start agent task opaque-new-branch-1/Stage1/日本語/00001234",
            id: "repo-workflows-agents-start-agent-task-d4ec2011",
            args: &[(
                "new-branch-1",
                Sample::Text(
                    "opaque-new-branch-1/Stage1/日本語/00001234",
                    "opaque-new-branch-1/Stage1/日本語/00001234",
                ),
            )],
        },
    ),
    (
        319,
        Case {
            raw: "setup",
            id: "repo-workflows-agents-agent-act-8fb6d5f3",
            args: &[],
        },
    ),
];
