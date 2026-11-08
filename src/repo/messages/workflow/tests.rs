use super::*;
use crate::repo::messages::{display_operation, display_status};

#[derive(Clone, Copy)]
enum Sample {
    Number(u64),
    Text(&'static str, &'static str),
}
struct Case {
    raw: &'static str,
    id: &'static str,
    args: &'static [(&'static str, Sample)],
}
const CASES: &[Case] = &[
    Case {
        raw: "Use YYYY-MM-DD dates",
        id: "repo-workflows-atlas-read-atlas-02c42736",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Use YYYY-MM-DD dates, with start on or before end",
        id: "repo-workflows-atlas-read-atlas-67b18cbf",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Use dates such as 2026-11-07",
        id: "repo-workflows-history_activity-requested-range-4d18caf5",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "The start date must be on or before the end date",
        id: "repo-workflows-history_activity-requested-range-25f6acd0",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Analysis cancelled. Refresh to try again.",
        id: "repo-workflows-history_activity-cancel-ddfe1c43",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "No supported forge remote; commit history remains available",
        id: "repo-workflows-line_dossier-poll-line-dossier-45e16671",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history",
        id: "repo-workflows-line_dossier-poll-line-dossier-1cd553a8",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Trace cancelled",
        id: "repo-workflows-line_dossier-line-dossier-view-a8198076",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Repository changed. Refresh the file view.",
        id: "repo-workflows-file_comparison-invalidate-81f92ce0",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Invalid comparison sides",
        id: "repo-workflows-file_comparison-prepare-da3978aa",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "More than 20,000 lines in a version; use the normal patch view",
        id: "repo-workflows-file_comparison-boundary-e44610d7",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Git could not align these raw versions",
        id: "repo-workflows-file_comparison-boundary-b861cbb0",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "The raw patch is not UTF-8",
        id: "repo-workflows-file_comparison-boundary-5421cc1b",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Git could not produce a raw text patch for these differing bytes; use the normal patch view",
        id: "repo-workflows-file_comparison-boundary-6732715f",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Comparison cancelled. Refresh to try again.",
        id: "repo-workflows-file_comparison-file-comparison-view-725513de",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "No key, agent, account or network action has run. Choose each step explicitly.",
        id: "repo-workflows-ssh_setup-new-8633c172",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Setup worker stopped; no later result will be applied.",
        id: "repo-workflows-ssh_setup-poll-380743c6",
        args: &[],
    }, // retained R1 finite contract
    Case {
        raw: "Cut release raw-tag/1234",
        id: "repo-workflows-release-release-stopped-fbe3bbf7",
        args: &[(
            "run-plan-tag-1",
            Sample::Text("raw-tag/1234", "raw-tag/1234"),
        )],
    }, // retained R1 finite contract
    Case {
        raw: "Clear the flag of raw/path:1234",
        id: "repo-workflows-doctor-clear-7f76d2cf",
        args: &[("one-path-1", Sample::Text("raw/path:1234", "raw/path:1234"))],
    }, // retained R1 finite contract
    Case {
        raw: "Clear the flags of 1234 files",
        id: "repo-workflows-doctor-clear-e318945b",
        args: &[("files-len-1", Sample::Number(1234))],
    }, // retained R1 finite contract
    Case {
        raw: "Could not start the selected machine's terminal/tool. Check that OpenSSH/WSL is installed. A reserved new folder, if any, is retained; existing files were not overwritten.",
        id: "repo-workflows-ssh_setup-launch-0690e36b",
        args: &[],
    }, // supplied entry 202
    Case {
        raw: "Revision appended to .git-blame-ignore-revs; staged contents preserved. Ordinary Git blame can use blame.ignoreRevsFile=.git-blame-ignore-revs.",
        id: "repo-workflows-line_dossier-dossier-ignore-finished-6e1731ff",
        args: &[],
    }, // supplied entry 147
    Case {
        raw: "Push exited 1234. Read the terminal's explanation. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine.",
        id: "repo-workflows-ssh_setup-poll-b5bb4cca",
        args: &[
            ("run-step-label-1", Sample::Text("Push", "Pushen")),
            ("code", Sample::Number(1234)),
        ],
    }, // supplied entry 209
    Case {
        raw: "SSH test exited 1234. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled.",
        id: "repo-workflows-ssh_setup-poll-acae7792",
        args: &[("code", Sample::Number(1234))],
    }, // supplied entry 207
    Case {
        raw: "This profile now selects that local SSH key. Author/signing and repository-local SSH overrides retain their existing precedence.",
        id: "repo-workflows-ssh_setup-poll-77510276",
        args: &[],
    }, // supplied entry 206
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
    }, // supplied entry 232
    Case {
        raw: "Approving runs the contributor's code on your CI: look through #opaque-number/Stage1/日本語/00001234's changes first, then choose Approve and run again",
        id: "repo-workflows-forks-approve-runs-a77eca22",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 123
    Case {
        raw: "Preview ready. The exported PNG contains exactly this composition; ellipses mark shortened or width-limited text.",
        id: "repo-workflows-postcards-poll-postcards-5358001c",
        args: &[],
    }, // supplied entry 160
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
    }, // supplied entry 18
    Case {
        raw: "The active profile or its connection choices changed. Check the intended account again before uploading.",
        id: "repo-workflows-ssh_setup-poll-b927d5a6",
        args: &[],
    }, // supplied entry 203
    Case {
        raw: "The selected public key was registered. This does not enable commit signing or change a global account.",
        id: "repo-workflows-ssh_setup-poll-cf037753",
        args: &[],
    }, // supplied entry 205
    Case {
        raw: "SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions.",
        id: "repo-workflows-ssh_setup-open-ssh-setup-0e2de7fb",
        args: &[],
    }, // supplied entry 210
    Case {
        raw: "Dependencies finished, but the parent repository, profile or sign-in changed. Prepare its push again.",
        id: "repo-workflows-repository_tools-dependency-job-a7b1dc6d",
        args: &[],
    }, // supplied entry 91
    Case {
        raw: "The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it (or abort it)",
        id: "repo-workflows-forks-forks-op-done-9c338e90",
        args: &[],
    }, // supplied entry 127
    Case {
        raw: "Push command completed successfully. Key files are retained; continue with the next explicit step.",
        id: "repo-workflows-ssh_setup-poll-b3badea2",
        args: &[("run-step-label-1", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 208
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
    }, // supplied entry 219
    Case {
        raw: "Pinned release range loaded. Review the disclosure list, then compose the PNG preview.",
        id: "repo-workflows-postcards-poll-postcards-60ccb94f",
        args: &[],
    }, // supplied entry 159
    Case {
        raw: "Push: that would skip “opaque-skipped/Stage1/日本語/00001234”, which moved no branch: step through it with opaque-cmd/Stage1/日本語/00001234+Z / opaque-cmd/Stage1/日本語/00001234+Y instead",
        id: "repo-workflows-timeline-jump-b77dfff9",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            (
                "skipped",
                Sample::Text(
                    "opaque-skipped/Stage1/日本語/00001234",
                    "opaque-skipped/Stage1/日本語/00001234",
                ),
            ),
            (
                "cmd",
                Sample::Text(
                    "opaque-cmd/Stage1/日本語/00001234",
                    "opaque-cmd/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 290
    Case {
        raw: "This repository isn't on a forge gitcraft knows: there is nowhere to publish to.",
        id: "repo-workflows-stack-open-publish-a23e7f8b",
        args: &[],
    }, // supplied entry 214
    Case {
        raw: "Didn't check out opaque-leave-label-1/Stage1/日本語/00001234: couldn't list the commits it would leave on no branch (opaque-e/Stage1/日本語/00001234)",
        id: "repo-workflows-detached-leave-to-48b0ce4c",
        args: &[
            (
                "leave-label-1",
                Sample::Text(
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 53
    Case {
        raw: "Go online and wait for the current operation before preparing an outbox push",
        id: "repo-workflows-repository_tools-act-d64d1e25",
        args: &[],
    }, // supplied entry 95
    Case {
        raw: "Profile changed. Read the release range again before composing a postcard.",
        id: "repo-workflows-postcards-poll-postcards-4773e588",
        args: &[],
    }, // supplied entry 158
    Case {
        raw: "Finish or close the conflict or the rebase editor first to see the timeline",
        id: "repo-workflows-timeline-toggle-timeline-d12d6347",
        args: &[],
    }, // supplied entry 297
    Case {
        raw: "What changed in #opaque-number/Stage1/日本語/00001234: it isn't in the list of this repository's pull requests",
        id: "repo-workflows-versions-open-pr-since-1e0c2674",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 319
    Case {
        raw: "Commit or stash your changes first: the rehearsal was of the commits alone",
        id: "repo-workflows-rehearse-apply-rehearsal-b09a6f3f",
        args: &[],
    }, // supplied entry 187
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
    }, // supplied entry 234
    Case {
        raw: "Nothing to commit yet: add or change files in the repository's folder",
        id: "repo-workflows-guide-start-first-commit-881f72cb",
        args: &[],
    }, // supplied entry 130
    Case {
        raw: "Mark as seen: #opaque-number/Stage1/日本語/00001234 isn't in the list of this repository's pull requests",
        id: "repo-workflows-versions-mark-pr-seen-93fe69bf",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 334
    Case {
        raw: "Prepared subtree profile/sign-in changed; prepare and approve again",
        id: "repo-workflows-repository_tools-act-d021ec81",
        args: &[],
    }, // supplied entry 100
    Case {
        raw: "opaque-plan-local-1/Stage1/日本語/00001234 is rebased here, but not pushed to opaque-plan-fork-2/Stage1/日本語/00001234: Ctrl+Z puts it back as it was",
        id: "repo-workflows-forks-forks-op-done-8287ad6b",
        args: &[
            (
                "plan-local-1",
                Sample::Text(
                    "opaque-plan-local-1/Stage1/日本語/00001234",
                    "opaque-plan-local-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "plan-fork-2",
                Sample::Text(
                    "opaque-plan-fork-2/Stage1/日本語/00001234",
                    "opaque-plan-fork-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 128
    Case {
        raw: "The last scan is still ending on that machine; try again in a moment",
        id: "repo-workflows-reflog-start-scan-7a1d64fe",
        args: &[],
    }, // supplied entry 183
    Case {
        raw: "Finish or close the conflict editor or the interactive rebase first",
        id: "repo-workflows-doctor-open-doctor-745454bd",
        args: &[],
    }, // supplied entry 63
    Case {
        raw: "Prepared outbox profile/sign-in changed; prepare and approve again",
        id: "repo-workflows-repository_tools-start-send-628a48e6",
        args: &[],
    }, // supplied entry 96
    Case {
        raw: "Distribute staged fixes: HEAD is detached, check out a branch first",
        id: "repo-workflows-surgery-open-absorb-3373a6f7",
        args: &[],
    }, // supplied entry 235
    Case {
        raw: "Distribute staged fixes: nothing is staged, stage the fixes first",
        id: "repo-workflows-surgery-open-absorb-55d2e258",
        args: &[],
    }, // supplied entry 237
    Case {
        raw: "HEAD is detached: check out a branch to make the first commit on",
        id: "repo-workflows-guide-ask-empty-commit-348d9f75",
        args: &[],
    }, // supplied entry 129
    Case {
        raw: "Push is running. Enter any passphrase only in the terminal below.",
        id: "repo-workflows-ssh_setup-launch-2a66f8c5",
        args: &[("step-label-1", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 201
    Case {
        raw: "Distribute staged fixes: close the editor that is open first",
        id: "repo-workflows-surgery-open-absorb-2cddd12d",
        args: &[],
    }, // supplied entry 238
    Case {
        raw: "Push: the repository couldn't be read again; reload it (F5) first",
        id: "repo-workflows-timeline-timeline-unsettled-3fd5b04c",
        args: &[("what", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 292
    Case {
        raw: "Revision already present; ignore file and index unchanged",
        id: "repo-workflows-line_dossier-dossier-ignore-finished-366a809a",
        args: &[],
    }, // supplied entry 148
    Case {
        raw: "Nothing to restack: every layer is on its parent's tip.",
        id: "repo-workflows-stack-open-restack-419e0d8e",
        args: &[],
    }, // supplied entry 222
    Case {
        raw: "Distribute staged fixes: another editor opened meanwhile",
        id: "repo-workflows-surgery-open-absorb-55f28458",
        args: &[],
    }, // supplied entry 240
    Case {
        raw: "UnityYAMLMerge wasn't found: choose it in Unity set-up…",
        id: "repo-workflows-unity-smart-merge-conflict-5a10252d",
        args: &[],
    }, // supplied entry 311
    Case {
        raw: "History summary cancelled or exceeded its output limit",
        id: "repo-workflows-line_dossier-run-a9f1b338",
        args: &[],
    }, // supplied entry 151
    Case {
        raw: "Moved-code analysis cancelled or exceeded 40 seconds",
        id: "repo-workflows-file_comparison-read-431f5828",
        args: &[],
    }, // supplied entry 112
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
    }, // supplied entry 204
    Case {
        raw: "Restack: every branch was already on its parent's tip",
        id: "repo-workflows-stack-restack-done-1fc5c0ba",
        args: &[],
    }, // supplied entry 227
    Case {
        raw: "Push refused, and the remote could not be checked: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-push-remote-checked-c6e528c9",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 181
    Case {
        raw: "UnityYAMLMerge merged opaque-path/Stage1/日本語/00001234: check the result, then save",
        id: "repo-workflows-unity-smart-merge-done-afe126d0",
        args: &[(
            "path",
            Sample::Text(
                "opaque-path/Stage1/日本語/00001234",
                "opaque-path/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 312
    Case {
        raw: "Restack aborted: every branch is back where it was",
        id: "repo-workflows-stack-restack-done-a7222989",
        args: &[],
    }, // supplied entry 231
    Case {
        raw: "Push: done, but its earlier version couldn't be kept: opaque-detail/Stage1/日本語/00001234",
        id: "repo-workflows-status-rewrite-retention",
        args: &[
            ("operation", Sample::Text("Push", "Pushen")),
            (
                "detail",
                Sample::Text(
                    "opaque-detail/Stage1/日本語/00001234",
                    "opaque-detail/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 350
    Case {
        raw: "Restack: 1234 branches moved · Ctrl+Z puts them back",
        id: "repo-workflows-stack-restack-done-7f6cce25",
        args: &[("n", Sample::Number(1234))],
    }, // supplied entry 229
    Case {
        raw: "Split commit: close the staged fixes editor first",
        id: "repo-workflows-surgery-open-split-01ee6f3c",
        args: &[],
    }, // supplied entry 248
    Case {
        raw: "Pick a name no branch has for the new branch (not “opaque-name/Stage1/日本語/00001234”)",
        id: "repo-workflows-timeline-run-remove-6fd826a1",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 276
    Case {
        raw: "Couldn't save Sign off for this repository (git config opaque-signoff-sign-off-key-1/Stage1/日本語/00001234): opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-dco-dco-act-10fe4a7c",
        args: &[
            (
                "signoff-sign-off-key-1",
                Sample::Text(
                    "opaque-signoff-sign-off-key-1/Stage1/日本語/00001234",
                    "opaque-signoff-sign-off-key-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 47
    Case {
        raw: "Tracker account changed; reload the issue source",
        id: "repo-workflows-issue_work-start-issue-563140a4",
        args: &[],
    }, // supplied entry 134
    Case {
        raw: "A parallel bisect is already running in this tab",
        id: "repo-workflows-parallel_bisect-start-parallel-3450e92d",
        args: &[],
    }, // supplied entry 153
    Case {
        raw: "Convert legacy grafts to removable replace refs",
        id: "repo-workflows-repository_tools-repository-tools-act-ad133eb8",
        args: &[],
    }, // supplied entry 85
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
    }, // supplied entry 21
    Case {
        raw: "The pull request's branch is not available yet",
        id: "repo-workflows-pr_view-select-pr-b788526c",
        args: &[],
    }, // supplied entry 164
    Case {
        raw: "Restack: 1 branch moved · Ctrl+Z puts it back",
        id: "repo-workflows-stack-restack-done-dbfb6ce0",
        args: &[],
    }, // supplied entry 228
    Case {
        raw: "Push: waiting for the repository to be read again…",
        id: "repo-workflows-timeline-timeline-unsettled-21705388",
        args: &[("what", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 291
    Case {
        raw: "Push: done. Commit .gitattributes to share it.",
        id: "repo-workflows-lfs_locks-lock-job-35b33376",
        args: &[("label", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 141
    Case {
        raw: "Choose one saved turn to preview its inverse",
        id: "repo-workflows-repository_tools-act-6dc9a943",
        args: &[],
    }, // supplied entry 102
    Case {
        raw: "This repository has no remote to rebase onto",
        id: "repo-workflows-forks-plan-fork-rebase-1abac341",
        args: &[],
    }, // supplied entry 119
    Case {
        raw: "UnityYAMLMerge wasn't found: choose it first",
        id: "repo-workflows-unity-register-driver-f6c55b48",
        args: &[],
    }, // supplied entry 309
    Case {
        raw: "UnityYAMLMerge left 1234 conflicts in opaque-path/Stage1/日本語/00001234 to choose",
        id: "repo-workflows-unity-smart-merge-done-7e870a3e",
        args: &[
            ("n", Sample::Number(1234)),
            (
                "path",
                Sample::Text(
                    "opaque-path/Stage1/日本語/00001234",
                    "opaque-path/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 313
    Case {
        raw: "Distribute staged fixes: a merge is in progress",
        id: "repo-workflows-surgery-open-absorb-edcc444b",
        args: &[(
            "state-label-to-lowercase-1",
            Sample::Text("Merge", "Zusammenführen"),
        )],
    }, // supplied entry 236
    Case {
        raw: "Tests are running already: stop them first",
        id: "repo-workflows-test_lane-start-tests-aa89f17f",
        args: &[],
    }, // supplied entry 252
    Case {
        raw: "Couldn't remember the version of #opaque-number/Stage1/日本語/00001234 seen: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-versions-seen-marked-f9ed1e57",
        args: &[
            (
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            ),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 333
    Case {
        raw: "Wait for Push to finish, then check out again",
        id: "repo-workflows-detached-detached-act-70dad4a3",
        args: &[("busy", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 54
    Case {
        raw: "Publish dependencies before parent review",
        id: "repo-workflows-repository_tools-dependency-job-f48f9c31",
        args: &[],
    }, // supplied entry 89
    Case {
        raw: "Wait for the operation going on to finish",
        id: "repo-workflows-parallel_bisect-mark-parallel-result-e5e3ecc5",
        args: &[],
    }, // supplied entry 154
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
    }, // supplied entry 188
    Case {
        raw: "Move changes from opaque-short-hash-1/Stage1/日本語/00001234 to the working changes",
        id: "repo-workflows-surgery-run-move-afb8b0ad",
        args: &[(
            "short-hash-1",
            Sample::Text(
                "opaque-short-hash-1/Stage1/日本語/00001234",
                "opaque-short-hash-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 246
    Case {
        raw: "Wait for the running operation to finish",
        id: "repo-workflows-agents-start-agent-task-f80416ff",
        args: &[],
    }, // supplied entry 24
    Case {
        raw: "Check submodule publication dependencies",
        id: "repo-workflows-repository_tools-dependency-job-57895189",
        args: &[],
    }, // supplied entry 90
    Case {
        raw: "This repository has no remote to push to",
        id: "repo-workflows-push-push-labelled-4933a6c9",
        args: &[],
    }, // supplied entry 172
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
    }, // supplied entry 215
    Case {
        raw: "opaque-tag/Stage1/日本語/00001234 isn't a version to start a hotfix from",
        id: "repo-workflows-hotfix-hotfix-act-06b55025",
        args: &[(
            "tag",
            Sample::Text(
                "opaque-tag/Stage1/日本語/00001234",
                "opaque-tag/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 131
    Case {
        raw: "#opaque-number/Stage1/日本語/00001234 hasn't changed since you last looked",
        id: "repo-workflows-versions-open-pr-since-f8a6567c",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 321
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
    }, // supplied entry 22
    Case {
        raw: "This repository has no CODEOWNERS file",
        id: "repo-workflows-owners-owners-act-9cd4150e",
        args: &[],
    }, // supplied entry 146
    Case {
        raw: "Check out a branch to rehearse with it",
        id: "repo-workflows-rehearse-start-rehearsal-252af634",
        args: &[],
    }, // supplied entry 185
    Case {
        raw: "Didn't check out opaque-leave-label-1/Stage1/日本語/00001234: Push started meanwhile",
        id: "repo-workflows-detached-leave-to-007a4c0e",
        args: &[
            (
                "leave-label-1",
                Sample::Text(
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                ),
            ),
            ("busy", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 52
    Case {
        raw: "Wait for the running operation to end",
        id: "repo-workflows-doctor-doctor-act-9bab8d95",
        args: &[],
    }, // supplied entry 64
    Case {
        raw: "Change virtual branch materialization",
        id: "repo-workflows-repository_tools-act-ce061d46",
        args: &[],
    }, // supplied entry 105
    Case {
        raw: "Couldn't approve #opaque-number/Stage1/日本語/00001234's workflow runs: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-forks-approve-runs-282082c7",
        args: &[
            (
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            ),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 126
    Case {
        raw: "Wait for the operation running to end",
        id: "repo-workflows-release-run-release-d9523bf2",
        args: &[],
    }, // supplied entry 189
    Case {
        raw: "Autosend opted-in plain outbox push",
        id: "repo-workflows-repository_tools-start-send-e7b0c78d",
        args: &[],
    }, // supplied entry 97
    Case {
        raw: "Couldn't read the branches' issues: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-issue_work-take-issue-read-bbcbbb59",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 133
    Case {
        raw: "Distribute staged fixes into 1234 commits",
        id: "repo-workflows-operation-distribute-staged",
        args: &[("count", Sample::Number(1234))],
    }, // supplied entry 349
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
    }, // supplied entry 28
    Case {
        raw: "Join imported histories temporarily",
        id: "repo-workflows-repository_tools-repository-tools-act-a7b0c90c",
        args: &[],
    }, // supplied entry 83
    Case {
        raw: "Where #opaque-number/Stage1/日本語/00001234 comes from isn't known yet",
        id: "repo-workflows-forks-checkout-fork-5ecb3bac",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 118
    Case {
        raw: "Publish: run it again once Push is done",
        id: "repo-workflows-stack-start-publish-e0c26cc9",
        args: &[("busy", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 216
    Case {
        raw: "Taking generated folders out of git",
        id: "repo-workflows-unity-untrack-generated-b0c9a9c5",
        args: &[],
    }, // supplied entry 306
    Case {
        raw: "Fetching #opaque-number/Stage1/日本語/00001234's branch and its base…",
        id: "repo-workflows-forks-plan-fork-rebase-ac71056e",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 120
    Case {
        raw: "Appending reviewed ignore revision",
        id: "repo-workflows-line_dossier-append-dossier-ignore-716419f1",
        args: &[],
    }, // supplied entry 327
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
    }, // supplied entry 19
    Case {
        raw: "Connection changed; reopen outbox",
        id: "repo-workflows-repository_tools-act-ccd2fdda",
        args: &[],
    }, // supplied entry 92
    Case {
        raw: "Commit virtual branch owned hunks",
        id: "repo-workflows-repository_tools-act-de986fa8",
        args: &[],
    }, // supplied entry 107
    Case {
        raw: "Checking against the agent fence…",
        id: "repo-workflows-fence_view-fence-gate-2b8e4f3b",
        args: &[],
    }, // supplied entry 111
    Case {
        raw: "Distribute staged fixes: reading…",
        id: "repo-workflows-surgery-open-absorb-5ebbefda",
        args: &[],
    }, // supplied entry 239
    Case {
        raw: "Push the signed-off commits of opaque-plan-branch-1/Stage1/日本語/00001234",
        id: "repo-workflows-dco-dco-op-done-55d2b32b",
        args: &[(
            "plan-branch-1",
            Sample::Text(
                "opaque-plan-branch-1/Stage1/日本語/00001234",
                "opaque-plan-branch-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 49
    Case {
        raw: "Checking against the agent fence",
        id: "repo-workflows-fence_view-fence-gate-6b730ef5",
        args: &[],
    }, // supplied entry 110
    Case {
        raw: "Moved-code comparison cancelled",
        id: "repo-workflows-file_comparison-assemble-56abde67",
        args: &[],
    }, // supplied entry 114
    Case {
        raw: "Finish or close the editor first",
        id: "repo-workflows-file_comparison-open-file-comparison-a6cb5dc1",
        args: &[],
    }, // supplied entry 116
    Case {
        raw: "Could not set what LFS fetches: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-lfs_storage-fetch-only-e3be5d1f",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 145
    Case {
        raw: "Push failed, so nothing was pushed: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-push-chain-step-ended-dfa32698",
        args: &[
            ("what", Sample::Text("Push", "Pushen")),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 177
    Case {
        raw: "Working on the selected machine…",
        id: "repo-workflows-ssh_setup-background-e4e2ef57",
        args: &[],
    }, // supplied entry 200
    Case {
        raw: "Cancelled before terminal start",
        id: "repo-workflows-devcontainer-terminal-f171832a",
        args: &[],
    }, // supplied entry 58
    Case {
        raw: "Reconcile exact replay receipts",
        id: "repo-workflows-repository_tools-act-c08c65bd",
        args: &[],
    }, // supplied entry 70
    Case {
        raw: "#opaque-number/Stage1/日本語/00001234's workflow runs are approved",
        id: "repo-workflows-forks-approve-runs-a151ad19",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 125
    Case {
        raw: "PNG saved as a new local file.",
        id: "repo-workflows-postcards-poll-postcards-10867442",
        args: &[],
    }, // supplied entry 161
    Case {
        raw: "A rehearsal is going on already",
        id: "repo-workflows-rehearse-start-rehearsal-bbfc5a1d",
        args: &[],
    }, // supplied entry 184
    Case {
        raw: "Adding lines to .gitattributes",
        id: "repo-workflows-unity-add-attributes-0b9b6295",
        args: &[],
    }, // supplied entry 304
    Case {
        raw: "Checkout opaque-leave-label-1/Stage1/日本語/00001234: wait for Push to finish",
        id: "repo-workflows-detached-leave-to-28ae5898",
        args: &[
            (
                "leave-label-1",
                Sample::Text(
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                    "opaque-leave-label-1/Stage1/日本語/00001234",
                ),
            ),
            ("busy", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 51
    Case {
        raw: "$opaque-name/Stage1/日本語/00001234 can't hold a NUL character",
        id: "repo-workflows-custom_commands-run-custom-44771367",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 46
    Case {
        raw: "Undo accepted isolated replay",
        id: "repo-workflows-repository_tools-act-346c061a",
        args: &[],
    }, // supplied entry 67
    Case {
        raw: "Save virtual branch ownership",
        id: "repo-workflows-repository_tools-act-901dccbe",
        args: &[],
    }, // supplied entry 104
    Case {
        raw: "Undo virtual branch operation",
        id: "repo-workflows-repository_tools-act-63403bc6",
        args: &[],
    }, // supplied entry 106
    Case {
        raw: "Approving #opaque-number/Stage1/日本語/00001234's workflow runs…",
        id: "repo-workflows-forks-approve-runs-2f659b28",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 124
    Case {
        raw: "Forget the issue of opaque-branch/Stage1/日本語/00001234 failed: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-issue_work-unlink-issue-6df1900e",
        args: &[
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
        ],
    }, // supplied entry 139
    Case {
        raw: "Type the command to test with",
        id: "repo-workflows-test_lane-start-tests-4b88f5c5",
        args: &[],
    }, // supplied entry 253
    Case {
        raw: "Add selected SSH key to agent",
        id: "repo-workflows-operation-ssh-14022",
        args: &[],
    }, // supplied entry 354
    Case {
        raw: "Register public SSH key there",
        id: "repo-workflows-operation-ssh-14048",
        args: &[],
    }, // supplied entry 358
    Case {
        raw: "Replay in isolated worktrees",
        id: "repo-workflows-repository_tools-act-5bd57c48",
        args: &[],
    }, // supplied entry 65
    Case {
        raw: "Postcard rendering cancelled",
        id: "repo-workflows-postcards-render-8e2ed023",
        args: &[],
    }, // supplied entry 165
    Case {
        raw: "Push: run it again once Push is done",
        id: "repo-workflows-stack-run-stack-op-95c7214f",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            ("busy", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 213
    Case {
        raw: "Worktree setup recipe saved",
        id: "repo-workflows-agents-save-recipe-97381b52",
        args: &[],
    }, // supplied entry 26
    Case {
        raw: "Installing the agent hooks…",
        id: "repo-workflows-agents-install-hooks-2f5d6dc4",
        args: &[],
    }, // supplied entry 27
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
    }, // supplied entry 29
    Case {
        raw: "Publish exact subtree split",
        id: "repo-workflows-repository_tools-act-494336ed",
        args: &[],
    }, // supplied entry 101
    Case {
        raw: "Couldn't read the remotes: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-forks-plan-tidy-adf73b14",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 121
    Case {
        raw: "Mark in the bisect failed: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-parallel_bisect-mark-parallel-result-48f3588b",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 156
    Case {
        raw: "Install local Git LFS hooks",
        id: "repo-workflows-tour-tour-view-952aa9de",
        args: &[],
    }, // supplied entry 301
    Case {
        raw: "Adding lines to .gitignore",
        id: "repo-workflows-unity-add-ignores-52181c32",
        args: &[],
    }, // supplied entry 305
    Case {
        raw: "Test selected SSH key there",
        id: "repo-workflows-operation-ssh-14038",
        args: &[],
    }, // supplied entry 357
    Case {
        raw: "Finish or close the opaque-editor/Stage1/日本語/00001234 first",
        id: "repo-workflows-ci_view-open-ci-991ae978",
        args: &[(
            "editor",
            Sample::Text(
                "opaque-editor/Stage1/日本語/00001234",
                "opaque-editor/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 43
    Case {
        raw: "Sign off the commits of opaque-plan-branch-1/Stage1/日本語/00001234 (here only)",
        id: "repo-core-operation-dco-here",
        args: &[(
            "branch",
            Sample::Text(
                "opaque-plan-branch-1/Stage1/日本語/00001234",
                "opaque-plan-branch-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 48
    Case {
        raw: "Remove unused fork remotes",
        id: "repo-workflows-forks-plan-tidy-a7e148ae",
        args: &[],
    }, // supplied entry 122
    Case {
        raw: "Registering UnityYAMLMerge",
        id: "repo-workflows-unity-register-driver-5d4e5077",
        args: &[],
    }, // supplied entry 310
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
    }, // supplied entry 332
    Case {
        raw: "Add SSH key to agent there",
        id: "repo-workflows-operation-ssh-14024",
        args: &[],
    }, // supplied entry 355
    Case {
        raw: "Cancelled before starting",
        id: "repo-workflows-devcontainer-devcontainer-act-84f8f92f",
        args: &[],
    }, // supplied entry 56
    Case {
        raw: "Resolve submodule pointer",
        id: "repo-workflows-repository_tools-repository-tools-act-66f98362",
        args: &[],
    }, // supplied entry 78
    Case {
        raw: "Save pull and push routes",
        id: "repo-workflows-repository_tools-repository-tools-act-82d14313",
        args: &[],
    }, // supplied entry 79
    Case {
        raw: "Save remote configuration",
        id: "repo-workflows-repository_tools-repository-tools-act-4e161156",
        args: &[],
    }, // supplied entry 81
    Case {
        raw: "Start work on opaque-issue-key-1/Stage1/日本語/00001234: wait for Push",
        id: "repo-workflows-issue_work-start-issue-e2b80c1b",
        args: &[
            (
                "issue-key-1",
                Sample::Text(
                    "opaque-issue-key-1/Stage1/日本語/00001234",
                    "opaque-issue-key-1/Stage1/日本語/00001234",
                ),
            ),
            ("busy", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 135
    Case {
        raw: "Postcard work cancelled.",
        id: "repo-workflows-postcards-postcard-window-b6333aff",
        args: &[],
    }, // supplied entry 162
    Case {
        raw: "Distribute staged fixes: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-surgery-open-absorb-ed3b6cd2",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 241
    Case {
        raw: "Go to before the timeline",
        id: "repo-workflows-timeline-jump-target-bd63e54c",
        args: &[],
    }, // supplied entry 294
    Case {
        raw: "Sign off the commits of opaque-plan-branch-1/Stage1/日本語/00001234",
        id: "repo-core-operation-dco",
        args: &[(
            "branch",
            Sample::Text(
                "opaque-plan-branch-1/Stage1/日本語/00001234",
                "opaque-plan-branch-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 50
    Case {
        raw: "Continue isolated replay",
        id: "repo-workflows-repository_tools-act-65d162a8",
        args: &[],
    }, // supplied entry 66
    Case {
        raw: "Sync: opaque-current/Stage1/日本語/00001234 pulled and pushed",
        id: "repo-workflows-push-sync-027fe2bd",
        args: &[(
            "current",
            Sample::Text(
                "opaque-current/Stage1/日本語/00001234",
                "opaque-current/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 174
    Case {
        raw: "opaque-r-branch-1/Stage1/日本語/00001234 pulled from opaque-r-target-label-2/Stage1/日本語/00001234 and pushed",
        id: "repo-workflows-push-push-act-ebd6e3a6",
        args: &[
            (
                "r-branch-1",
                Sample::Text(
                    "opaque-r-branch-1/Stage1/日本語/00001234",
                    "opaque-r-branch-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "r-target-label-2",
                Sample::Text(
                    "opaque-r-target-label-2/Stage1/日本語/00001234",
                    "opaque-r-target-label-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 175
    Case {
        raw: "Publish pull requests: nothing to do",
        id: "repo-workflows-stack-summary-47eeff3f",
        args: &[("noun", Sample::Text("pull request", "Pull Request"))],
    }, // supplied entry 220
    Case {
        raw: "Rebase #opaque-number/Stage1/日本語/00001234 onto opaque-base/Stage1/日本語/00001234 and push",
        id: "repo-workflows-operation-rebase-fork-push",
        args: &[
            (
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            ),
            (
                "base",
                Sample::Text(
                    "opaque-base/Stage1/日本語/00001234",
                    "opaque-base/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 337
    Case {
        raw: "Dev container operation",
        id: "repo-workflows-devcontainer-devcontainer-act-519a489a",
        args: &[],
    }, // supplied entry 57
    Case {
        raw: "Close isolated proposal",
        id: "repo-workflows-repository_tools-act-08cd49af",
        args: &[],
    }, // supplied entry 69
    Case {
        raw: "Unshelve one changelist",
        id: "repo-workflows-repository_tools-act-551afaa9",
        args: &[],
    }, // supplied entry 75
    Case {
        raw: "History worker panicked",
        id: "repo-workflows-saved_views-poll-33647b3e",
        args: &[],
    }, // supplied entry 195
    Case {
        raw: "Move changes: reading opaque-short-hash-1/Stage1/日本語/00001234…",
        id: "repo-workflows-surgery-open-move-9a3c16a9",
        args: &[(
            "short-hash-1",
            Sample::Text(
                "opaque-short-hash-1/Stage1/日本語/00001234",
                "opaque-short-hash-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 243
    Case {
        raw: "Split commit: reading opaque-short-hash-1/Stage1/日本語/00001234…",
        id: "repo-workflows-surgery-open-split-a472a361",
        args: &[(
            "short-hash-1",
            Sample::Text(
                "opaque-short-hash-1/Stage1/日本語/00001234",
                "opaque-short-hash-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 249
    Case {
        raw: "Before the oldest entry",
        id: "repo-workflows-timeline-tree-rows-6fa556f4",
        args: &[],
    }, // supplied entry 298
    Case {
        raw: "File history cancelled",
        id: "repo-workflows-atlas-new-70ba4563",
        args: &[],
    }, // supplied entry 30
    Case {
        raw: "Can't start a bisect: opaque-why/Stage1/日本語/00001234",
        id: "repo-workflows-bisect-start-bisect-5c82e3e0",
        args: &[(
            "why",
            Sample::Text(
                "opaque-why/Stage1/日本語/00001234",
                "opaque-why/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 34
    Case {
        raw: "Accept isolated replay",
        id: "repo-workflows-repository_tools-act-b64c091a",
        args: &[],
    }, // supplied entry 68
    Case {
        raw: "Make clone independent",
        id: "repo-workflows-repository_tools-repository-tools-act-07fb4089",
        args: &[],
    }, // supplied entry 77
    Case {
        raw: "Save publication group",
        id: "repo-workflows-repository_tools-repository-tools-act-3c1c7a3a",
        args: &[],
    }, // supplied entry 80
    Case {
        raw: "Send exact queued push",
        id: "repo-workflows-repository_tools-start-send-994ea6b0",
        args: &[],
    }, // supplied entry 98
    Case {
        raw: "Push refused: opaque-target-label-1/Stage1/日本語/00001234 is gone",
        id: "repo-workflows-push-remote-checked-205a41fa",
        args: &[(
            "target-label-1",
            Sample::Text(
                "opaque-target-label-1/Stage1/日本語/00001234",
                "opaque-target-label-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 180
    Case {
        raw: "History worker stopped",
        id: "repo-workflows-saved_views-poll-da12ac51",
        args: &[],
    }, // supplied entry 193
    Case {
        raw: "History read cancelled",
        id: "repo-workflows-saved_views-poll-dc101692",
        args: &[],
    }, // supplied entry 194
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
    }, // supplied entry 212
    Case {
        raw: "Move changes from opaque-short-hash-1/Stage1/日本語/00001234 to opaque-short-t-hash-2/Stage1/日本語/00001234",
        id: "repo-workflows-surgery-run-move-b5726a1d",
        args: &[
            (
                "short-hash-1",
                Sample::Text(
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "short-t-hash-2",
                Sample::Text(
                    "opaque-short-t-hash-2/Stage1/日本語/00001234",
                    "opaque-short-t-hash-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 245
    Case {
        raw: "LFS: Prune old objects",
        id: "repo-workflows-operation-lfs-prune",
        args: &[],
    }, // supplied entry 346
    Case {
        raw: "No bisect is going on",
        id: "repo-workflows-bisect-mark-bisect-a1c6c771",
        args: &[],
    }, // supplied entry 36
    Case {
        raw: "Commit one changelist",
        id: "repo-workflows-repository_tools-act-c1eec3a9",
        args: &[],
    }, // supplied entry 74
    Case {
        raw: "Shelve one changelist",
        id: "repo-workflows-repository_tools-act-bea96408",
        args: &[],
    }, // supplied entry 76
    Case {
        raw: "Revert one saved turn",
        id: "repo-workflows-repository_tools-act-fc328635",
        args: &[],
    }, // supplied entry 103
    Case {
        raw: "LFS now fetches only opaque-names-join-1/Stage1/日本語/00001234",
        id: "repo-workflows-lfs_storage-fetch-only-3f558a4c",
        args: &[(
            "names-join-1",
            Sample::Text(
                "opaque-names-join-1/Stage1/日本語/00001234",
                "opaque-names-join-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 144
    Case {
        raw: "Initialize submodules",
        id: "repo-workflows-tour-tour-view-7bfe85f2",
        args: &[],
    }, // supplied entry 302
    Case {
        raw: "opaque-file-display-1/Stage1/日本語/00001234 isn't UnityYAMLMerge",
        id: "repo-workflows-unity-choose-tool-af82d6a2",
        args: &[(
            "file-display-1",
            Sample::Text(
                "opaque-file-display-1/Stage1/日本語/00001234",
                "opaque-file-display-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 307
    Case {
        raw: "Push the rebased opaque-branch/Stage1/日本語/00001234 to opaque-remote/Stage1/日本語/00001234",
        id: "repo-workflows-operation-push-rebased",
        args: &[
            (
                "branch",
                Sample::Text(
                    "opaque-branch/Stage1/日本語/00001234",
                    "opaque-branch/Stage1/日本語/00001234",
                ),
            ),
            (
                "remote",
                Sample::Text(
                    "opaque-remote/Stage1/日本語/00001234",
                    "opaque-remote/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 338
    Case {
        raw: "Test selected SSH key",
        id: "repo-workflows-operation-ssh-14036",
        args: &[],
    }, // supplied entry 356
    Case {
        raw: "Marked in the bisect",
        id: "repo-workflows-parallel_bisect-mark-parallel-result-e05d54d7",
        args: &[],
    }, // supplied entry 155
    Case {
        raw: "Comment relocation: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-pr_view-pr-files-arrived-20be6481",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 163
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
    }, // supplied entry 233
    Case {
        raw: "Remove “opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234” from opaque-plan-branch-2/Stage1/日本語/00001234, as opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-timeline-run-remove-73ccc5e0",
        args: &[
            (
                "self-entry-label-plan-id-1",
                Sample::Text(
                    "opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234",
                    "opaque-self-entry-label-plan-id-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "plan-branch-2",
                Sample::Text(
                    "opaque-plan-branch-2/Stage1/日本語/00001234",
                    "opaque-plan-branch-2/Stage1/日本語/00001234",
                ),
            ),
            (
                "name",
                Sample::Text(
                    "opaque-name/Stage1/日本語/00001234",
                    "opaque-name/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 277
    Case {
        raw: "What changed in #opaque-number/Stage1/日本語/00001234: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-versions-open-pr-since-e9aaff48",
        args: &[
            (
                "number",
                Sample::Text(
                    "opaque-number/Stage1/日本語/00001234",
                    "opaque-number/Stage1/日本語/00001234",
                ),
            ),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 322
    Case {
        raw: "Create SSH key there",
        id: "repo-workflows-operation-ssh-14019",
        args: &[],
    }, // supplied entry 353
    Case {
        raw: "Migrate ref storage",
        id: "repo-workflows-repository_tools-repository-tools-act-888e8b45",
        args: &[],
    }, // supplied entry 82
    Case {
        raw: "Remove outbox entry",
        id: "repo-workflows-repository_tools-act-7f694b62",
        args: &[],
    }, // supplied entry 94
    Case {
        raw: "Stage opaque-snapshot-spec-path-2/Stage1/日本語/00001234 from staging lens",
        id: "repo-workflows-file_comparison-comparison-pick-28dc27ab",
        args: &[
            ("pick-verb-1", Sample::Text("Stage", "Vormerken")),
            (
                "snapshot-spec-path-2",
                Sample::Text(
                    "opaque-snapshot-spec-path-2/Stage1/日本語/00001234",
                    "opaque-snapshot-spec-path-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 117
    Case {
        raw: "Register public key",
        id: "repo-workflows-ssh_setup-label-03ff732f",
        args: &[],
    }, // supplied entry 198
    Case {
        raw: "Force unlock 1234 files",
        id: "repo-workflows-operation-force-unlock-count",
        args: &[("count", Sample::Number(1234))],
    }, // supplied entry 344
    Case {
        raw: "Content recovery: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-content_recovery-content-perform-9d57ebb2",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 45
    Case {
        raw: "Remove replace ref",
        id: "repo-workflows-repository_tools-repository-tools-act-69a2d9db",
        args: &[],
    }, // supplied entry 84
    Case {
        raw: "Email patches · b4",
        id: "repo-workflows-email-email-act-c19c9f05",
        args: &[],
    }, // supplied entry 109
    Case {
        raw: "Sync: opaque-current/Stage1/日本語/00001234 published (it had nothing to pull)",
        id: "repo-workflows-push-sync-5ca83b4b",
        args: &[(
            "current",
            Sample::Text(
                "opaque-current/Stage1/日本語/00001234",
                "opaque-current/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 173
    Case {
        raw: "Compare versions: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-versions-open-branch-versions-70c06512",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 318
    Case {
        raw: "Earlier versions: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-working_drafts-poll-working-drafts-7a49e8a4",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 326
    Case {
        raw: "Mark in the bisect",
        id: "repo-workflows-operation-mark-bisect",
        args: &[],
    }, // supplied entry 347
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
    }, // supplied entry 25
    Case {
        raw: "Remove opaque-origin-key-1/Stage1/日本語/00001234 (opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234) from opaque-file/Stage1/日本語/00001234",
        id: "repo-workflows-doctor-remove-confirm-b0b26eef",
        args: &[
            (
                "origin-key-1",
                Sample::Text(
                    "opaque-origin-key-1/Stage1/日本語/00001234",
                    "opaque-origin-key-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "shortened-origin-shown-value-2",
                Sample::Text(
                    "opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234",
                    "opaque-shortened-origin-shown-value-2/Stage1/日本語/00001234",
                ),
            ),
            (
                "file",
                Sample::Text(
                    "opaque-file/Stage1/日本語/00001234",
                    "opaque-file/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 60
    Case {
        raw: "Push is still running",
        id: "repo-workflows-repository_tools-repository-tool-mutation-988356ee",
        args: &[("busy", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 86
    Case {
        raw: "Subtree operation",
        id: "repo-workflows-repository_tools-act-88c16034",
        args: &[],
    }, // supplied entry 99
    Case {
        raw: "Ignore revision: opaque-error/Stage1/日本語/00001234",
        id: "repo-workflows-line_dossier-dossier-ignore-finished-4c50387c",
        args: &[(
            "error",
            Sample::Text(
                "opaque-error/Stage1/日本語/00001234",
                "opaque-error/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 149
    Case {
        raw: "Summary cancelled",
        id: "repo-workflows-line_dossier-run-ae6f0623",
        args: &[],
    }, // supplied entry 150
    Case {
        raw: "Saving the choice",
        id: "repo-workflows-unity-choose-tool-34706d9f",
        args: &[],
    }, // supplied entry 308
    Case {
        raw: "Move commit down",
        id: "repo-workflows-agent-rewrite-2630fe5c",
        args: &[],
    }, // supplied entry 14
    Case {
        raw: "Skip this commit",
        id: "repo-workflows-bisect-mark-bisect-15306802",
        args: &[],
    }, // supplied entry 38
    Case {
        raw: "Isolated restack",
        id: "repo-workflows-repository_tools-show-9174163c",
        args: &[],
    }, // supplied entry 71
    Case {
        raw: "Remote preview: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-repository_tools-remote-config-preview-bba0304c",
        args: &[(
            "e",
            Sample::Text(
                "opaque-e/Stage1/日本語/00001234",
                "opaque-e/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 87
    Case {
        raw: "Queue exact push",
        id: "repo-workflows-repository_tools-act-b967f21f",
        args: &[],
    }, // supplied entry 93
    Case {
        raw: "Start work on opaque-issue-key-1/Stage1/日本語/00001234: opaque-why/Stage1/日本語/00001234",
        id: "repo-workflows-issue_work-start-issue-c14dd822",
        args: &[
            (
                "issue-key-1",
                Sample::Text(
                    "opaque-issue-key-1/Stage1/日本語/00001234",
                    "opaque-issue-key-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "why",
                Sample::Text(
                    "opaque-why/Stage1/日本語/00001234",
                    "opaque-why/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 136
    Case {
        raw: "Recover content",
        id: "repo-workflows-content_recovery-content-perform-fbdf23f8",
        args: &[],
    }, // supplied entry 44
    Case {
        raw: "Save changelist",
        id: "repo-workflows-repository_tools-act-a3cbb206",
        args: &[],
    }, // supplied entry 73
    Case {
        raw: "Branch opaque-c-name-1/Stage1/日本語/00001234 deleted",
        id: "repo-workflows-timeline-fallback-label-b5bd817d",
        args: &[(
            "c-name-1",
            Sample::Text(
                "opaque-c-name-1/Stage1/日本語/00001234",
                "opaque-c-name-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 271
    Case {
        raw: "Nothing to undo",
        id: "repo-workflows-timeline-undo-83290078",
        args: &[],
    }, // supplied entry 284
    Case {
        raw: "Nothing to redo",
        id: "repo-workflows-timeline-redo-4aaf2710",
        args: &[],
    }, // supplied entry 287
    Case {
        raw: "Create branch opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-agent-create-branch-d2a49cac",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 8
    Case {
        raw: "Move commit up",
        id: "repo-workflows-agent-rewrite-3d1ccb79",
        args: &[],
    }, // supplied entry 13
    Case {
        raw: "Delete branch opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-agent-delete-branch-d32d800a",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 15
    Case {
        raw: "Push refused: origin/00001234 has 1234 commits you don't have",
        id: "repo-workflows-push-remote-checked-3bbec358",
        args: &[(
            "banner-headline-r-1",
            Sample::Text(
                "origin/00001234 has 1,234 commits you don't have",
                "origin/00001234 enthält 1.234 Commits, die Ihnen fehlen",
            ),
        )],
    }, // supplied entry 179
    Case {
        raw: "Move changes: opaque-why/Stage1/日本語/00001234",
        id: "repo-workflows-surgery-open-move-9a352cd5",
        args: &[(
            "why",
            Sample::Text(
                "opaque-why/Stage1/日本語/00001234",
                "opaque-why/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 242
    Case {
        raw: "Split commit: opaque-why/Stage1/日本語/00001234",
        id: "repo-workflows-surgery-open-split-ad93af6b",
        args: &[(
            "why",
            Sample::Text(
                "opaque-why/Stage1/日本語/00001234",
                "opaque-why/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 247
    Case {
        raw: "Saved manually",
        id: "repo-workflows-working_drafts-save-working-draft-3e48426a",
        args: &[],
    }, // supplied entry 335
    Case {
        raw: "Finish hotfix opaque-tag/Stage1/日本語/00001234",
        id: "repo-workflows-operation-finish-hotfix",
        args: &[(
            "tag",
            Sample::Text(
                "opaque-tag/Stage1/日本語/00001234",
                "opaque-tag/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 340
    Case {
        raw: "Start work on opaque-issue/Stage1/日本語/00001234",
        id: "repo-workflows-operation-start-issue-work",
        args: &[(
            "issue",
            Sample::Text(
                "opaque-issue/Stage1/日本語/00001234",
                "opaque-issue/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 342
    Case {
        raw: "Create SSH key",
        id: "repo-workflows-operation-ssh-14017",
        args: &[],
    }, // supplied entry 352
    Case {
        raw: "Finish bisect",
        id: "repo-workflows-bisect-reset-bisect-1bdcc155",
        args: &[],
    }, // supplied entry 32
    Case {
        raw: "Container opaque-binding-id-12-1/Stage1/日本語/00001234 · opaque-binding-folder-2/Stage1/日本語/00001234",
        id: "repo-workflows-devcontainer-terminal-77e417ed",
        args: &[
            (
                "binding-id-12-1",
                Sample::Text(
                    "opaque-binding-id-12-1/Stage1/日本語/00001234",
                    "opaque-binding-id-12-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "binding-folder-2",
                Sample::Text(
                    "opaque-binding-folder-2/Stage1/日本語/00001234",
                    "opaque-binding-folder-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 59
    Case {
        raw: "Set opaque-setting-key-1/Stage1/日本語/00001234 to opaque-shortened-value-2/Stage1/日本語/00001234 for this repository",
        id: "repo-workflows-doctor-set-row-921f564b",
        args: &[
            (
                "setting-key-1",
                Sample::Text(
                    "opaque-setting-key-1/Stage1/日本語/00001234",
                    "opaque-setting-key-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "shortened-value-2",
                Sample::Text(
                    "opaque-shortened-value-2/Stage1/日本語/00001234",
                    "opaque-shortened-value-2/Stage1/日本語/00001234",
                ),
            ),
            (
                "scope-choice-scope-to-lowercase-3",
                Sample::Text("This repository", "Dieses Repository"),
            ),
        ],
    }, // supplied entry 61
    Case {
        raw: "Publish group",
        id: "repo-workflows-repository_tools-publish-group-3cc9a2c6",
        args: &[],
    }, // supplied entry 88
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
    }, // supplied entry 221
    Case {
        raw: "Abort restack",
        id: "repo-workflows-stack-abort-restack-4fe4c0de",
        args: &[],
    }, // supplied entry 225
    Case {
        raw: "Put opaque-branch/Stage1/日本語/00001234 back at opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
        id: "repo-workflows-timeline-put-branch-back-94acd0c3",
        args: &[
            (
                "branch",
                Sample::Text(
                    "opaque-branch/Stage1/日本語/00001234",
                    "opaque-branch/Stage1/日本語/00001234",
                ),
            ),
            (
                "crate-repo-short-commit-1",
                Sample::Text(
                    "opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
                    "opaque-crate-repo-short-commit-1/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 295
    Case {
        raw: "Working draft",
        id: "repo-workflows-working_drafts-change-working-draft-278865c0",
        args: &[],
    }, // supplied entry 336
    Case {
        raw: "Force unlock opaque-path/Stage1/日本語/00001234",
        id: "repo-workflows-operation-force-unlock",
        args: &[(
            "path",
            Sample::Text(
                "opaque-path/Stage1/日本語/00001234",
                "opaque-path/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 343
    Case {
        raw: "Track opaque-pattern/Stage1/日本語/00001234 in LFS",
        id: "repo-workflows-operation-lfs-track",
        args: &[(
            "pattern",
            Sample::Text(
                "opaque-pattern/Stage1/日本語/00001234",
                "opaque-pattern/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 345
    Case {
        raw: "Cherry-pick",
        id: "interaction-operation-cherry-pick",
        args: &[],
    }, // supplied entry 1
    Case {
        raw: "Start bisect",
        id: "repo-workflows-bisect-start-bisect-db936451",
        args: &[],
    }, // supplied entry 35
    Case {
        raw: "Add to agent",
        id: "repo-workflows-ssh_setup-label-1db4242b",
        args: &[],
    }, // supplied entry 197
    Case {
        raw: "Split opaque-short-hash-1/Stage1/日本語/00001234 into 1234",
        id: "repo-workflows-surgery-run-split-ddd763c5",
        args: &[
            (
                "short-hash-1",
                Sample::Text(
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                    "opaque-short-hash-1/Stage1/日本語/00001234",
                ),
            ),
            ("messages-len-2", Sample::Number(1234)),
        ],
    }, // supplied entry 251
    Case {
        raw: "Merge commit",
        id: "repo-workflows-timeline-describe-reflog-8e9d44a6",
        args: &[],
    }, // supplied entry 257
    Case {
        raw: "Branch opaque-c-name-1/Stage1/日本語/00001234 made",
        id: "repo-workflows-timeline-fallback-label-1ac54e67",
        args: &[(
            "c-name-1",
            Sample::Text(
                "opaque-c-name-1/Stage1/日本語/00001234",
                "opaque-c-name-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 270
    Case {
        raw: "Stash staged",
        id: "repo-workflows-partial_stash-label-057347af",
        args: &[],
    }, // supplied entry 330
    Case {
        raw: "Apply stash opaque-short-pick-hash-1/Stage1/日本語/00001234",
        id: "repo-workflows-reflog-apply-stash-93a6ce40",
        args: &[(
            "short-pick-hash-1",
            Sample::Text(
                "opaque-short-pick-hash-1/Stage1/日本語/00001234",
                "opaque-short-pick-hash-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 331
    Case {
        raw: "Push hotfix opaque-tag/Stage1/日本語/00001234",
        id: "repo-workflows-operation-push-hotfix",
        args: &[(
            "tag",
            Sample::Text(
                "opaque-tag/Stage1/日本語/00001234",
                "opaque-tag/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 341
    Case {
        raw: "Rename opaque-from/Stage1/日本語/00001234 to opaque-to/Stage1/日本語/00001234",
        id: "repo-workflows-agent-rename-branch-5de8c7a7",
        args: &[
            (
                "from",
                Sample::Text(
                    "opaque-from/Stage1/日本語/00001234",
                    "opaque-from/Stage1/日本語/00001234",
                ),
            ),
            (
                "to",
                Sample::Text(
                    "opaque-to/Stage1/日本語/00001234",
                    "opaque-to/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 9
    Case {
        raw: "Drop commit",
        id: "repo-workflows-agent-rewrite-601e2a07",
        args: &[],
    }, // supplied entry 12
    Case {
        raw: "Stop bisect",
        id: "repo-workflows-bisect-reset-bisect-0ddac35c",
        args: &[],
    }, // supplied entry 33
    Case {
        raw: "Push: wait for Push",
        id: "repo-workflows-issue_work-refresh-issue-a0e9ad2a",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            ("busy", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 137
    Case {
        raw: "Push: Push failed: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-release-release-stopped-a4afd9be",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            ("step-label-1", Sample::Text("Push", "Pushen")),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 190
    Case {
        raw: "Push, and 1234 more",
        id: "repo-workflows-timeline-outside-label-d1d10f56",
        args: &[
            ("first", Sample::Text("Push", "Pushen")),
            ("rest-len-1", Sample::Number(1234)),
        ],
    }, // supplied entry 254
    Case {
        raw: "Branch made",
        id: "repo-workflows-timeline-describe-reflog-56bb1bb2",
        args: &[],
    }, // supplied entry 266
    Case {
        raw: "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)",
        id: "repo-workflows-timeline-show-receipt-e172bf9c",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            (
                "receipt-sentence-1",
                Sample::Text(
                    "Final files identical, authors kept, messages kept",
                    "Dateien am Ende identisch, Autoren beibehalten, Nachrichten beibehalten",
                ),
            ),
        ],
    }, // supplied entry 282
    Case {
        raw: "Going there",
        id: "repo-workflows-timeline-jump-a5337838",
        args: &[],
    }, // supplied entry 289
    Case {
        raw: "Reading #opaque-number/Stage1/日本語/00001234…",
        id: "repo-workflows-versions-open-pr-since-60afdf08",
        args: &[(
            "number",
            Sample::Text(
                "opaque-number/Stage1/日本語/00001234",
                "opaque-number/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 320
    Case {
        raw: "Stash files",
        id: "repo-workflows-partial_stash-label-598787f5",
        args: &[],
    }, // supplied entry 329
    Case {
        raw: "Force push",
        id: "repo-workflows-agent-push-1d502fc3",
        args: &[],
    }, // supplied entry 16
    Case {
        raw: "Bisect run",
        id: "repo-workflows-bisect-start-bisect-run-d300d73e",
        args: &[],
    }, // supplied entry 41
    Case {
        raw: "Push refused: opaque-out-trim-1/Stage1/日本語/00001234",
        id: "repo-workflows-push-push-op-ended-79357f2e",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            (
                "out-trim-1",
                Sample::Text(
                    "opaque-out-trim-1/Stage1/日本語/00001234",
                    "opaque-out-trim-1/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 176
    Case {
        raw: "Create key",
        id: "repo-workflows-ssh_setup-label-b6cde903",
        args: &[],
    }, // supplied entry 196
    Case {
        raw: "Checkout opaque-branch/Stage1/日本語/00001234",
        id: "repo-workflows-agent-checkout-734452f6",
        args: &[(
            "branch",
            Sample::Text(
                "opaque-branch/Stage1/日本語/00001234",
                "opaque-branch/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 10
    Case {
        raw: "Mark opaque-short-rev-1/Stage1/日本語/00001234 as opaque-bisect-word-mark-2/Stage1/日本語/00001234",
        id: "repo-workflows-bisect-mark-bisect-22f6b91b",
        args: &[
            (
                "short-rev-1",
                Sample::Text(
                    "opaque-short-rev-1/Stage1/日本語/00001234",
                    "opaque-short-rev-1/Stage1/日本語/00001234",
                ),
            ),
            (
                "bisect-word-mark-2",
                Sample::Text(
                    "opaque-bisect-word-mark-2/Stage1/日本語/00001234",
                    "opaque-bisect-word-mark-2/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 37
    Case {
        raw: "Push failed: opaque-e/Stage1/日本語/00001234",
        id: "repo-workflows-bisect-bisect-step-done-8336fbfe",
        args: &[
            ("label", Sample::Text("Push", "Pushen")),
            (
                "e",
                Sample::Text(
                    "opaque-e/Stage1/日本語/00001234",
                    "opaque-e/Stage1/日本語/00001234",
                ),
            ),
        ],
    }, // supplied entry 40
    Case {
        raw: "Turn on: commit-graph",
        id: "repo-workflows-doctor-turn-on-action-d8e3e02d",
        args: &[(
            "s-title-to-lowercase-1",
            Sample::Text("Commit-graph", "Commit-Graph"),
        )],
    }, // supplied entry 62
    Case {
        raw: "Checking opaque-what/Stage1/日本語/00001234",
        id: "repo-workflows-push-check-remote-9a7c53eb",
        args: &[(
            "what",
            Sample::Text(
                "opaque-what/Stage1/日本語/00001234",
                "opaque-what/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 178
    Case {
        raw: "Released opaque-run-plan-tag-1/Stage1/日本語/00001234",
        id: "repo-workflows-release-release-stopped-1904d12e",
        args: &[(
            "run-plan-tag-1",
            Sample::Text(
                "opaque-run-plan-tag-1/Stage1/日本語/00001234",
                "opaque-run-plan-tag-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 191
    Case {
        raw: "Push: stopped",
        id: "repo-workflows-release-release-stopped-328c7ab5",
        args: &[("label", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 192
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
    }, // supplied entry 211
    Case {
        raw: "Publish pull requests",
        id: "repo-workflows-stack-start-publish-c4ffa0c1",
        args: &[("noun", Sample::Text("pull request", "Pull Request"))],
    }, // supplied entry 217
    Case {
        raw: "Reset to opaque-to/Stage1/日本語/00001234",
        id: "repo-workflows-timeline-describe-reflog-32fa2259",
        args: &[(
            "to",
            Sample::Text(
                "opaque-to/Stage1/日本語/00001234",
                "opaque-to/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 261
    Case {
        raw: "opaque-running-agent-1/Stage1/日本語/00001234: Push: done",
        id: "repo-workflows-agent-agent-op-done-16d1e9b2",
        args: &[
            (
                "running-agent-1",
                Sample::Text(
                    "opaque-running-agent-1/Stage1/日本語/00001234",
                    "opaque-running-agent-1/Stage1/日本語/00001234",
                ),
            ),
            ("label", Sample::Text("Push", "Pushen")),
        ],
    }, // supplied entry 0
    Case {
        raw: "Unstage opaque-paths-label-paths-1/Stage1/日本語/00001234",
        id: "repo-workflows-agent-unstage-eb003390",
        args: &[(
            "paths-label-paths-1",
            Sample::Text(
                "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                "opaque-paths-label-paths-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 5
    Case {
        raw: "build · opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-bakeoff-build-c9238812",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 31
    Case {
        raw: "Mark as opaque-bisect-word-mark-1/Stage1/日本語/00001234",
        id: "repo-workflows-bisect-mark-bisect-9b0f3138",
        args: &[(
            "bisect-word-mark-1",
            Sample::Text(
                "opaque-bisect-word-mark-1/Stage1/日本語/00001234",
                "opaque-bisect-word-mark-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 39
    Case {
        raw: "Test SSH",
        id: "repo-workflows-ssh_setup-label-60083f45",
        args: &[],
    }, // supplied entry 199
    Case {
        raw: "Checkout",
        id: "repo-workflows-timeline-describe-reflog-99e71f48",
        args: &[],
    }, // supplied entry 264
    Case {
        raw: "Discard opaque-branch/Stage1/日本語/00001234",
        id: "repo-workflows-timeline-discard-remove-66f54467",
        args: &[(
            "branch",
            Sample::Text(
                "opaque-branch/Stage1/日本語/00001234",
                "opaque-branch/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 278
    Case {
        raw: "Go to “opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234”",
        id: "repo-workflows-timeline-jump-target-1e4e36b2",
        args: &[(
            "self-timeline-get-id-label-1",
            Sample::Text(
                "opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234",
                "opaque-self-timeline-get-id-label-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 293
    Case {
        raw: "Snapshot",
        id: "repo-workflows-operation-snapshot",
        args: &[],
    }, // supplied entry 348
    Case {
        raw: "Rebase opaque-name/Stage1/日本語/00001234",
        id: "repo-workflows-repository_tools-show-34b10b8d",
        args: &[(
            "name",
            Sample::Text(
                "opaque-name/Stage1/日本語/00001234",
                "opaque-name/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 72
    Case {
        raw: "Restack",
        id: "repo-workflows-stack-start-restack-9fbc2915",
        args: &[],
    }, // supplied entry 223
    Case {
        raw: "Git LFS",
        id: "repo-workflows-tour-tour-view-6df591db",
        args: &[],
    }, // supplied entry 300
    Case {
        raw: "Revert",
        id: "interaction-operation-revert",
        args: &[],
    }, // supplied entry 2
    Case {
        raw: "1234 paths",
        id: "repo-workflows-agent-paths-label-26d36d06",
        args: &[("many-len-1", Sample::Number(1234))],
    }, // supplied entry 3
    Case {
        raw: "Stage opaque-paths-label-paths-1/Stage1/日本語/00001234",
        id: "repo-workflows-agent-stage-3b4ddea7",
        args: &[(
            "paths-label-paths-1",
            Sample::Text(
                "opaque-paths-label-paths-1/Stage1/日本語/00001234",
                "opaque-paths-label-paths-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 4
    Case {
        raw: "Commit",
        id: "interaction-operation-commit",
        args: &[],
    }, // supplied entry 7
    Case {
        raw: "Push: done",
        id: "repo-workflows-lfs_locks-lock-job-36f5baee",
        args: &[("label", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 142
    Case {
        raw: "Rebase",
        id: "interaction-operation-rebase",
        args: &[],
    }, // supplied entry 260
    Case {
        raw: "Merge opaque-w-merge-len-1/Stage1/日本語/00001234",
        id: "repo-workflows-timeline-describe-reflog-43695d49",
        args: &[(
            "w-merge-len-1",
            Sample::Text(
                "opaque-w-merge-len-1/Stage1/日本語/00001234",
                "opaque-w-merge-len-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 267
    Case {
        raw: "opaque-c-name-1/Stage1/日本語/00001234 moved",
        id: "repo-workflows-timeline-fallback-label-e48d463d",
        args: &[(
            "c-name-1",
            Sample::Text(
                "opaque-c-name-1/Stage1/日本語/00001234",
                "opaque-c-name-1/Stage1/日本語/00001234",
            ),
        )],
    }, // supplied entry 272
    Case {
        raw: "Amend",
        id: "interaction-operation-amend",
        args: &[],
    }, // supplied entry 6
    Case {
        raw: "Stash",
        id: "interaction-operation-stash",
        args: &[],
    }, // supplied entry 11
    Case {
        raw: "setup",
        id: "repo-workflows-agents-agent-act-8fb6d5f3",
        args: &[],
    }, // supplied entry 20
    Case {
        raw: "Reset",
        id: "repo-workflows-timeline-describe-reflog-daee7606",
        args: &[],
    }, // supplied entry 262
    Case {
        raw: "Moved",
        id: "repo-workflows-timeline-describe-reflog-b11c9047",
        args: &[],
    }, // supplied entry 269
    Case {
        raw: "Undo Push",
        id: "interaction-operation-undo",
        args: &[("operation", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 285
    Case {
        raw: "Redo Push",
        id: "interaction-operation-redo",
        args: &[("operation", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 288
    Case {
        raw: "Push",
        id: "interaction-operation-push",
        args: &[],
    }, // supplied entry 17
    Case {
        raw: "Pull",
        id: "interaction-operation-pull",
        args: &[],
    }, // supplied entry 265
    Case {
        raw: "Undo",
        id: "repo-workflows-timeline-undo-a8283ade",
        args: &[],
    }, // supplied entry 283
    Case {
        raw: "Redo",
        id: "repo-workflows-timeline-redo-74273989",
        args: &[],
    }, // supplied entry 286
    Case {
        raw: "Push (stopped part-way)",
        id: "repo-workflows-hotfix-hotfix-op-done-fbdef87e",
        args: &[("run-label-1", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 132
    Case {
        raw: "Push (stopped partway)",
        id: "repo-workflows-timeline-timeline-op-ended-be0297e1",
        args: &[("label", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 281
    Case {
        raw: "Push (here only)",
        id: "repo-workflows-operation-here-only",
        args: &[("operation", Sample::Text("Push", "Pushen"))],
    }, // supplied entry 339
    Case {
        raw: "Check out #1234 to push to raw-owner/ß",
        id: "repo-workflows-forks-checkout-fork-abaaf4c3",
        args: &[
            ("number", Sample::Text("1234", "1234")),
            ("info-owner-1", Sample::Text("raw-owner/ß", "raw-owner/ß")),
        ],
    }, // additive immutable owner9471 contract; original396 entries unchanged
];

#[test]
fn workflow_finite_contracts_initialize_all_patterns_and_render_named_types_in_both_locales() {
    assert_eq!(CASES.len(), TEMPLATES.len());
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for case in CASES {
            let args: Vec<(&str, crate::i18n::Value<'_>)> = case
                .args
                .iter()
                .map(|(name, sample)| {
                    (
                        *name,
                        match sample {
                            Sample::Number(n) => (*n).into(),
                            Sample::Text(en, de) => {
                                (if language == "de" { *de } else { *en }).into()
                            }
                        },
                    )
                })
                .collect();
            let expected = crate::i18n::message(case.id, &args);
            assert_ne!(expected, case.id, "embedded catalog: {}", case.id);
            let stored = case.raw.to_owned();
            assert_eq!(
                display_operation(&stored),
                expected,
                "operation {}: {}",
                case.id,
                case.raw
            );
            assert_eq!(
                display_status(&stored),
                expected,
                "status {}: {}",
                case.id,
                case.raw
            );
            assert_eq!(stored, case.raw);
        }
    }
}

#[test]
fn workflow_selector_defaults_nested_guards_and_numeric_identity_are_paint_only() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let german = language == "de";
        for (raw, target, remote) in [
            (
                "refs/heads/00001234's commits are signed off here, but not pushed: Ctrl+Z puts them back as they were",
                "no",
                "",
            ),
            (
                "refs/heads/00001234's commits are signed off here, but not pushed to ssh://opaque/00001234: Ctrl+Z puts them back as they were",
                "yes",
                "ssh://opaque/00001234",
            ),
        ] {
            let expected = crate::i18n::message(
                "repo-workflows-status-dco-local-only",
                &[
                    ("branch", "refs/heads/00001234".into()),
                    ("target", target.into()),
                    ("remote", remote.into()),
                ],
            );
            let stored = raw.to_owned();
            assert_eq!(display_operation(&stored), expected);
            assert_eq!(display_status(&stored), expected);
            assert_eq!(stored, raw);
        }
        for (raw, kept, names) in [
            ("No fork remote can go", "no", ""),
            (
                "No fork remote can go\n\nKept: refs/00001234, 日本語, whose branches have commits no branch here has.",
                "yes",
                "refs/00001234, 日本語",
            ),
        ] {
            let expected = crate::i18n::message(
                "repo-workflows-status-forks-none-to-remove",
                &[("kept", kept.into()), ("names", names.into())],
            );
            assert_eq!(display_status(raw), expected);
        }
        // Source-derived workflow60: both splits are legal opaque value/file data.
        let raw = "Remove core.editor (old) from new) from .git/config";
        let stored = raw.to_owned();
        assert_eq!(display_operation(&stored), raw);
        assert_eq!(display_status(&stored), raw);
        assert_eq!(stored, raw);
        let pr = "Rebase #00001234 onto refs/heads/00001234 and push";
        let shown = display_operation(pr);
        assert!(shown.contains("00001234"));
        assert!(!shown.contains("1.234"));
        assert!(!shown.contains("1,234"));
        for raw in [
            "Force unlock 1234 files",
            "Distribute staged fixes into 1234 commits",
        ] {
            assert!(display_operation(raw).contains(if german { "1.234" } else { "1,234" }));
        }
        for raw in [
            "Abort custom-deploy failed: Analysis cancelled",
            "Continue custom-script: done",
            "Undo custom-deploy: done",
            "Push: wait for Abort custom-deploy",
            "Preparing custom-script (here only)",
            "Publish custom requests",
            "Turn on: arbitrary provider suggestion",
            "Set key to literal for custom scope",
            "custom path from staging lens",
            "Distribute staged fixes: a custom state is in progress",
            "Restack stopped at refs/00001234 on conflicts: resolve them at provider location, then Continue restack (or Abort restack)",
            "Push: done. provider prose (receipt in the timeline, Ctrl+Shift+Z)",
            "external-custom, and 12 more",
            "external provider: Analysis cancelled",
            "external provider failed: Analysis cancelled",
        ] {
            assert_eq!(display_operation(raw), raw, "operation unknown: {raw}");
            assert_eq!(display_status(raw), raw, "status unknown: {raw}");
        }
        for raw in [
            "Stage1",
            "Before (external)",
            "After (external)",
            "provider-a and provider-b",
            "Original fix",
            "Adapted fix",
        ] {
            assert_eq!(display_operation(raw), raw);
        }
        assert_eq!(headline("origin has 1 commits you don't have"), None);
        assert_eq!(headline("origin has +1 commit you don't have"), None);
        assert_eq!(headline("origin has 1,234 commits you don't have"), None);
        assert!(headline("origin has 1 commit you don't have").is_some());
        assert!(headline("origin has 0 commits you don't have").is_some());
        assert!(headline("origin has nothing you don't have now: push again").is_some());
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_receipt_adapter_accepts_only_finite_owned_clause_order_and_opaque_external_data() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for raw in [
            "Final files identical",
            "Final files differ, too many commits to follow one by one",
            "Final files identical, 1 author changed, 2 messages changed, 1234 commits now signed, 2 commits → 1234",
            "Final files identical, authors kept, messages kept, 1 signature dropped",
            "Final files differ, 2 authors changed, 1 message changed, signatures replaced",
            "Final files differ, signatures replaced, 12 dropped",
            "Final files differ, signatures replaced, 12 more signed",
        ] {
            let shown = receipt(raw);
            assert!(shown.is_some(), "receipt missing: {raw}");
        }
        for raw in [
            "external command",
            "Stage 1",
            "Final files identical, provider prose",
            "Final files identical, 2 author changed",
            "Final files identical, 1 authors changed",
            "Final files identical, authors kept, authors kept",
            "Final files identical, messages kept, authors kept",
            "Final files identical, too many commits to follow one by one, authors kept",
            "Final files differ, signatures replaced, arbitrary dropped",
            "Final files differ, signatures replaced, 0 dropped",
            "Final files differ, signatures replaced, 1,234 dropped",
        ] {
            assert_eq!(receipt(raw), None, "external/malformed receipt: {raw}");
        }
        let raw = "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)";
        let rendered = display_status(raw);
        assert_eq!(
            raw,
            "Push: done. Final files identical, authors kept, messages kept (receipt in the timeline, Ctrl+Shift+Z)"
        );
        assert!(rendered.contains(if language == "de" {
            "Dateien am Ende identisch"
        } else {
            "Final files identical"
        }));
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_finite_scope_health_noun_and_state_choices_render_each_supplied_contract() {
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (raw, id) in [
            (
                "this repository",
                "repo-workflows-doctor-scope-choice-38429e22",
            ),
            (
                "this worktree",
                "repo-workflows-doctor-scope-choice-0eb26325",
            ),
            (
                "all my repositories",
                "repo-workflows-doctor-scope-choice-59b8031f",
            ),
            (
                "everyone on this machine",
                "repo-workflows-doctor-scope-choice-ad944dcb",
            ),
            (
                "command line",
                "repo-workflows-doctor-scope-choice-c925a118",
            ),
        ] {
            let input = format!("Set opaque-key to opaque-value for {raw}");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-doctor-set-row-921f564b",
                    &[
                        ("setting-key-1", "opaque-key".into()),
                        ("shortened-value-2", "opaque-value".into()),
                        (
                            "scope-choice-scope-to-lowercase-3",
                            crate::i18n::text(id).into()
                        )
                    ]
                )
            );
        }
        for (raw, id) in [
            ("commit-graph", "backend-health-commit-graph"),
            (
                "scheduled maintenance",
                "backend-scheduled-maintenance-f688c9",
            ),
            ("file system monitor", "backend-file-system-monitor-bd6322"),
            ("untracked cache", "backend-untracked-cache-de0942"),
            ("pack the objects", "backend-pack-the-objects-5a853d"),
        ] {
            let input = format!("Turn on: {raw}");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-doctor-turn-on-action-d8e3e02d",
                    &[("s-title-to-lowercase-1", crate::i18n::text(id).into())]
                )
            );
        }
        for (raw, id) in [
            ("pull request", "backend-noun-pull-request"),
            ("merge request", "backend-noun-merge-request"),
        ] {
            let input = format!("Publish {raw}s");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-stack-start-publish-c4ffa0c1",
                    &[("noun", crate::i18n::text(id).into())]
                )
            );
        }
        for (raw, id) in [
            ("merge", "backend-in-progress-merge"),
            ("cherry-pick", "backend-in-progress-cherry-pick"),
            ("revert", "backend-in-progress-revert"),
            ("rebase", "backend-in-progress-rebase"),
            ("patch", "backend-in-progress-patch"),
        ] {
            let input = format!("Distribute staged fixes: a {raw} is in progress");
            assert_eq!(
                display_operation(&input),
                crate::i18n::message(
                    "repo-workflows-surgery-open-absorb-edcc444b",
                    &[("state-label-to-lowercase-1", crate::i18n::text(id).into())]
                )
            );
        }
        for raw in ["this repository ", "THIS REPOSITORY", "custom scope"] {
            assert!(scope(raw).is_none());
        }
        for raw in [
            "commit-graph ",
            "Commit-graph",
            "external provider suggestion",
        ] {
            assert!(health(raw).is_none());
        }
        for raw in ["pull requests", "custom request"] {
            assert!(noun(raw).is_none());
        }
        for raw in ["Merge", "external state"] {
            assert!(in_progress(raw).is_none());
        }
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn workflow_checkout_contract_preserves_decimal_pr_and_opaque_owner_in_public_wrappers() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for number in ["1234", "00001234"] {
            for owner in [
                "raw-owner/ß",
                "Stage1 to push to Continue custom-script: 日本語/00001234",
                "$(cargo test) --offline",
            ] {
                let raw = format!("Check out #{number} to push to {owner}");
                let stored = raw.clone();
                let expected = if language == "de" {
                    format!("#{number} auschecken, um nach {owner} zu pushen")
                } else {
                    raw.clone()
                };
                assert_eq!(display_operation(&stored), expected);
                assert_eq!(display_status(&stored), expected);
                assert!(expected.contains(&format!("#{number}")));
                assert!(expected.contains(owner));
                assert!(!expected.contains("#1,234"));
                assert!(!expected.contains("#1.234"));
                assert_eq!(stored, raw);
                let nested = format!("Undo {raw}");
                let nested_expected = crate::i18n::message(
                    "interaction-operation-undo",
                    &[("operation", expected.into())],
                );
                assert_eq!(display_operation(&nested), nested_expected);
                assert_eq!(display_status(&nested), nested_expected);
            }
        }
        for raw in [
            "Check out #Stage1 to push to raw-owner/ß",
            "Check out #1,234 to push to raw-owner/ß",
            "Check out #1.234 to push to raw-owner/ß",
            "Check out #-1 to push to raw-owner/ß",
            "Check out #+1 to push to raw-owner/ß",
            "Check out #1234 to push toward raw-owner/ß",
            "custom Check out #1234 to push to raw-owner/ß",
            "Undo Check out #provider to push to raw-owner/ß",
            "Preparing Check out #provider to push to raw-owner/ß",
            "Continue custom-script Check out #1234 to push to raw-owner/ß",
            "Stage1",
        ] {
            let stored = raw.to_owned();
            assert_eq!(display_operation(&stored), raw);
            assert_eq!(display_status(&stored), raw);
            assert_eq!(stored, raw);
        }
    }
}

#[test]
fn workflow_r1_fixed_sentinels_and_raw_operation_templates_project_only_at_paint() {
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for (raw, id) in &[
            (
                "History worker stopped",
                "repo-workflows-saved_views-poll-da12ac51",
            ),
            (
                "History read cancelled",
                "repo-workflows-saved_views-poll-dc101692",
            ),
            (
                "History worker panicked",
                "repo-workflows-saved_views-poll-33647b3e",
            ),
            (
                "Use YYYY-MM-DD dates",
                "repo-workflows-atlas-read-atlas-02c42736",
            ),
            (
                "Use YYYY-MM-DD dates, with start on or before end",
                "repo-workflows-atlas-read-atlas-67b18cbf",
            ),
            (
                "Use dates such as 2026-11-07",
                "repo-workflows-history_activity-requested-range-4d18caf5",
            ),
            (
                "The start date must be on or before the end date",
                "repo-workflows-history_activity-requested-range-25f6acd0",
            ),
            (
                "Analysis cancelled. Refresh to try again.",
                "repo-workflows-history_activity-cancel-ddfe1c43",
            ),
            (
                "No supported forge remote; commit history remains available",
                "repo-workflows-line_dossier-poll-line-dossier-45e16671",
            ),
            (
                "Dossier display reached256versions or32MiB; start a separate trace to inspect earlier history",
                "repo-workflows-line_dossier-poll-line-dossier-1cd553a8",
            ),
            (
                "Trace cancelled",
                "repo-workflows-line_dossier-line-dossier-view-a8198076",
            ),
            (
                "Repository changed. Refresh the file view.",
                "repo-workflows-file_comparison-invalidate-81f92ce0",
            ),
            (
                "Invalid comparison sides",
                "repo-workflows-file_comparison-prepare-da3978aa",
            ),
            (
                "More than 20,000 lines in a version; use the normal patch view",
                "repo-workflows-file_comparison-boundary-e44610d7",
            ),
            (
                "Git could not align these raw versions",
                "repo-workflows-file_comparison-boundary-b861cbb0",
            ),
            (
                "The raw patch is not UTF-8",
                "repo-workflows-file_comparison-boundary-5421cc1b",
            ),
            (
                "Git could not produce a raw text patch for these differing bytes; use the normal patch view",
                "repo-workflows-file_comparison-boundary-6732715f",
            ),
            (
                "Comparison cancelled. Refresh to try again.",
                "repo-workflows-file_comparison-file-comparison-view-725513de",
            ),
            (
                "No key, agent, account or network action has run. Choose each step explicitly.",
                "repo-workflows-ssh_setup-new-8633c172",
            ),
            (
                "Setup worker stopped; no later result will be applied.",
                "repo-workflows-ssh_setup-poll-380743c6",
            ),
        ] {
            assert_eq!(
                crate::repo::messages::display_status(raw),
                crate::i18n::text(id)
            );
        }
        for raw in [
            "Cut release tag/$(opaque)\n1234",
            "Clear the flag of path/{$x}: before\n1234",
            "Clear the flags of 1234 files",
        ] {
            let shown = crate::repo::messages::display_operation(raw);
            if language == "en-US" && !raw.ends_with(" files") {
                assert_eq!(shown, raw);
            }
            if language == "de" {
                assert_ne!(shown, raw);
            }
        }
        let private =
            "Binary or non-UTF8 versions: compare the byte preview; line staging is unavailable";
        assert_eq!(crate::repo::messages::display_status(private), private);
        assert_eq!(
            crate::repo::activity::display_workflow_error(private),
            private
        );
        for opaque in [
            "provider arbitrary diagnostic",
            "Cut release",
            "Clear the flags of 1.234 files",
        ] {
            assert_eq!(crate::repo::messages::display_operation(opaque), opaque);
        }
    }
}
