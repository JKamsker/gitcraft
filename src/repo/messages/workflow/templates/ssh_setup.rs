pub(super) const ROWS: &[(usize, crate::repo::messages::TemplateSpec)] = &[
    (
        15,
        (
            "(?s)^No key, agent, account or network action has run\\. Choose each step explicitly\\.$",
            "repo-workflows-ssh_setup-new-8633c172",
            &[],
        ),
    ),
    (
        16,
        (
            "(?s)^Setup worker stopped; no later result will be applied\\.$",
            "repo-workflows-ssh_setup-poll-380743c6",
            &[],
        ),
    ),
    (
        20,
        (
            "(?s)^Could not start the selected machine's terminal/tool\\. Check that OpenSSH/WSL is installed\\. A reserved new folder, if any, is retained; existing files were not overwritten\\.$",
            "repo-workflows-ssh_setup-launch-0690e36b",
            &[],
        ),
    ),
    (
        22,
        (
            "(?s)^(.*?) exited ([0-9]+)\\. Read the terminal's explanation\\. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine\\.$",
            "repo-workflows-ssh_setup-poll-b5bb4cca",
            &[("run-step-label-1", false), ("code", true)],
        ),
    ),
    (
        23,
        (
            "(?s)^SSH test exited ([0-9]+)\\. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled\\.$",
            "repo-workflows-ssh_setup-poll-acae7792",
            &[("code", true)],
        ),
    ),
    (
        24,
        (
            "(?s)^This profile now selects that local SSH key\\. Author/signing and repository\\-local SSH overrides retain their existing precedence\\.$",
            "repo-workflows-ssh_setup-poll-77510276",
            &[],
        ),
    ),
    (
        29,
        (
            "(?s)^The active profile or its connection choices changed\\. Check the intended account again before uploading\\.$",
            "repo-workflows-ssh_setup-poll-b927d5a6",
            &[],
        ),
    ),
    (
        30,
        (
            "(?s)^The selected public key was registered\\. This does not enable commit signing or change a global account\\.$",
            "repo-workflows-ssh_setup-poll-cf037753",
            &[],
        ),
    ),
    (
        31,
        (
            "(?s)^SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions\\.$",
            "repo-workflows-ssh_setup-open-ssh-setup-0e2de7fb",
            &[],
        ),
    ),
    (
        34,
        (
            "(?s)^(.*?) command completed successfully\\. Key files are retained; continue with the next explicit step\\.$",
            "repo-workflows-ssh_setup-poll-b3badea2",
            &[("run-step-label-1", false)],
        ),
    ),
    (
        56,
        (
            "(?s)^(.*?) is running\\. Enter any passphrase only in the terminal below\\.$",
            "repo-workflows-ssh_setup-launch-2a66f8c5",
            &[("step-label-1", false)],
        ),
    ),
    (
        65,
        (
            "(?s)^Checked account: (.*?) on (.*?)\\. Upload is a separate action\\.$",
            "repo-workflows-ssh_setup-poll-59faaa41",
            &[("account-owner-1", false), ("self-host-2", false)],
        ),
    ),
    (
        130,
        (
            "(?s)^Working on the selected machine…$",
            "repo-workflows-ssh_setup-background-e4e2ef57",
            &[],
        ),
    ),
    (
        216,
        (
            "(?s)^Register public key$",
            "repo-workflows-ssh_setup-label-03ff732f",
            &[],
        ),
    ),
    (
        265,
        (
            "(?s)^Add to agent$",
            "repo-workflows-ssh_setup-label-1db4242b",
            &[],
        ),
    ),
    (
        286,
        (
            "(?s)^Create key$",
            "repo-workflows-ssh_setup-label-b6cde903",
            &[],
        ),
    ),
    (
        301,
        (
            "(?s)^Test SSH$",
            "repo-workflows-ssh_setup-label-60083f45",
            &[],
        ),
    ),
];
