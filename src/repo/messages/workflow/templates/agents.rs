pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        28,
        (
            "(?s)^No agent terminal in this worktree to send (.*?) to: start the agent \\(Start agent task…\\), or pick its terminal next to Send to agent$",
            "repo-workflows-agents-send-to-agent-a6f80242",
            &[("what", false)],
        ),
    ),
    (
        78,
        (
            "(?s)^The agent checkpoint hooks weren't installed: (.*?)$",
            "repo-workflows-agents-after-worktree-add-9fd23075",
            &[("e", false)],
        ),
    ),
    (
        95,
        (
            "(?s)^Wait for the running operation to finish$",
            "repo-workflows-agents-start-agent-task-f80416ff",
            &[],
        ),
    ),
    (
        101,
        (
            "(?s)^The worktree setup recipe wasn't run: (.*?)$",
            "repo-workflows-agents-after-worktree-add-a2ffe669",
            &[("e", false)],
        ),
    ),
    (
        112,
        (
            "(?s)^The agent hooks weren't installed: (.*?)$",
            "repo-workflows-agents-install-hooks-a8f2778c",
            &[("e", false)],
        ),
    ),
    (
        119,
        (
            "(?s)^Couldn't send (.*?) to the (.*?) terminal: (.*?)$",
            "repo-workflows-agents-send-to-agent-4e9f0022",
            &[("what", false), ("label", false), ("e", false)],
        ),
    ),
    (
        150,
        (
            "(?s)^Worktree setup recipe saved$",
            "repo-workflows-agents-save-recipe-97381b52",
            &[],
        ),
    ),
    (
        151,
        (
            "(?s)^Installing the agent hooks…$",
            "repo-workflows-agents-install-hooks-2f5d6dc4",
            &[],
        ),
    ),
    (
        152,
        (
            "(?s)^The turns weren't deleted: (.*?)$",
            "repo-workflows-agents-forget-turns-1d1d391e",
            &[("e", false)],
        ),
    ),
    (
        225,
        (
            "(?s)^Start agent task (.*?)$",
            "repo-workflows-agents-start-agent-task-d4ec2011",
            &[("new-branch-1", false)],
        ),
    ),
    (
        319,
        (
            "(?s)^setup$",
            "repo-workflows-agents-agent-act-8fb6d5f3",
            &[],
        ),
    ),
];
