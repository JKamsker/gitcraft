use super::*;

pub(super) const ROWS: &[(usize, Case)] = &[
    (
        15,
        Case {
            raw: "No key, agent, account or network action has run. Choose each step explicitly.",
            id: "repo-workflows-ssh_setup-new-8633c172",
            args: &[],
        },
    ),
    (
        16,
        Case {
            raw: "Setup worker stopped; no later result will be applied.",
            id: "repo-workflows-ssh_setup-poll-380743c6",
            args: &[],
        },
    ),
    (
        20,
        Case {
            raw: "Could not start the selected machine's terminal/tool. Check that OpenSSH/WSL is installed. A reserved new folder, if any, is retained; existing files were not overwritten.",
            id: "repo-workflows-ssh_setup-launch-0690e36b",
            args: &[],
        },
    ),
    (
        22,
        Case {
            raw: "Push exited 1234. Read the terminal's explanation. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine.",
            id: "repo-workflows-ssh_setup-poll-b5bb4cca",
            args: &[
                ("run-step-label-1", Sample::Text("Push", "Pushen")),
                ("code", Sample::Number(1234)),
            ],
        },
    ),
    (
        23,
        Case {
            raw: "SSH test exited 1234. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled.",
            id: "repo-workflows-ssh_setup-poll-acae7792",
            args: &[("code", Sample::Number(1234))],
        },
    ),
    (
        24,
        Case {
            raw: "This profile now selects that local SSH key. Author/signing and repository-local SSH overrides retain their existing precedence.",
            id: "repo-workflows-ssh_setup-poll-77510276",
            args: &[],
        },
    ),
    (
        29,
        Case {
            raw: "The active profile or its connection choices changed. Check the intended account again before uploading.",
            id: "repo-workflows-ssh_setup-poll-b927d5a6",
            args: &[],
        },
    ),
    (
        30,
        Case {
            raw: "The selected public key was registered. This does not enable commit signing or change a global account.",
            id: "repo-workflows-ssh_setup-poll-cf037753",
            args: &[],
        },
    ),
    (
        31,
        Case {
            raw: "SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions.",
            id: "repo-workflows-ssh_setup-open-ssh-setup-0e2de7fb",
            args: &[],
        },
    ),
    (
        34,
        Case {
            raw: "Push command completed successfully. Key files are retained; continue with the next explicit step.",
            id: "repo-workflows-ssh_setup-poll-b3badea2",
            args: &[("run-step-label-1", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        56,
        Case {
            raw: "Push is running. Enter any passphrase only in the terminal below.",
            id: "repo-workflows-ssh_setup-launch-2a66f8c5",
            args: &[("step-label-1", Sample::Text("Push", "Pushen"))],
        },
    ),
    (
        65,
        Case {
            raw: "Checked account: opaque-account-owner-1/Stage1/日本語/00001234 on opaque-self-host-2/Stage1/日本語/00001234. Upload is a separate action.",
            id: "repo-workflows-ssh_setup-poll-59faaa41",
            args: &[
                (
                    "account-owner-1",
                    Sample::Text(
                        "opaque-account-owner-1/Stage1/日本語/00001234",
                        "opaque-account-owner-1/Stage1/日本語/00001234",
                    ),
                ),
                (
                    "self-host-2",
                    Sample::Text(
                        "opaque-self-host-2/Stage1/日本語/00001234",
                        "opaque-self-host-2/Stage1/日本語/00001234",
                    ),
                ),
            ],
        },
    ),
    (
        130,
        Case {
            raw: "Working on the selected machine…",
            id: "repo-workflows-ssh_setup-background-e4e2ef57",
            args: &[],
        },
    ),
    (
        216,
        Case {
            raw: "Register public key",
            id: "repo-workflows-ssh_setup-label-03ff732f",
            args: &[],
        },
    ),
    (
        265,
        Case {
            raw: "Add to agent",
            id: "repo-workflows-ssh_setup-label-1db4242b",
            args: &[],
        },
    ),
    (
        286,
        Case {
            raw: "Create key",
            id: "repo-workflows-ssh_setup-label-b6cde903",
            args: &[],
        },
    ),
    (
        301,
        Case {
            raw: "Test SSH",
            id: "repo-workflows-ssh_setup-label-60083f45",
            args: &[],
        },
    ),
];
