//! Exact workflow paint contracts supplied at immutable 32a7cb12.
//! No producer/control/command identities are changed. Source-owned version headings,
//! test-lane results and push headlines retain their typed owner paint boundaries.

pub(super) const TEMPLATES: &[super::TemplateSpec] = &[
    (
        "(?s)^Could not start the selected machine's terminal/tool\\. Check that OpenSSH/WSL is installed\\. A reserved new folder, if any, is retained; existing files were not overwritten\\.$",
        "repo-workflows-ssh_setup-launch-0690e36b",
        &[],
    ), // source entry 202
    (
        "(?s)^Revision appended to \\.git\\-blame\\-ignore\\-revs; staged contents preserved\\. Ordinary Git blame can use blame\\.ignoreRevsFile=\\.git\\-blame\\-ignore\\-revs\\.$",
        "repo-workflows-line_dossier-dossier-ignore-finished-6e1731ff",
        &[],
    ), // source entry 147
    (
        "(?s)^(.*?) exited ([0-9]+)\\. Read the terminal's explanation\\. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine\\.$",
        "repo-workflows-ssh_setup-poll-b5bb4cca",
        &[("run-step-label-1", false), ("code", true)],
    ), // source entry 209
    (
        "(?s)^SSH test exited ([0-9]+)\\. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled\\.$",
        "repo-workflows-ssh_setup-poll-acae7792",
        &[("code", true)],
    ), // source entry 207
    (
        "(?s)^This profile now selects that local SSH key\\. Author/signing and repository\\-local SSH overrides retain their existing precedence\\.$",
        "repo-workflows-ssh_setup-poll-77510276",
        &[],
    ), // source entry 206
    (
        "(?s)^Restack aborted: every branch is back where it was but (.*?), which got commits after it was restacked and is left as it is$",
        "repo-workflows-stack-restack-done-c27578ee",
        &[("kept-join-1", false)],
    ), // source entry 232
    (
        "(?s)^Approving runs the contributor's code on your CI: look through \\#(.*?)'s changes first, then choose Approve and run again$",
        "repo-workflows-forks-approve-runs-a77eca22",
        &[("number", false)],
    ), // source entry 123
    (
        "(?s)^Preview ready\\. The exported PNG contains exactly this composition; ellipses mark shortened or width\\-limited text\\.$",
        "repo-workflows-postcards-poll-postcards-5358001c",
        &[],
    ), // source entry 160
    (
        "(?s)^No agent terminal in this worktree to send (.*?) to: start the agent \\(Start agent task…\\), or pick its terminal next to Send to agent$",
        "repo-workflows-agents-send-to-agent-a6f80242",
        &[("what", false)],
    ), // source entry 18
    (
        "(?s)^The active profile or its connection choices changed\\. Check the intended account again before uploading\\.$",
        "repo-workflows-ssh_setup-poll-b927d5a6",
        &[],
    ), // source entry 203
    (
        "(?s)^The selected public key was registered\\. This does not enable commit signing or change a global account\\.$",
        "repo-workflows-ssh_setup-poll-cf037753",
        &[],
    ), // source entry 205
    (
        "(?s)^SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions\\.$",
        "repo-workflows-ssh_setup-open-ssh-setup-0e2de7fb",
        &[],
    ), // source entry 210
    (
        "(?s)^Dependencies finished, but the parent repository, profile or sign\\-in changed\\. Prepare its push again\\.$",
        "repo-workflows-repository_tools-dependency-job-a7b1dc6d",
        &[],
    ), // source entry 91
    (
        "(?s)^The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it \\(or abort it\\)$",
        "repo-workflows-forks-forks-op-done-9c338e90",
        &[],
    ), // source entry 127
    (
        "(?s)^(.*?) command completed successfully\\. Key files are retained; continue with the next explicit step\\.$",
        "repo-workflows-ssh_setup-poll-b3badea2",
        &[("run-step-label-1", false)],
    ), // source entry 208
    (
        "(?s)^([0-9]+) of ([0-9]+) done, then (.*?) failed: (.*?)\\. Publish again to go on: what was done stays, no (.*?) is opened twice\\.$",
        "repo-workflows-stack-publish-done-6ffb5e8c",
        &[
            ("report-done-len-1", true),
            ("total", true),
            ("branch", false),
            ("e", false),
            ("noun", false),
        ],
    ), // source entry 219
    (
        "(?s)^Pinned release range loaded\\. Review the disclosure list, then compose the PNG preview\\.$",
        "repo-workflows-postcards-poll-postcards-60ccb94f",
        &[],
    ), // source entry 159
    (
        "(?s)^(.*?): that would skip “(.*?)”, which moved no branch: step through it with (.*?)\\+Z / (.*?)\\+Y instead$",
        "repo-workflows-timeline-jump-b77dfff9",
        &[
            ("label", false),
            ("skipped", false),
            ("cmd", false),
            ("", false),
        ],
    ), // source entry 290
    (
        "(?s)^This repository isn't on a forge gitcraft knows: there is nowhere to publish to\\.$",
        "repo-workflows-stack-open-publish-a23e7f8b",
        &[],
    ), // source entry 214
    (
        "(?s)^Didn't check out (.*?): couldn't list the commits it would leave on no branch \\((.*?)\\)$",
        "repo-workflows-detached-leave-to-48b0ce4c",
        &[("leave-label-1", false), ("e", false)],
    ), // source entry 53
    (
        "(?s)^Go online and wait for the current operation before preparing an outbox push$",
        "repo-workflows-repository_tools-act-d64d1e25",
        &[],
    ), // source entry 95
    (
        "(?s)^Profile changed\\. Read the release range again before composing a postcard\\.$",
        "repo-workflows-postcards-poll-postcards-4773e588",
        &[],
    ), // source entry 158
    (
        "(?s)^Finish or close the conflict or the rebase editor first to see the timeline$",
        "repo-workflows-timeline-toggle-timeline-d12d6347",
        &[],
    ), // source entry 297
    (
        "(?s)^What changed in \\#(.*?): it isn't in the list of this repository's pull requests$",
        "repo-workflows-versions-open-pr-since-1e0c2674",
        &[("number", false)],
    ), // source entry 319
    (
        "(?s)^Commit or stash your changes first: the rehearsal was of the commits alone$",
        "repo-workflows-rehearse-apply-rehearsal-b09a6f3f",
        &[],
    ), // source entry 187
    (
        "(?s)^Restack stopped at (.*?) on conflicts: resolve them(.*?), then Continue restack \\(or Abort restack\\)$",
        "repo-workflows-stack-restack-done-33b4b547",
        &[("branch", false), ("place", false)],
    ), // source entry 234
    (
        "(?s)^Nothing to commit yet: add or change files in the repository's folder$",
        "repo-workflows-guide-start-first-commit-881f72cb",
        &[],
    ), // source entry 130
    (
        "(?s)^Mark as seen: \\#(.*?) isn't in the list of this repository's pull requests$",
        "repo-workflows-versions-mark-pr-seen-93fe69bf",
        &[("number", false)],
    ), // source entry 334
    (
        "(?s)^Prepared subtree profile/sign\\-in changed; prepare and approve again$",
        "repo-workflows-repository_tools-act-d021ec81",
        &[],
    ), // source entry 100
    (
        "(?s)^(.*?) is rebased here, but not pushed to (.*?): Ctrl\\+Z puts it back as it was$",
        "repo-workflows-forks-forks-op-done-8287ad6b",
        &[("plan-local-1", false), ("plan-fork-2", false)],
    ), // source entry 128
    (
        "(?s)^The last scan is still ending on that machine; try again in a moment$",
        "repo-workflows-reflog-start-scan-7a1d64fe",
        &[],
    ), // source entry 183
    (
        "(?s)^Finish or close the conflict editor or the interactive rebase first$",
        "repo-workflows-doctor-open-doctor-745454bd",
        &[],
    ), // source entry 63
    (
        "(?s)^Prepared outbox profile/sign\\-in changed; prepare and approve again$",
        "repo-workflows-repository_tools-start-send-628a48e6",
        &[],
    ), // source entry 96
    (
        "(?s)^Distribute staged fixes: HEAD is detached, check out a branch first$",
        "repo-workflows-surgery-open-absorb-3373a6f7",
        &[],
    ), // source entry 235
    (
        "(?s)^Distribute staged fixes: nothing is staged, stage the fixes first$",
        "repo-workflows-surgery-open-absorb-55d2e258",
        &[],
    ), // source entry 237
    (
        "(?s)^HEAD is detached: check out a branch to make the first commit on$",
        "repo-workflows-guide-ask-empty-commit-348d9f75",
        &[],
    ), // source entry 129
    (
        "(?s)^(.*?) is running\\. Enter any passphrase only in the terminal below\\.$",
        "repo-workflows-ssh_setup-launch-2a66f8c5",
        &[("step-label-1", false)],
    ), // source entry 201
    (
        "(?s)^Distribute staged fixes: close the editor that is open first$",
        "repo-workflows-surgery-open-absorb-2cddd12d",
        &[],
    ), // source entry 238
    (
        "(?s)^(.*?): the repository couldn't be read again; reload it \\(F5\\) first$",
        "repo-workflows-timeline-timeline-unsettled-3fd5b04c",
        &[("what", false)],
    ), // source entry 292
    (
        "(?s)^Revision already present; ignore file and index unchanged$",
        "repo-workflows-line_dossier-dossier-ignore-finished-366a809a",
        &[],
    ), // source entry 148
    (
        "(?s)^Nothing to restack: every layer is on its parent's tip\\.$",
        "repo-workflows-stack-open-restack-419e0d8e",
        &[],
    ), // source entry 222
    (
        "(?s)^Distribute staged fixes: another editor opened meanwhile$",
        "repo-workflows-surgery-open-absorb-55f28458",
        &[],
    ), // source entry 240
    (
        "(?s)^UnityYAMLMerge wasn't found: choose it in Unity set\\-up…$",
        "repo-workflows-unity-smart-merge-conflict-5a10252d",
        &[],
    ), // source entry 311
    (
        "(?s)^History summary cancelled or exceeded its output limit$",
        "repo-workflows-line_dossier-run-a9f1b338",
        &[],
    ), // source entry 151
    (
        "(?s)^Moved\\-code analysis cancelled or exceeded 40 seconds$",
        "repo-workflows-file_comparison-read-431f5828",
        &[],
    ), // source entry 112
    (
        "(?s)^Checked account: (.*?) on (.*?)\\. Upload is a separate action\\.$",
        "repo-workflows-ssh_setup-poll-59faaa41",
        &[("account-owner-1", false), ("self-host-2", false)],
    ), // source entry 204
    (
        "(?s)^Restack: every branch was already on its parent's tip$",
        "repo-workflows-stack-restack-done-1fc5c0ba",
        &[],
    ), // source entry 227
    (
        "(?s)^Push refused, and the remote could not be checked: (.*?)$",
        "repo-workflows-push-remote-checked-c6e528c9",
        &[("e", false)],
    ), // source entry 181
    (
        "(?s)^UnityYAMLMerge merged (.*?): check the result, then save$",
        "repo-workflows-unity-smart-merge-done-afe126d0",
        &[("path", false)],
    ), // source entry 312
    (
        "(?s)^Restack aborted: every branch is back where it was$",
        "repo-workflows-stack-restack-done-a7222989",
        &[],
    ), // source entry 231
    (
        "(?s)^(.*?): done, but its earlier version couldn't be kept: (.*?)$",
        "repo-workflows-status-rewrite-retention",
        &[("operation", false), ("detail", false)],
    ), // source entry 350
    (
        "(?s)^Restack: ([0-9]+) branches moved · Ctrl\\+Z puts them back$",
        "repo-workflows-stack-restack-done-7f6cce25",
        &[("n", true)],
    ), // source entry 229
    (
        "(?s)^Split commit: close the staged fixes editor first$",
        "repo-workflows-surgery-open-split-01ee6f3c",
        &[],
    ), // source entry 248
    (
        "(?s)^Pick a name no branch has for the new branch \\(not “(.*?)”\\)$",
        "repo-workflows-timeline-run-remove-6fd826a1",
        &[("name", false)],
    ), // source entry 276
    (
        "(?s)^Couldn't save Sign off for this repository \\(git config (.*?)\\): (.*?)$",
        "repo-workflows-dco-dco-act-10fe4a7c",
        &[("signoff-sign-off-key-1", false), ("e", false)],
    ), // source entry 47
    (
        "(?s)^Tracker account changed; reload the issue source$",
        "repo-workflows-issue_work-start-issue-563140a4",
        &[],
    ), // source entry 134
    (
        "(?s)^A parallel bisect is already running in this tab$",
        "repo-workflows-parallel_bisect-start-parallel-3450e92d",
        &[],
    ), // source entry 153
    (
        "(?s)^Convert legacy grafts to removable replace refs$",
        "repo-workflows-repository_tools-repository-tools-act-ad133eb8",
        &[],
    ), // source entry 85
    (
        "(?s)^The agent checkpoint hooks weren't installed: (.*?)$",
        "repo-workflows-agents-after-worktree-add-9fd23075",
        &[("e", false)],
    ), // source entry 21
    (
        "(?s)^The pull request's branch is not available yet$",
        "repo-workflows-pr_view-select-pr-b788526c",
        &[],
    ), // source entry 164
    (
        "(?s)^Restack: 1 branch moved · Ctrl\\+Z puts it back$",
        "repo-workflows-stack-restack-done-dbfb6ce0",
        &[],
    ), // source entry 228
    (
        "(?s)^(.*?): waiting for the repository to be read again…$",
        "repo-workflows-timeline-timeline-unsettled-21705388",
        &[("what", false)],
    ), // source entry 291
    (
        "(?s)^(.*?): done\\. Commit \\.gitattributes to share it\\.$",
        "repo-workflows-lfs_locks-lock-job-35b33376",
        &[("label", false)],
    ), // source entry 141
    (
        "(?s)^Choose one saved turn to preview its inverse$",
        "repo-workflows-repository_tools-act-6dc9a943",
        &[],
    ), // source entry 102
    (
        "(?s)^This repository has no remote to rebase onto$",
        "repo-workflows-forks-plan-fork-rebase-1abac341",
        &[],
    ), // source entry 119
    (
        "(?s)^UnityYAMLMerge wasn't found: choose it first$",
        "repo-workflows-unity-register-driver-f6c55b48",
        &[],
    ), // source entry 309
    (
        "(?s)^UnityYAMLMerge left ([0-9]+) conflicts in (.*?) to choose$",
        "repo-workflows-unity-smart-merge-done-7e870a3e",
        &[("n", true), ("path", false)],
    ), // source entry 313
    (
        "(?s)^Distribute staged fixes: a (.*?) is in progress$",
        "repo-workflows-surgery-open-absorb-edcc444b",
        &[("state-label-to-lowercase-1", false)],
    ), // source entry 236
    (
        "(?s)^Tests are running already: stop them first$",
        "repo-workflows-test_lane-start-tests-aa89f17f",
        &[],
    ), // source entry 252
    (
        "(?s)^Couldn't remember the version of \\#(.*?) seen: (.*?)$",
        "repo-workflows-versions-seen-marked-f9ed1e57",
        &[("number", false), ("e", false)],
    ), // source entry 333
    (
        "(?s)^Wait for (.*?) to finish, then check out again$",
        "repo-workflows-detached-detached-act-70dad4a3",
        &[("busy", false)],
    ), // source entry 54
    (
        "(?s)^Publish dependencies before parent review$",
        "repo-workflows-repository_tools-dependency-job-f48f9c31",
        &[],
    ), // source entry 89
    (
        "(?s)^Wait for the operation going on to finish$",
        "repo-workflows-parallel_bisect-mark-parallel-result-e5e3ecc5",
        &[],
    ), // source entry 154
    (
        "(?s)^(.*?) is no longer checked out: rehearse again$",
        "repo-workflows-rehearse-apply-rehearsal-d63206ac",
        &[("run-branch-1", false)],
    ), // source entry 188
    (
        "(?s)^Move changes from (.*?) to the working changes$",
        "repo-workflows-surgery-run-move-afb8b0ad",
        &[("short-hash-1", false)],
    ), // source entry 246
    (
        "(?s)^Wait for the running operation to finish$",
        "repo-workflows-agents-start-agent-task-f80416ff",
        &[],
    ), // source entry 24
    (
        "(?s)^Check submodule publication dependencies$",
        "repo-workflows-repository_tools-dependency-job-57895189",
        &[],
    ), // source entry 90
    (
        "(?s)^This repository has no remote to push to$",
        "repo-workflows-push-push-labelled-4933a6c9",
        &[],
    ), // source entry 172
    (
        "(?s)^(.*?) is gone: pick what (.*?) is based on first\\.$",
        "repo-workflows-stack-open-publish-3e534e60",
        &[("layer-parent-1", false), ("layer-branch-2", false)],
    ), // source entry 215
    (
        "(?s)^(.*?) isn't a version to start a hotfix from$",
        "repo-workflows-hotfix-hotfix-act-06b55025",
        &[("tag", false)],
    ), // source entry 131
    (
        "(?s)^\\#(.*?) hasn't changed since you last looked$",
        "repo-workflows-versions-open-pr-since-f8a6567c",
        &[("number", false)],
    ), // source entry 321
    (
        "(?s)^The worktree setup recipe wasn't run: (.*?)$",
        "repo-workflows-agents-after-worktree-add-a2ffe669",
        &[("e", false)],
    ), // source entry 22
    (
        "(?s)^This repository has no CODEOWNERS file$",
        "repo-workflows-owners-owners-act-9cd4150e",
        &[],
    ), // source entry 146
    (
        "(?s)^Check out a branch to rehearse with it$",
        "repo-workflows-rehearse-start-rehearsal-252af634",
        &[],
    ), // source entry 185
    (
        "(?s)^Didn't check out (.*?): (.*?) started meanwhile$",
        "repo-workflows-detached-leave-to-007a4c0e",
        &[("leave-label-1", false), ("busy", false)],
    ), // source entry 52
    (
        "(?s)^Wait for the running operation to end$",
        "repo-workflows-doctor-doctor-act-9bab8d95",
        &[],
    ), // source entry 64
    (
        "(?s)^Change virtual branch materialization$",
        "repo-workflows-repository_tools-act-ce061d46",
        &[],
    ), // source entry 105
    (
        "(?s)^Couldn't approve \\#(.*?)'s workflow runs: (.*?)$",
        "repo-workflows-forks-approve-runs-282082c7",
        &[("number", false), ("e", false)],
    ), // source entry 126
    (
        "(?s)^Wait for the operation running to end$",
        "repo-workflows-release-run-release-d9523bf2",
        &[],
    ), // source entry 189
    (
        "(?s)^Autosend opted\\-in plain outbox push$",
        "repo-workflows-repository_tools-start-send-e7b0c78d",
        &[],
    ), // source entry 97
    (
        "(?s)^Couldn't read the branches' issues: (.*?)$",
        "repo-workflows-issue_work-take-issue-read-bbcbbb59",
        &[("e", false)],
    ), // source entry 133
    (
        "(?s)^Distribute staged fixes into ([0-9]+) commit(?:s?)$",
        "repo-workflows-operation-distribute-staged",
        &[("count", true)],
    ), // source entry 349
    (
        "(?s)^The agent hooks weren't installed: (.*?)$",
        "repo-workflows-agents-install-hooks-a8f2778c",
        &[("e", false)],
    ), // source entry 28
    (
        "(?s)^Join imported histories temporarily$",
        "repo-workflows-repository_tools-repository-tools-act-a7b0c90c",
        &[],
    ), // source entry 83
    (
        "(?s)^Where \\#(.*?) comes from isn't known yet$",
        "repo-workflows-forks-checkout-fork-5ecb3bac",
        &[("number", false)],
    ), // source entry 118
    (
        "(?s)^Publish: run it again once (.*?) is done$",
        "repo-workflows-stack-start-publish-e0c26cc9",
        &[("busy", false)],
    ), // source entry 216
    (
        "(?s)^Taking generated folders out of git$",
        "repo-workflows-unity-untrack-generated-b0c9a9c5",
        &[],
    ), // source entry 306
    (
        "(?s)^Fetching \\#(.*?)'s branch and its base…$",
        "repo-workflows-forks-plan-fork-rebase-ac71056e",
        &[("number", false)],
    ), // source entry 120
    (
        "(?s)^Appending reviewed ignore revision$",
        "repo-workflows-line_dossier-append-dossier-ignore-716419f1",
        &[],
    ), // source entry 327
    (
        "(?s)^Couldn't send (.*?) to the (.*?) terminal: (.*?)$",
        "repo-workflows-agents-send-to-agent-4e9f0022",
        &[("what", false), ("label", false), ("e", false)],
    ), // source entry 19
    (
        "(?s)^Connection changed; reopen outbox$",
        "repo-workflows-repository_tools-act-ccd2fdda",
        &[],
    ), // source entry 92
    (
        "(?s)^Commit virtual branch owned hunks$",
        "repo-workflows-repository_tools-act-de986fa8",
        &[],
    ), // source entry 107
    (
        "(?s)^Checking against the agent fence…$",
        "repo-workflows-fence_view-fence-gate-2b8e4f3b",
        &[],
    ), // source entry 111
    (
        "(?s)^Distribute staged fixes: reading…$",
        "repo-workflows-surgery-open-absorb-5ebbefda",
        &[],
    ), // source entry 239
    (
        "(?s)^Push the signed\\-off commits of (.*?)$",
        "repo-workflows-dco-dco-op-done-55d2b32b",
        &[("plan-branch-1", false)],
    ), // source entry 49
    (
        "(?s)^Checking against the agent fence$",
        "repo-workflows-fence_view-fence-gate-6b730ef5",
        &[],
    ), // source entry 110
    (
        "(?s)^Moved\\-code comparison cancelled$",
        "repo-workflows-file_comparison-assemble-56abde67",
        &[],
    ), // source entry 114
    (
        "(?s)^Finish or close the editor first$",
        "repo-workflows-file_comparison-open-file-comparison-a6cb5dc1",
        &[],
    ), // source entry 116
    (
        "(?s)^Could not set what LFS fetches: (.*?)$",
        "repo-workflows-lfs_storage-fetch-only-e3be5d1f",
        &[("e", false)],
    ), // source entry 145
    (
        "(?s)^(.*?) failed, so nothing was pushed: (.*?)$",
        "repo-workflows-push-chain-step-ended-dfa32698",
        &[("what", false), ("e", false)],
    ), // source entry 177
    (
        "(?s)^Working on the selected machine…$",
        "repo-workflows-ssh_setup-background-e4e2ef57",
        &[],
    ), // source entry 200
    (
        "(?s)^Cancelled before terminal start$",
        "repo-workflows-devcontainer-terminal-f171832a",
        &[],
    ), // source entry 58
    (
        "(?s)^Reconcile exact replay receipts$",
        "repo-workflows-repository_tools-act-c08c65bd",
        &[],
    ), // source entry 70
    (
        "(?s)^\\#(.*?)'s workflow runs are approved$",
        "repo-workflows-forks-approve-runs-a151ad19",
        &[("number", false)],
    ), // source entry 125
    (
        "(?s)^PNG saved as a new local file\\.$",
        "repo-workflows-postcards-poll-postcards-10867442",
        &[],
    ), // source entry 161
    (
        "(?s)^A rehearsal is going on already$",
        "repo-workflows-rehearse-start-rehearsal-bbfc5a1d",
        &[],
    ), // source entry 184
    (
        "(?s)^Adding lines to \\.gitattributes$",
        "repo-workflows-unity-add-attributes-0b9b6295",
        &[],
    ), // source entry 304
    (
        "(?s)^Checkout (.*?): wait for (.*?) to finish$",
        "repo-workflows-detached-leave-to-28ae5898",
        &[("leave-label-1", false), ("busy", false)],
    ), // source entry 51
    (
        "(?s)^\\$(.*?) can't hold a NUL character$",
        "repo-workflows-custom_commands-run-custom-44771367",
        &[("name", false)],
    ), // source entry 46
    (
        "(?s)^Undo accepted isolated replay$",
        "repo-workflows-repository_tools-act-346c061a",
        &[],
    ), // source entry 67
    (
        "(?s)^Save virtual branch ownership$",
        "repo-workflows-repository_tools-act-901dccbe",
        &[],
    ), // source entry 104
    (
        "(?s)^Undo virtual branch operation$",
        "repo-workflows-repository_tools-act-63403bc6",
        &[],
    ), // source entry 106
    (
        "(?s)^Approving \\#(.*?)'s workflow runs…$",
        "repo-workflows-forks-approve-runs-2f659b28",
        &[("number", false)],
    ), // source entry 124
    (
        "(?s)^Forget the issue of (.*?) failed: (.*?)$",
        "repo-workflows-issue_work-unlink-issue-6df1900e",
        &[("branch", false), ("e", false)],
    ), // source entry 139
    (
        "(?s)^Type the command to test with$",
        "repo-workflows-test_lane-start-tests-4b88f5c5",
        &[],
    ), // source entry 253
    (
        "(?s)^Add selected SSH key to agent$",
        "repo-workflows-operation-ssh-14022",
        &[],
    ), // source entry 354
    (
        "(?s)^Register public SSH key there$",
        "repo-workflows-operation-ssh-14048",
        &[],
    ), // source entry 358
    (
        "(?s)^Replay in isolated worktrees$",
        "repo-workflows-repository_tools-act-5bd57c48",
        &[],
    ), // source entry 65
    (
        "(?s)^Postcard rendering cancelled$",
        "repo-workflows-postcards-render-8e2ed023",
        &[],
    ), // source entry 165
    (
        "(?s)^(.*?): run it again once (.*?) is done$",
        "repo-workflows-stack-run-stack-op-95c7214f",
        &[("label", false), ("busy", false)],
    ), // source entry 213
    (
        "(?s)^Worktree setup recipe saved$",
        "repo-workflows-agents-save-recipe-97381b52",
        &[],
    ), // source entry 26
    (
        "(?s)^Installing the agent hooks…$",
        "repo-workflows-agents-install-hooks-2f5d6dc4",
        &[],
    ), // source entry 27
    (
        "(?s)^The turns weren't deleted: (.*?)$",
        "repo-workflows-agents-forget-turns-1d1d391e",
        &[("e", false)],
    ), // source entry 29
    (
        "(?s)^Publish exact subtree split$",
        "repo-workflows-repository_tools-act-494336ed",
        &[],
    ), // source entry 101
    (
        "(?s)^Couldn't read the remotes: (.*?)$",
        "repo-workflows-forks-plan-tidy-adf73b14",
        &[("e", false)],
    ), // source entry 121
    (
        "(?s)^Mark in the bisect failed: (.*?)$",
        "repo-workflows-parallel_bisect-mark-parallel-result-48f3588b",
        &[("e", false)],
    ), // source entry 156
    (
        "(?s)^Install local Git LFS hooks$",
        "repo-workflows-tour-tour-view-952aa9de",
        &[],
    ), // source entry 301
    (
        "(?s)^Adding lines to \\.gitignore$",
        "repo-workflows-unity-add-ignores-52181c32",
        &[],
    ), // source entry 305
    (
        "(?s)^Test selected SSH key there$",
        "repo-workflows-operation-ssh-14038",
        &[],
    ), // source entry 357
    (
        "(?s)^Finish or close the (.*?) first$",
        "repo-workflows-ci_view-open-ci-991ae978",
        &[("editor", false)],
    ), // source entry 43
    (
        "(?s)^Sign off the commits of (.*?) \\(here only\\)$",
        "repo-workflows-dco-dco-op-done-5d0c9ff1",
        &[("plan-branch-1", false)],
    ), // source entry 48
    (
        "(?s)^Remove unused fork remotes$",
        "repo-workflows-forks-plan-tidy-a7e148ae",
        &[],
    ), // source entry 122
    (
        "(?s)^Registering UnityYAMLMerge$",
        "repo-workflows-unity-register-driver-5d4e5077",
        &[],
    ), // source entry 310
    (
        "(?s)^Couldn't read the stacks: (.*?)$",
        "repo-workflows-stack-remove-parent-4272623a",
        &[("e", false)],
    ), // source entry 332
    (
        "(?s)^Add SSH key to agent there$",
        "repo-workflows-operation-ssh-14024",
        &[],
    ), // source entry 355
    (
        "(?s)^Cancelled before starting$",
        "repo-workflows-devcontainer-devcontainer-act-84f8f92f",
        &[],
    ), // source entry 56
    (
        "(?s)^Resolve submodule pointer$",
        "repo-workflows-repository_tools-repository-tools-act-66f98362",
        &[],
    ), // source entry 78
    (
        "(?s)^Save pull and push routes$",
        "repo-workflows-repository_tools-repository-tools-act-82d14313",
        &[],
    ), // source entry 79
    (
        "(?s)^Save remote configuration$",
        "repo-workflows-repository_tools-repository-tools-act-4e161156",
        &[],
    ), // source entry 81
    (
        "(?s)^Start work on (.*?): wait for (.*?)$",
        "repo-workflows-issue_work-start-issue-e2b80c1b",
        &[("issue-key-1", false), ("busy", false)],
    ), // source entry 135
    (
        "(?s)^Postcard work cancelled\\.$",
        "repo-workflows-postcards-postcard-window-b6333aff",
        &[],
    ), // source entry 162
    (
        "(?s)^Distribute staged fixes: (.*?)$",
        "repo-workflows-surgery-open-absorb-ed3b6cd2",
        &[("e", false)],
    ), // source entry 241
    (
        "(?s)^Go to before the timeline$",
        "repo-workflows-timeline-jump-target-bd63e54c",
        &[],
    ), // source entry 294
    (
        "(?s)^Sign off the commits of (.*?)$",
        "repo-workflows-dco-fix-label-dfc76b8d",
        &[("plan-branch-1", false)],
    ), // source entry 50
    (
        "(?s)^Continue isolated replay$",
        "repo-workflows-repository_tools-act-65d162a8",
        &[],
    ), // source entry 66
    (
        "(?s)^Sync: (.*?) pulled and pushed$",
        "repo-workflows-push-sync-027fe2bd",
        &[("current", false)],
    ), // source entry 174
    (
        "(?s)^(.*?) pulled from (.*?) and pushed$",
        "repo-workflows-push-push-act-ebd6e3a6",
        &[("r-branch-1", false), ("r-target-label-2", false)],
    ), // source entry 175
    (
        "(?s)^Publish (.*?)s: nothing to do$",
        "repo-workflows-stack-summary-47eeff3f",
        &[("noun", false)],
    ), // source entry 220
    (
        "(?s)^Rebase \\#(.*?) onto (.*?) and push$",
        "repo-workflows-operation-rebase-fork-push",
        &[("number", false), ("base", false)],
    ), // source entry 337
    (
        "(?s)^Dev container operation$",
        "repo-workflows-devcontainer-devcontainer-act-519a489a",
        &[],
    ), // source entry 57
    (
        "(?s)^Close isolated proposal$",
        "repo-workflows-repository_tools-act-08cd49af",
        &[],
    ), // source entry 69
    (
        "(?s)^Unshelve one changelist$",
        "repo-workflows-repository_tools-act-551afaa9",
        &[],
    ), // source entry 75
    (
        "(?s)^History worker panicked$",
        "repo-workflows-saved_views-poll-33647b3e",
        &[],
    ), // source entry 195
    (
        "(?s)^Move changes: reading (.*?)…$",
        "repo-workflows-surgery-open-move-9a3c16a9",
        &[("short-hash-1", false)],
    ), // source entry 243
    (
        "(?s)^Split commit: reading (.*?)…$",
        "repo-workflows-surgery-open-split-a472a361",
        &[("short-hash-1", false)],
    ), // source entry 249
    (
        "(?s)^Before the oldest entry$",
        "repo-workflows-timeline-tree-rows-6fa556f4",
        &[],
    ), // source entry 298
    (
        "(?s)^File history cancelled$",
        "repo-workflows-atlas-new-70ba4563",
        &[],
    ), // source entry 30
    (
        "(?s)^Can't start a bisect: (.*?)$",
        "repo-workflows-bisect-start-bisect-5c82e3e0",
        &[("why", false)],
    ), // source entry 34
    (
        "(?s)^Accept isolated replay$",
        "repo-workflows-repository_tools-act-b64c091a",
        &[],
    ), // source entry 68
    (
        "(?s)^Make clone independent$",
        "repo-workflows-repository_tools-repository-tools-act-07fb4089",
        &[],
    ), // source entry 77
    (
        "(?s)^Save publication group$",
        "repo-workflows-repository_tools-repository-tools-act-3c1c7a3a",
        &[],
    ), // source entry 80
    (
        "(?s)^Send exact queued push$",
        "repo-workflows-repository_tools-start-send-994ea6b0",
        &[],
    ), // source entry 98
    (
        "(?s)^Push refused: (.*?) is gone$",
        "repo-workflows-push-remote-checked-205a41fa",
        &[("target-label-1", false)],
    ), // source entry 180
    (
        "(?s)^History worker stopped$",
        "repo-workflows-saved_views-poll-da12ac51",
        &[],
    ), // source entry 193
    (
        "(?s)^History read cancelled$",
        "repo-workflows-saved_views-poll-dc101692",
        &[],
    ), // source entry 194
    (
        "(?s)^Remove (.*?) from its stack$",
        "repo-workflows-stack-remove-parent-e6733ec1",
        &[("branch", false)],
    ), // source entry 212
    (
        "(?s)^Move changes from (.*?) to (.*?)$",
        "repo-workflows-surgery-run-move-b5726a1d",
        &[("short-hash-1", false), ("short-t-hash-2", false)],
    ), // source entry 245
    (
        "(?s)^LFS: Prune old objects$",
        "repo-workflows-operation-lfs-prune",
        &[],
    ), // source entry 346
    (
        "(?s)^No bisect is going on$",
        "repo-workflows-bisect-mark-bisect-a1c6c771",
        &[],
    ), // source entry 36
    (
        "(?s)^Commit one changelist$",
        "repo-workflows-repository_tools-act-c1eec3a9",
        &[],
    ), // source entry 74
    (
        "(?s)^Shelve one changelist$",
        "repo-workflows-repository_tools-act-bea96408",
        &[],
    ), // source entry 76
    (
        "(?s)^Revert one saved turn$",
        "repo-workflows-repository_tools-act-fc328635",
        &[],
    ), // source entry 103
    (
        "(?s)^LFS now fetches only (.*?)$",
        "repo-workflows-lfs_storage-fetch-only-3f558a4c",
        &[("names-join-1", false)],
    ), // source entry 144
    (
        "(?s)^Initialize submodules$",
        "repo-workflows-tour-tour-view-7bfe85f2",
        &[],
    ), // source entry 302
    (
        "(?s)^(.*?) isn't UnityYAMLMerge$",
        "repo-workflows-unity-choose-tool-af82d6a2",
        &[("file-display-1", false)],
    ), // source entry 307
    (
        "(?s)^Push the rebased (.*?) to (.*?)$",
        "repo-workflows-operation-push-rebased",
        &[("branch", false), ("remote", false)],
    ), // source entry 338
    (
        "(?s)^Test selected SSH key$",
        "repo-workflows-operation-ssh-14036",
        &[],
    ), // source entry 356
    (
        "(?s)^Marked in the bisect$",
        "repo-workflows-parallel_bisect-mark-parallel-result-e05d54d7",
        &[],
    ), // source entry 155
    (
        "(?s)^Comment relocation: (.*?)$",
        "repo-workflows-pr_view-pr-files-arrived-20be6481",
        &[("e", false)],
    ), // source entry 163
    (
        "(?s)^ in its worktree at (.*?)$",
        "repo-workflows-stack-restack-done-6cecc855",
        &[("wt-display-1", false)],
    ), // source entry 233
    (
        "(?s)^Remove “(.*?)” from (.*?), as (.*?)$",
        "repo-workflows-timeline-run-remove-73ccc5e0",
        &[
            ("self-entry-label-plan-id-1", false),
            ("plan-branch-2", false),
            ("name", false),
        ],
    ), // source entry 277
    (
        "(?s)^What changed in \\#(.*?): (.*?)$",
        "repo-workflows-versions-open-pr-since-e9aaff48",
        &[("number", false), ("e", false)],
    ), // source entry 322
    (
        "(?s)^Create SSH key there$",
        "repo-workflows-operation-ssh-14019",
        &[],
    ), // source entry 353
    (
        "(?s)^Migrate ref storage$",
        "repo-workflows-repository_tools-repository-tools-act-888e8b45",
        &[],
    ), // source entry 82
    (
        "(?s)^Remove outbox entry$",
        "repo-workflows-repository_tools-act-7f694b62",
        &[],
    ), // source entry 94
    (
        "(?s)^(.*?) (.*?) from staging lens$",
        "repo-workflows-file_comparison-comparison-pick-28dc27ab",
        &[("pick-verb-1", false), ("snapshot-spec-path-2", false)],
    ), // source entry 117
    (
        "(?s)^Register public key$",
        "repo-workflows-ssh_setup-label-03ff732f",
        &[],
    ), // source entry 198
    (
        "(?s)^Force unlock ([0-9]+) files$",
        "repo-workflows-operation-force-unlock-count",
        &[("count", true)],
    ), // source entry 344
    (
        "(?s)^Content recovery: (.*?)$",
        "repo-workflows-content_recovery-content-perform-9d57ebb2",
        &[("e", false)],
    ), // source entry 45
    (
        "(?s)^Remove replace ref$",
        "repo-workflows-repository_tools-repository-tools-act-69a2d9db",
        &[],
    ), // source entry 84
    (
        "(?s)^Email patches · b4$",
        "repo-workflows-email-email-act-c19c9f05",
        &[],
    ), // source entry 109
    (
        "(?s)^Sync: (.*?) published \\(it had nothing to pull\\)$",
        "repo-workflows-push-sync-5ca83b4b",
        &[("current", false)],
    ), // source entry 173
    (
        "(?s)^Compare versions: (.*?)$",
        "repo-workflows-versions-open-branch-versions-70c06512",
        &[("e", false)],
    ), // source entry 318
    (
        "(?s)^Earlier versions: (.*?)$",
        "repo-workflows-working_drafts-poll-working-drafts-7a49e8a4",
        &[("e", false)],
    ), // source entry 326
    (
        "(?s)^Mark in the bisect$",
        "repo-workflows-operation-mark-bisect",
        &[],
    ), // source entry 347
    (
        "(?s)^Start agent task (.*?)$",
        "repo-workflows-agents-start-agent-task-d4ec2011",
        &[("new-branch-1", false)],
    ), // source entry 25
    (
        "(?s)^Remove (.*?) \\((.*?)\\) from (.*?)$",
        "repo-workflows-doctor-remove-confirm-b0b26eef",
        &[
            ("origin-key-1", false),
            ("shortened-origin-shown-value-2", false),
            ("file", false),
        ],
    ), // source entry 60
    (
        "(?s)^(.*?) is still running$",
        "repo-workflows-repository_tools-repository-tool-mutation-988356ee",
        &[("busy", false)],
    ), // source entry 86
    (
        "(?s)^Subtree operation$",
        "repo-workflows-repository_tools-act-88c16034",
        &[],
    ), // source entry 99
    (
        "(?s)^Ignore revision: (.*?)$",
        "repo-workflows-line_dossier-dossier-ignore-finished-4c50387c",
        &[("error", false)],
    ), // source entry 149
    (
        "(?s)^Summary cancelled$",
        "repo-workflows-line_dossier-run-ae6f0623",
        &[],
    ), // source entry 150
    (
        "(?s)^Saving the choice$",
        "repo-workflows-unity-choose-tool-34706d9f",
        &[],
    ), // source entry 308
    (
        "(?s)^Move commit down$",
        "repo-workflows-agent-rewrite-2630fe5c",
        &[],
    ), // source entry 14
    (
        "(?s)^Skip this commit$",
        "repo-workflows-bisect-mark-bisect-15306802",
        &[],
    ), // source entry 38
    (
        "(?s)^Isolated restack$",
        "repo-workflows-repository_tools-show-9174163c",
        &[],
    ), // source entry 71
    (
        "(?s)^Remote preview: (.*?)$",
        "repo-workflows-repository_tools-remote-config-preview-bba0304c",
        &[("e", false)],
    ), // source entry 87
    (
        "(?s)^Queue exact push$",
        "repo-workflows-repository_tools-act-b967f21f",
        &[],
    ), // source entry 93
    (
        "(?s)^Start work on (.*?): (.*?)$",
        "repo-workflows-issue_work-start-issue-c14dd822",
        &[("issue-key-1", false), ("why", false)],
    ), // source entry 136
    (
        "(?s)^Recover content$",
        "repo-workflows-content_recovery-content-perform-fbdf23f8",
        &[],
    ), // source entry 44
    (
        "(?s)^Save changelist$",
        "repo-workflows-repository_tools-act-a3cbb206",
        &[],
    ), // source entry 73
    (
        "(?s)^Branch (.*?) deleted$",
        "repo-workflows-timeline-fallback-label-b5bd817d",
        &[("c-name-1", false)],
    ), // source entry 271
    (
        "(?s)^Nothing to undo$",
        "repo-workflows-timeline-undo-83290078",
        &[],
    ), // source entry 284
    (
        "(?s)^Nothing to redo$",
        "repo-workflows-timeline-redo-4aaf2710",
        &[],
    ), // source entry 287
    (
        "(?s)^Create branch (.*?)$",
        "repo-workflows-agent-create-branch-d2a49cac",
        &[("name", false)],
    ), // source entry 8
    (
        "(?s)^Move commit up$",
        "repo-workflows-agent-rewrite-3d1ccb79",
        &[],
    ), // source entry 13
    (
        "(?s)^Delete branch (.*?)$",
        "repo-workflows-agent-delete-branch-d32d800a",
        &[("name", false)],
    ), // source entry 15
    (
        "(?s)^Push refused: (.*?)$",
        "repo-workflows-push-remote-checked-3bbec358",
        &[("banner-headline-r-1", false)],
    ), // source entry 179
    (
        "(?s)^Move changes: (.*?)$",
        "repo-workflows-surgery-open-move-9a352cd5",
        &[("why", false)],
    ), // source entry 242
    (
        "(?s)^Split commit: (.*?)$",
        "repo-workflows-surgery-open-split-ad93af6b",
        &[("why", false)],
    ), // source entry 247
    (
        "(?s)^Saved manually$",
        "repo-workflows-working_drafts-save-working-draft-3e48426a",
        &[],
    ), // source entry 335
    (
        "(?s)^Finish hotfix (.*?)$",
        "repo-workflows-operation-finish-hotfix",
        &[("tag", false)],
    ), // source entry 340
    (
        "(?s)^Start work on (.*?)$",
        "repo-workflows-operation-start-issue-work",
        &[("issue", false)],
    ), // source entry 342
    (
        "(?s)^Create SSH key$",
        "repo-workflows-operation-ssh-14017",
        &[],
    ), // source entry 352
    (
        "(?s)^Finish bisect$",
        "repo-workflows-bisect-reset-bisect-1bdcc155",
        &[],
    ), // source entry 32
    (
        "(?s)^Container (.*?) · (.*?)$",
        "repo-workflows-devcontainer-terminal-77e417ed",
        &[("binding-id-12-1", false), ("binding-folder-2", false)],
    ), // source entry 59
    (
        "(?s)^Set (.*?) to (.*?) for (.*?)$",
        "repo-workflows-doctor-set-row-921f564b",
        &[
            ("setting-key-1", false),
            ("shortened-value-2", false),
            ("scope-choice-scope-to-lowercase-3", false),
        ],
    ), // source entry 61
    (
        "(?s)^Publish group$",
        "repo-workflows-repository_tools-publish-group-3cc9a2c6",
        &[],
    ), // source entry 88
    (
        "(?s)^Published (.*?)s: (.*?)$",
        "repo-workflows-stack-summary-7f75368f",
        &[("noun", false), ("parts-join-1", false)],
    ), // source entry 221
    (
        "(?s)^Abort restack$",
        "repo-workflows-stack-abort-restack-4fe4c0de",
        &[],
    ), // source entry 225
    (
        "(?s)^Put (.*?) back at (.*?)$",
        "repo-workflows-timeline-put-branch-back-94acd0c3",
        &[("branch", false), ("crate-repo-short-commit-1", false)],
    ), // source entry 295
    (
        "(?s)^Working draft$",
        "repo-workflows-working_drafts-change-working-draft-278865c0",
        &[],
    ), // source entry 336
    (
        "(?s)^Force unlock (.*?)$",
        "repo-workflows-operation-force-unlock",
        &[("path", false)],
    ), // source entry 343
    (
        "(?s)^Track (.*?) in LFS$",
        "repo-workflows-operation-lfs-track",
        &[("pattern", false)],
    ), // source entry 345
    (
        "(?s)^Cherry\\-pick$",
        "repo-workflows-agent-plan-4c49fdb6",
        &[],
    ), // source entry 1
    (
        "(?s)^Start bisect$",
        "repo-workflows-bisect-start-bisect-db936451",
        &[],
    ), // source entry 35
    (
        "(?s)^Add to agent$",
        "repo-workflows-ssh_setup-label-1db4242b",
        &[],
    ), // source entry 197
    (
        "(?s)^Split (.*?) into ([0-9]+)$",
        "repo-workflows-surgery-run-split-ddd763c5",
        &[("short-hash-1", false), ("messages-len-2", true)],
    ), // source entry 251
    (
        "(?s)^Merge commit$",
        "repo-workflows-timeline-describe-reflog-8e9d44a6",
        &[],
    ), // source entry 257
    (
        "(?s)^Branch (.*?) made$",
        "repo-workflows-timeline-fallback-label-1ac54e67",
        &[("c-name-1", false)],
    ), // source entry 270
    (
        "(?s)^Stash staged$",
        "repo-workflows-partial_stash-label-057347af",
        &[],
    ), // source entry 330
    (
        "(?s)^Apply stash (.*?)$",
        "repo-workflows-reflog-apply-stash-93a6ce40",
        &[("short-pick-hash-1", false)],
    ), // source entry 331
    (
        "(?s)^Push hotfix (.*?)$",
        "repo-workflows-operation-push-hotfix",
        &[("tag", false)],
    ), // source entry 341
    (
        "(?s)^Rename (.*?) to (.*?)$",
        "repo-workflows-agent-rename-branch-5de8c7a7",
        &[("from", false), ("to", false)],
    ), // source entry 9
    (
        "(?s)^Drop commit$",
        "repo-workflows-agent-rewrite-601e2a07",
        &[],
    ), // source entry 12
    (
        "(?s)^Stop bisect$",
        "repo-workflows-bisect-reset-bisect-0ddac35c",
        &[],
    ), // source entry 33
    (
        "(?s)^(.*?): wait for (.*?)$",
        "repo-workflows-issue_work-refresh-issue-a0e9ad2a",
        &[("label", false), ("busy", false)],
    ), // source entry 137
    (
        "(?s)^(.*?): (.*?) failed: (.*?)$",
        "repo-workflows-release-release-stopped-a4afd9be",
        &[("label", false), ("step-label-1", false), ("e", false)],
    ), // source entry 190
    (
        "(?s)^(.*?), and ([0-9]+) more$",
        "repo-workflows-timeline-outside-label-d1d10f56",
        &[("first", false), ("rest-len-1", true)],
    ), // source entry 254
    (
        "(?s)^Branch made$",
        "repo-workflows-timeline-describe-reflog-56bb1bb2",
        &[],
    ), // source entry 266
    (
        "(?s)^(.*?): done\\. (.*?) \\(receipt in the timeline, Ctrl\\+Shift\\+Z\\)$",
        "repo-workflows-timeline-show-receipt-e172bf9c",
        &[("label", false), ("receipt-sentence-1", false)],
    ), // source entry 282
    (
        "(?s)^Going there$",
        "repo-workflows-timeline-jump-a5337838",
        &[],
    ), // source entry 289
    (
        "(?s)^Reading \\#(.*?)…$",
        "repo-workflows-versions-open-pr-since-60afdf08",
        &[("number", false)],
    ), // source entry 320
    (
        "(?s)^Stash files$",
        "repo-workflows-partial_stash-label-598787f5",
        &[],
    ), // source entry 329
    (
        "(?s)^Force push$",
        "repo-workflows-agent-push-1d502fc3",
        &[],
    ), // source entry 16
    (
        "(?s)^Bisect run$",
        "repo-workflows-bisect-start-bisect-run-d300d73e",
        &[],
    ), // source entry 41
    (
        "(?s)^(.*?) refused: (.*?)$",
        "repo-workflows-push-push-op-ended-79357f2e",
        &[("label", false), ("out-trim-1", false)],
    ), // source entry 176
    (
        "(?s)^Create key$",
        "repo-workflows-ssh_setup-label-b6cde903",
        &[],
    ), // source entry 196
    (
        "(?s)^Checkout (.*?)$",
        "repo-workflows-agent-checkout-734452f6",
        &[("branch", false)],
    ), // source entry 10
    (
        "(?s)^Mark (.*?) as (.*?)$",
        "repo-workflows-bisect-mark-bisect-22f6b91b",
        &[("short-rev-1", false), ("bisect-word-mark-2", false)],
    ), // source entry 37
    (
        "(?s)^(.*?) failed: (.*?)$",
        "repo-workflows-bisect-bisect-step-done-8336fbfe",
        &[("label", false), ("e", false)],
    ), // source entry 40
    (
        "(?s)^Turn on: (.*?)$",
        "repo-workflows-doctor-turn-on-action-d8e3e02d",
        &[("s-title-to-lowercase-1", false)],
    ), // source entry 62
    (
        "(?s)^Checking (.*?)$",
        "repo-workflows-push-check-remote-9a7c53eb",
        &[("what", false)],
    ), // source entry 178
    (
        "(?s)^Released (.*?)$",
        "repo-workflows-release-release-stopped-1904d12e",
        &[("run-plan-tag-1", false)],
    ), // source entry 191
    (
        "(?s)^(.*?): stopped$",
        "repo-workflows-release-release-stopped-328c7ab5",
        &[("label", false)],
    ), // source entry 192
    (
        "(?s)^Base (.*?) on (.*?)$",
        "repo-workflows-stack-set-parent-81fef295",
        &[("branch", false), ("parent", false)],
    ), // source entry 211
    (
        "(?s)^Publish (.*?)s$",
        "repo-workflows-stack-start-publish-c4ffa0c1",
        &[("noun", false)],
    ), // source entry 217
    (
        "(?s)^Reset to (.*?)$",
        "repo-workflows-timeline-describe-reflog-32fa2259",
        &[("to", false)],
    ), // source entry 261
    (
        "(?s)^(.*?): (.*?): done$",
        "repo-workflows-agent-agent-op-done-16d1e9b2",
        &[("running-agent-1", false), ("label", false)],
    ), // source entry 0
    (
        "(?s)^Unstage (.*?)$",
        "repo-workflows-agent-unstage-eb003390",
        &[("paths-label-paths-1", false)],
    ), // source entry 5
    (
        "(?s)^build · (.*?)$",
        "repo-workflows-bakeoff-build-c9238812",
        &[("name", false)],
    ), // source entry 31
    (
        "(?s)^Mark as (.*?)$",
        "repo-workflows-bisect-mark-bisect-9b0f3138",
        &[("bisect-word-mark-1", false)],
    ), // source entry 39
    (
        "(?s)^Test SSH$",
        "repo-workflows-ssh_setup-label-60083f45",
        &[],
    ), // source entry 199
    (
        "(?s)^Checkout$",
        "repo-workflows-timeline-describe-reflog-99e71f48",
        &[],
    ), // source entry 264
    (
        "(?s)^Discard (.*?)$",
        "repo-workflows-timeline-discard-remove-66f54467",
        &[("branch", false)],
    ), // source entry 278
    (
        "(?s)^Go to “(.*?)”$",
        "repo-workflows-timeline-jump-target-1e4e36b2",
        &[("self-timeline-get-id-label-1", false)],
    ), // source entry 293
    ("(?s)^Snapshot$", "repo-workflows-operation-snapshot", &[]), // source entry 348
    (
        "(?s)^Rebase (.*?)$",
        "repo-workflows-repository_tools-show-34b10b8d",
        &[("name", false)],
    ), // source entry 72
    (
        "(?s)^Restack$",
        "repo-workflows-stack-start-restack-9fbc2915",
        &[],
    ), // source entry 223
    (
        "(?s)^Git LFS$",
        "repo-workflows-tour-tour-view-6df591db",
        &[],
    ), // source entry 300
    ("(?s)^Revert$", "repo-workflows-agent-plan-0026c505", &[]),  // source entry 2
    (
        "(?s)^([0-9]+) paths$",
        "repo-workflows-agent-paths-label-26d36d06",
        &[("many-len-1", true)],
    ), // source entry 3
    (
        "(?s)^Stage (.*?)$",
        "repo-workflows-agent-stage-3b4ddea7",
        &[("paths-label-paths-1", false)],
    ), // source entry 4
    ("(?s)^Commit$", "repo-workflows-agent-commit-82a9c46f", &[]), // source entry 7
    (
        "(?s)^(.*?): done$",
        "repo-workflows-lfs_locks-lock-job-36f5baee",
        &[("label", false)],
    ), // source entry 142
    (
        "(?s)^Rebase$",
        "repo-workflows-timeline-describe-reflog-9eb5b275",
        &[],
    ), // source entry 260
    (
        "(?s)^Merge (.*?)$",
        "repo-workflows-timeline-describe-reflog-43695d49",
        &[("w-merge-len-1", false)],
    ), // source entry 267
    (
        "(?s)^(.*?) moved$",
        "repo-workflows-timeline-fallback-label-e48d463d",
        &[("c-name-1", false)],
    ), // source entry 272
    ("(?s)^Amend$", "repo-workflows-agent-commit-4475f515", &[]), // source entry 6
    ("(?s)^Stash$", "repo-workflows-agent-stash-38bdfeaa", &[]),  // source entry 11
    (
        "(?s)^setup$",
        "repo-workflows-agents-agent-act-8fb6d5f3",
        &[],
    ), // source entry 20
    (
        "(?s)^Reset$",
        "repo-workflows-timeline-describe-reflog-daee7606",
        &[],
    ), // source entry 262
    (
        "(?s)^Moved$",
        "repo-workflows-timeline-describe-reflog-b11c9047",
        &[],
    ), // source entry 269
    (
        "(?s)^Undo (.*?)$",
        "repo-workflows-timeline-undo-bf959af0",
        &[("entry-label-1", false)],
    ), // source entry 285
    (
        "(?s)^Redo (.*?)$",
        "repo-workflows-timeline-redo-2b66f1cb",
        &[("entry-label-1", false)],
    ), // source entry 288
    ("(?s)^Push$", "repo-workflows-agent-push-731ce7ed", &[]),    // source entry 17
    (
        "(?s)^Pull$",
        "repo-workflows-timeline-describe-reflog-b66d0441",
        &[],
    ), // source entry 265
    ("(?s)^Undo$", "repo-workflows-timeline-undo-a8283ade", &[]), // source entry 283
    ("(?s)^Redo$", "repo-workflows-timeline-redo-74273989", &[]), // source entry 286
    (
        "(?s)^(.*?) \\(stopped part\\-way\\)$",
        "repo-workflows-hotfix-hotfix-op-done-fbdef87e",
        &[("run-label-1", false)],
    ), // source entry 132
    (
        "(?s)^(.*?) \\(stopped partway\\)$",
        "repo-workflows-timeline-timeline-op-ended-be0297e1",
        &[("label", false)],
    ), // source entry 281
    (
        "(?s)^(.*?) \\(here only\\)$",
        "repo-workflows-operation-here-only",
        &[("operation", false)],
    ), // source entry 339
    // Additive immutable owner9471 contract; original396 rows retain their indices.
    (
        "(?s)^Check out #([0-9]+) to push to (.*?)$",
        "repo-workflows-forks-checkout-fork-abaaf4c3",
        &[("number", false), ("info-owner-1", false)],
    ),
];

#[derive(Clone, Copy)]
pub(super) enum Adapter {
    Operation,
    Scope,
    Health,
    Pick,
    Noun,
    Headline,
    Place,
    InProgress,
    Receipt,
}

pub(super) fn adapter(id: &str, name: &str) -> Option<Adapter> {
    match (id, name) {
        ("repo-workflows-operation-here-only", "operation")
        | ("repo-workflows-status-rewrite-retention", "operation") => Some(Adapter::Operation),
        ("repo-workflows-agent-agent-op-done-16d1e9b2", "label") => Some(Adapter::Operation),
        ("repo-workflows-agents-send-to-agent-4e9f0022", "label") => Some(Adapter::Operation),
        ("repo-workflows-bisect-bisect-step-done-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-detached-leave-to-28ae5898", "busy") => Some(Adapter::Operation),
        ("repo-workflows-detached-leave-to-007a4c0e", "busy") => Some(Adapter::Operation),
        ("repo-workflows-detached-detached-act-70dad4a3", "busy") => Some(Adapter::Operation),
        ("repo-workflows-doctor-set-row-921f564b", "scope-choice-scope-to-lowercase-3") => {
            Some(Adapter::Scope)
        }
        ("repo-workflows-doctor-turn-on-action-d8e3e02d", "s-title-to-lowercase-1") => {
            Some(Adapter::Health)
        }
        ("repo-workflows-repository_tools-repository-tool-mutation-988356ee", "busy") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-file_comparison-comparison-pick-28dc27ab", "pick-verb-1") => {
            Some(Adapter::Pick)
        }
        ("repo-workflows-hotfix-hotfix-op-done-fbdef87e", "run-label-1") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-issue_work-start-issue-e2b80c1b", "busy") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-a0e9ad2a", "label") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-a0e9ad2a", "busy") => Some(Adapter::Operation),
        ("repo-workflows-issue_work-refresh-issue-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-35b33376", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-36f5baee", "label") => Some(Adapter::Operation),
        ("repo-workflows-lfs_locks-lock-job-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-partial_stash-stash-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-push-push-op-ended-79357f2e", "label") => Some(Adapter::Operation),
        ("repo-workflows-push-chain-step-ended-dfa32698", "what") => Some(Adapter::Operation),
        ("repo-workflows-push-remote-checked-3bbec358", "banner-headline-r-1") => {
            Some(Adapter::Headline)
        }
        ("repo-workflows-release-release-stopped-a4afd9be", "label") => Some(Adapter::Operation),
        ("repo-workflows-release-release-stopped-a4afd9be", "step-label-1") => {
            Some(Adapter::Operation)
        }
        ("repo-workflows-release-release-stopped-328c7ab5", "label") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-launch-2a66f8c5", "step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-poll-b3badea2", "run-step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-ssh_setup-poll-b5bb4cca", "run-step-label-1") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-stack-op-95c7214f", "label") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-stack-op-95c7214f", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-start-publish-e0c26cc9", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-start-publish-c4ffa0c1", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-publish-done-c4ffa0c1", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-publish-done-6ffb5e8c", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-summary-47eeff3f", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-summary-7f75368f", "noun") => Some(Adapter::Noun),
        ("repo-workflows-stack-run-restack-95c7214f", "label") => Some(Adapter::Operation),
        ("repo-workflows-stack-run-restack-95c7214f", "busy") => Some(Adapter::Operation),
        ("repo-workflows-stack-restack-done-33b4b547", "place") => Some(Adapter::Place),
        ("repo-workflows-surgery-open-absorb-edcc444b", "state-label-to-lowercase-1") => {
            Some(Adapter::InProgress)
        }
        ("repo-workflows-timeline-outside-label-d1d10f56", "first") => Some(Adapter::Operation),
        ("repo-workflows-timeline-fallback-label-4da74cbd", "one-c-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-discard-remove-36f5baee", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-discard-remove-8336fbfe", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-op-ended-be0297e1", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-show-receipt-e172bf9c", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-show-receipt-e172bf9c", "receipt-sentence-1") => {
            Some(Adapter::Receipt)
        }
        ("repo-workflows-timeline-undo-bf959af0", "entry-label-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-redo-2b66f1cb", "entry-label-1") => Some(Adapter::Operation),
        ("repo-workflows-timeline-jump-b77dfff9", "label") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-unsettled-21705388", "what") => Some(Adapter::Operation),
        ("repo-workflows-timeline-timeline-unsettled-3fd5b04c", "what") => Some(Adapter::Operation),
        _ => None,
    }
}

pub(super) fn scope(raw: &str) -> Option<String> {
    let id = match raw {
        "this repository" => "repo-workflows-doctor-scope-choice-38429e22",
        "this worktree" => "repo-workflows-doctor-scope-choice-0eb26325",
        "all my repositories" => "repo-workflows-doctor-scope-choice-59b8031f",
        "everyone on this machine" => "repo-workflows-doctor-scope-choice-ad944dcb",
        "command line" => "repo-workflows-doctor-scope-choice-c925a118",
        _ => return None,
    };
    Some(crate::i18n::text(id).to_owned())
}
pub(super) fn health(raw: &str) -> Option<String> {
    let id = match raw {
        "commit-graph" => "backend-health-commit-graph",
        "scheduled maintenance" => "backend-scheduled-maintenance-f688c9",
        "file system monitor" => "backend-file-system-monitor-bd6322",
        "untracked cache" => "backend-untracked-cache-de0942",
        "pack the objects" => "backend-pack-the-objects-5a853d",
        _ => return None,
    };
    Some(crate::i18n::text(id).to_owned())
}
pub(super) fn noun(raw: &str) -> Option<String> {
    match raw {
        "pull request" => Some(crate::forge::Forge::GitHub.noun_label().to_owned()),
        "merge request" => Some(crate::forge::Forge::GitLab.noun_label().to_owned()),
        _ => None,
    }
}
pub(super) fn place(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return Some(String::new());
    }
    let path = raw.strip_prefix(" in its worktree at ")?;
    Some(crate::i18n::message(
        "repo-workflows-stack-restack-done-6cecc855",
        &[("wt-display-1", path.into())],
    ))
}
pub(super) fn in_progress(raw: &str) -> Option<String> {
    let state = match raw {
        "merge" => crate::git::InProgress::Merge,
        "cherry-pick" => crate::git::InProgress::CherryPick,
        "revert" => crate::git::InProgress::Revert,
        "rebase" => crate::git::InProgress::Rebase,
        "patch" => crate::git::InProgress::Am,
        _ => return None,
    };
    Some(state.label().to_owned())
}

pub(super) fn headline(raw: &str) -> Option<String> {
    let (remote, count, clear) =
        if let Some(remote) = raw.strip_suffix(" has nothing you don't have now: push again") {
            (remote, 0, "yes")
        } else {
            let body = raw.strip_suffix(" you don't have")?;
            let (remote, rest) = body.rsplit_once(" has ")?;
            let (number, plural) = rest.split_once(" commit")?;
            let count = decimal(number)?;
            if plural != if count == 1 { "" } else { "s" } {
                return None;
            }
            (remote, count, "no")
        };
    Some(crate::i18n::message(
        "repo-workflows-push-headline",
        &[
            ("remote", remote.into()),
            ("count", count.into()),
            ("clear", clear.into()),
        ],
    ))
}
fn decimal(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

// Three supplied whole producer variants require explicit selector/default bindings.
// The delimiters are literal application envelopes; every inserted ref/name stays opaque.
pub(super) fn special(raw: &str) -> Option<String> {
    if let Some(body) = raw.strip_suffix(": Ctrl+Z puts them back as they were")
        && let Some((branch, target)) =
            body.split_once("'s commits are signed off here, but not pushed")
    {
        let (target, remote) = if target.is_empty() {
            ("no", "")
        } else {
            ("yes", target.strip_prefix(" to ")?)
        };
        return Some(crate::i18n::message(
            "repo-workflows-status-dco-local-only",
            &[
                ("branch", branch.into()),
                ("target", target.into()),
                ("remote", remote.into()),
            ],
        ));
    }
    let (kept, names) = if raw == "No fork remote can go" {
        ("no", "")
    } else {
        let names = raw
            .strip_prefix("No fork remote can go\n\nKept: ")?
            .strip_suffix(", whose branches have commits no branch here has.")?;
        ("yes", names)
    };
    Some(crate::i18n::message(
        "repo-workflows-status-forks-none-to-remove",
        &[("kept", kept.into()), ("names", names.into())],
    ))
}

/// Finite legacy receipt grammar; unknown or reordered clauses do not become operations.
/// This only adapts the supplied timeline-receipt argument of a recognized status wrapper.
pub(super) fn receipt(raw: &str) -> Option<String> {
    let mut clauses = raw.split(", ");
    let first_id = match clauses.next()? {
        "Final files identical" => "repo-workflows-timeline-sentence-31b36481",
        "Final files differ" => "repo-workflows-timeline-sentence-99e79f74",
        _ => return None,
    };
    let mut rendered = vec![crate::i18n::text(first_id).to_owned()];
    let mut rank = 0;
    let mut signature_replaced = false;
    for clause in clauses {
        if clause == "too many commits to follow one by one" {
            if rank != 0 {
                return None;
            }
            rank = 5;
            rendered
                .push(crate::i18n::text("repo-workflows-timeline-sentence-4a61e684").to_owned());
            continue;
        }
        if signature_replaced {
            for (suffix, id, name) in [
                (
                    " dropped",
                    "repo-workflows-timeline-signatures-b59d95ba",
                    "b-a-1",
                ),
                (
                    " more signed",
                    "repo-workflows-timeline-signatures-33c87627",
                    "a-b-1",
                ),
            ] {
                if let Some(number) = clause
                    .strip_suffix(suffix)
                    .and_then(decimal)
                    .filter(|n| *n > 0)
                {
                    rendered.pop();
                    rendered.push(crate::i18n::message(id, &[(name, number.into())]));
                    signature_replaced = false;
                    break;
                }
            }
            if clause.ends_with(" dropped") || clause.ends_with(" more signed") {
                if signature_replaced {
                    return None;
                }
                continue;
            }
        }
        signature_replaced = false;
        let fixed = match clause {
            "authors kept" => Some((1, "repo-workflows-timeline-sentence-d30ebcc6")),
            "messages kept" => Some((2, "repo-workflows-timeline-sentence-360c0cec")),
            "signatures replaced" => {
                signature_replaced = true;
                Some((3, "repo-workflows-timeline-signatures-aa14a1ba"))
            }
            _ => None,
        };
        let (next_rank, text) = if let Some((next, id)) = fixed {
            (next, crate::i18n::text(id).to_owned())
        } else if let Some((old, new)) = clause.split_once(" commits → ") {
            (
                4,
                crate::i18n::message(
                    "repo-workflows-timeline-sentence-b5ea0271",
                    &[("old", decimal(old)?.into()), ("new", decimal(new)?.into())],
                ),
            )
        } else {
            let mut known = None;
            for (one, many, id, next) in [
                (
                    " author changed",
                    " authors changed",
                    "repo-workflows-timeline-authors-changed",
                    1,
                ),
                (
                    " message changed",
                    " messages changed",
                    "repo-workflows-timeline-messages-changed",
                    2,
                ),
                (
                    " commit now signed",
                    " commits now signed",
                    "repo-workflows-timeline-commits-signed",
                    3,
                ),
                (
                    " signature dropped",
                    " signatures dropped",
                    "repo-workflows-timeline-signatures-dropped",
                    3,
                ),
            ] {
                for (suffix, singular) in [(one, true), (many, false)] {
                    if let Some(number) = clause
                        .strip_suffix(suffix)
                        .and_then(decimal)
                        .filter(|n| *n > 0)
                        && (number == 1) == singular
                    {
                        known = Some((next, crate::i18n::message(id, &[("count", number.into())])));
                    }
                }
            }
            known?
        };
        if next_rank <= rank {
            return None;
        }
        rank = next_rank;
        rendered.push(text);
    }
    let sentence = rendered.join(", ");
    let mut chars = sentence.chars();
    Some(match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => sentence,
    })
}

#[cfg(test)]
mod tests;

pub(super) fn shared_fixed(raw: &str) -> Option<String> {
    match raw {
        "Fetch" => Some(crate::operation_labels::display(raw)),
        "Pull" => Some(crate::operation_labels::display(raw)),
        "Push" => Some(crate::operation_labels::display(raw)),
        "Commit" => Some(crate::operation_labels::display(raw)),
        "Amend" => Some(crate::operation_labels::display(raw)),
        "Stage" => Some(crate::operation_labels::display(raw)),
        "Stage all" => Some(crate::operation_labels::display(raw)),
        "Unstage" => Some(crate::operation_labels::display(raw)),
        "Unstage all" => Some(crate::operation_labels::display(raw)),
        "Discard" => Some(crate::operation_labels::display(raw)),
        "Stash" => Some(crate::operation_labels::display(raw)),
        "Pop stash" => Some(crate::operation_labels::display(raw)),
        "Apply stash" => Some(crate::operation_labels::display(raw)),
        "Drop stash" => Some(crate::operation_labels::display(raw)),
        "Set identity" => Some(crate::operation_labels::display(raw)),
        "Use the profile's author" => Some(crate::operation_labels::display(raw)),
        "Mark resolved" => Some(crate::operation_labels::display(raw)),
        "Reload" => Some(crate::operation_labels::display(raw)),
        "Sync" => Some(crate::operation_labels::display(raw)),
        "Push for review" => Some(crate::operation_labels::display(raw)),
        "Push, skipping hooks" => Some(crate::operation_labels::display(raw)),
        "Apply patch" => Some(crate::operation_labels::display(raw)),
        "Cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Revert" => Some(crate::operation_labels::display(raw)),
        "Rebase" => Some(crate::operation_labels::display(raw)),
        "Merge" => Some(crate::operation_labels::display(raw)),
        "DVC: Pull" => Some(crate::operation_labels::display(raw)),
        "Abort merge" => Some(crate::operation_labels::display(raw)),
        "Abort rebase" => Some(crate::operation_labels::display(raw)),
        "Abort cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Abort revert" => Some(crate::operation_labels::display(raw)),
        "Abort am" => Some(crate::operation_labels::display(raw)),
        "Continue merge" => Some(crate::operation_labels::display(raw)),
        "Continue rebase" => Some(crate::operation_labels::display(raw)),
        "Continue cherry-pick" => Some(crate::operation_labels::display(raw)),
        "Continue revert" => Some(crate::operation_labels::display(raw)),
        "Continue am" => Some(crate::operation_labels::display(raw)),
        _ => None,
    }
}
