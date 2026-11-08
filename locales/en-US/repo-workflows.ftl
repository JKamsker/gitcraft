repo-workflows-agent-agent-op-done-16d1e9b2 = { $running-agent-1 }: { $label }: done
repo-workflows-agent-paths-label-26d36d06 = { $many-len-1 ->
        [one] one path
       *[other] { $many-len-1 } paths
    }
repo-workflows-agent-push-1d502fc3 = Force push
repo-workflows-agents-after-worktree-add-9fd23075 = The agent checkpoint hooks weren't installed: { $e }
repo-workflows-agents-after-worktree-add-a2ffe669 = The worktree setup recipe wasn't run: { $e }
repo-workflows-agents-agent-act-8fb6d5f3 = setup
repo-workflows-agents-forget-turns-1d1d391e = The turns weren't deleted: { $e }
repo-workflows-agents-install-hooks-2f5d6dc4 = Installing the agent hooks…
repo-workflows-agents-install-hooks-a8f2778c = The agent hooks weren't installed: { $e }
repo-workflows-agents-save-recipe-97381b52 = Worktree setup recipe saved
repo-workflows-agents-send-to-agent-4e9f0022 = Couldn't send { $what } to the { $label } terminal: { $e }
repo-workflows-agents-send-to-agent-a6f80242 = No agent terminal in this worktree to send { $what } to: start the agent (Start agent task…), or pick its terminal next to Send to agent
repo-workflows-agents-start-agent-task-d4ec2011 = Start agent task { $new-branch-1 }
repo-workflows-agents-start-agent-task-f80416ff = Wait for the running operation to finish
repo-workflows-atlas-new-70ba4563 = File history cancelled
repo-workflows-bakeoff-build-c9238812 = build · { $name }
repo-workflows-bisect-bisect-step-done-8336fbfe = { $label } failed: { $e }
repo-workflows-bisect-mark-bisect-15306802 = Skip this commit
repo-workflows-bisect-mark-bisect-22f6b91b = Mark { $short-rev-1 } as { $bisect-word-mark-2 }
repo-workflows-bisect-mark-bisect-9b0f3138 = Mark as { $bisect-word-mark-1 }
repo-workflows-bisect-mark-bisect-a1c6c771 = No bisect is going on
repo-workflows-bisect-reset-bisect-0ddac35c = Stop bisect
repo-workflows-bisect-reset-bisect-1bdcc155 = Finish bisect
repo-workflows-bisect-start-bisect-5c82e3e0 = Can't start a bisect: { $why }
repo-workflows-bisect-start-bisect-db936451 = Start bisect
repo-workflows-bisect-start-bisect-run-d300d73e = Bisect run
repo-workflows-ci_view-open-ci-991ae978 = Finish or close the { $editor } first
repo-workflows-content_recovery-content-perform-9d57ebb2 = Content recovery: { $e }
repo-workflows-content_recovery-content-perform-fbdf23f8 = Recover content
repo-workflows-custom_commands-run-custom-44771367 = ${ $name } can't hold a NUL character
repo-workflows-dco-dco-act-10fe4a7c = Couldn't save Sign off for this repository (git config { $signoff-sign-off-key-1 }): { $e }
repo-workflows-dco-dco-op-done-55d2b32b = Push the signed-off commits of { $plan-branch-1 }
repo-workflows-detached-detached-act-70dad4a3 = Wait for { $busy } to finish, then check out again
repo-workflows-detached-leave-to-007a4c0e = Didn't check out { $leave-label-1 }: { $busy } started meanwhile
repo-workflows-detached-leave-to-28ae5898 = Checkout { $leave-label-1 }: wait for { $busy } to finish
repo-workflows-detached-leave-to-48b0ce4c = Didn't check out { $leave-label-1 }: couldn't list the commits it would leave on no branch ({ $e })
repo-workflows-devcontainer-devcontainer-act-84f8f92f = Cancelled before starting
repo-workflows-devcontainer-terminal-77e417ed = Container { $binding-id-12-1 } · { $binding-folder-2 }
repo-workflows-devcontainer-terminal-f171832a = Cancelled before terminal start
repo-workflows-doctor-doctor-act-9bab8d95 = Wait for the running operation to end
repo-workflows-doctor-open-doctor-745454bd = Finish or close the conflict editor or the interactive rebase first
repo-workflows-doctor-remove-confirm-b0b26eef = Remove { $origin-key-1 } ({ $shortened-origin-shown-value-2 }) from { $file }
repo-workflows-doctor-set-row-921f564b = Set { $setting-key-1 } to { $shortened-value-2 } for { $scope-choice-scope-to-lowercase-3 }
repo-workflows-doctor-turn-on-action-d8e3e02d = Turn on: { $s-title-to-lowercase-1 }
repo-workflows-fence_view-fence-gate-2b8e4f3b = Checking against the agent fence…
repo-workflows-file_comparison-assemble-56abde67 = Moved-code comparison cancelled
repo-workflows-file_comparison-comparison-pick-28dc27ab = { $pick-verb-1 } { $snapshot-spec-path-2 } from staging lens
repo-workflows-file_comparison-open-file-comparison-a6cb5dc1 = Finish or close the editor first
repo-workflows-file_comparison-read-431f5828 = Moved-code analysis cancelled or exceeded 40 seconds
repo-workflows-forks-approve-runs-282082c7 = Couldn't approve #{ $number }'s workflow runs: { $e }
repo-workflows-forks-approve-runs-2f659b28 = Approving #{ $number }'s workflow runs…
repo-workflows-forks-approve-runs-a151ad19 = #{ $number }'s workflow runs are approved
repo-workflows-forks-approve-runs-a77eca22 = Approving runs the contributor's code on your CI: look through #{ $number }'s changes first, then choose Approve and run again
repo-workflows-forks-checkout-fork-5ecb3bac = Where #{ $number } comes from isn't known yet
repo-workflows-forks-forks-op-done-8287ad6b = { $plan-local-1 } is rebased here, but not pushed to { $plan-fork-2 }: Ctrl+Z puts it back as it was
repo-workflows-forks-forks-op-done-9c338e90 = The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it (or abort it)
repo-workflows-forks-plan-fork-rebase-1abac341 = This repository has no remote to rebase onto
repo-workflows-forks-plan-fork-rebase-ac71056e = Fetching #{ $number }'s branch and its base…
repo-workflows-forks-plan-tidy-a7e148ae = Remove unused fork remotes
repo-workflows-forks-plan-tidy-adf73b14 = Couldn't read the remotes: { $e }
repo-workflows-forks-plan-tidy-da56e1d6 = No fork remote can go{ $kept }
repo-workflows-guide-ask-empty-commit-348d9f75 = HEAD is detached: check out a branch to make the first commit on
repo-workflows-guide-start-first-commit-881f72cb = Nothing to commit yet: add or change files in the repository's folder
repo-workflows-hotfix-hotfix-act-06b55025 = { $tag } isn't a version to start a hotfix from
repo-workflows-hotfix-hotfix-op-done-fbdef87e = { $run-label-1 } (stopped part-way)
repo-workflows-issue_work-refresh-issue-a0e9ad2a = { $label }: wait for { $busy }
repo-workflows-issue_work-start-issue-563140a4 = Tracker account changed; reload the issue source
repo-workflows-issue_work-start-issue-c14dd822 = Start work on { $issue-key-1 }: { $why }
repo-workflows-issue_work-start-issue-e2b80c1b = Start work on { $issue-key-1 }: wait for { $busy }
repo-workflows-issue_work-take-issue-read-bbcbbb59 = Couldn't read the branches' issues: { $e }
repo-workflows-issue_work-unlink-issue-6df1900e = Forget the issue of { $branch } failed: { $e }
repo-workflows-lfs_locks-lock-job-35b33376 = { $label }: done. Commit .gitattributes to share it.
repo-workflows-lfs_storage-fetch-only-3f558a4c = LFS now fetches only { $names-join-1 }
repo-workflows-lfs_storage-fetch-only-e3be5d1f = Could not set what LFS fetches: { $e }
repo-workflows-line_dossier-dossier-ignore-finished-366a809a = Revision already present; ignore file and index unchanged
repo-workflows-line_dossier-dossier-ignore-finished-4c50387c = Ignore revision: { $error }
repo-workflows-line_dossier-dossier-ignore-finished-6e1731ff = Revision appended to .git-blame-ignore-revs; staged contents preserved. Ordinary Git blame can use blame.ignoreRevsFile=.git-blame-ignore-revs.
repo-workflows-line_dossier-run-a9f1b338 = History summary cancelled or exceeded its output limit
repo-workflows-line_dossier-run-ae6f0623 = Summary cancelled
repo-workflows-owners-owners-act-9cd4150e = This repository has no CODEOWNERS file
repo-workflows-parallel_bisect-mark-parallel-result-48f3588b = Mark in the bisect failed: { $e }
repo-workflows-parallel_bisect-mark-parallel-result-e05d54d7 = Marked in the bisect
repo-workflows-parallel_bisect-mark-parallel-result-e5e3ecc5 = Wait for the operation going on to finish
repo-workflows-parallel_bisect-start-parallel-3450e92d = A parallel bisect is already running in this tab
repo-workflows-postcards-poll-postcards-10867442 = PNG saved as a new local file.
repo-workflows-postcards-poll-postcards-4773e588 = Profile changed. Read the release range again before composing a postcard.
repo-workflows-postcards-poll-postcards-5358001c = Preview ready. The exported PNG contains exactly this composition; ellipses mark shortened or width-limited text.
repo-workflows-postcards-poll-postcards-60ccb94f = Pinned release range loaded. Review the disclosure list, then compose the PNG preview.
repo-workflows-postcards-postcard-window-b6333aff = Postcard work cancelled.
repo-workflows-postcards-render-8e2ed023 = Postcard rendering cancelled
repo-workflows-pr_view-pr-files-arrived-20be6481 = Comment relocation: { $e }
repo-workflows-pr_view-select-pr-b788526c = The pull request's branch is not available yet
repo-workflows-repository_tools-act-08cd49af = Close isolated proposal
repo-workflows-repository_tools-act-346c061a = Undo accepted isolated replay
repo-workflows-repository_tools-act-494336ed = Publish exact subtree split
repo-workflows-repository_tools-act-551afaa9 = Unshelve one changelist
repo-workflows-repository_tools-act-5bd57c48 = Replay in isolated worktrees
repo-workflows-repository_tools-act-63403bc6 = Undo virtual branch operation
repo-workflows-repository_tools-act-65d162a8 = Continue isolated replay
repo-workflows-repository_tools-act-6dc9a943 = Choose one saved turn to preview its inverse
repo-workflows-repository_tools-act-7f694b62 = Remove outbox entry
repo-workflows-repository_tools-act-88c16034 = Subtree operation
repo-workflows-repository_tools-act-901dccbe = Save virtual branch ownership
repo-workflows-repository_tools-act-a3cbb206 = Save changelist
repo-workflows-repository_tools-act-b64c091a = Accept isolated replay
repo-workflows-repository_tools-act-b967f21f = Queue exact push
repo-workflows-repository_tools-act-bea96408 = Shelve one changelist
repo-workflows-repository_tools-act-c08c65bd = Reconcile exact replay receipts
repo-workflows-repository_tools-act-c1eec3a9 = Commit one changelist
repo-workflows-repository_tools-act-ccd2fdda = Connection changed; reopen outbox
repo-workflows-repository_tools-act-ce061d46 = Change virtual branch materialization
repo-workflows-repository_tools-act-d021ec81 = Prepared subtree profile/sign-in changed; prepare and approve again
repo-workflows-repository_tools-act-d64d1e25 = Go online and wait for the current operation before preparing an outbox push
repo-workflows-repository_tools-act-de986fa8 = Commit virtual branch owned hunks
repo-workflows-repository_tools-act-fc328635 = Revert one saved turn
repo-workflows-repository_tools-dependency-job-57895189 = Check submodule publication dependencies
repo-workflows-repository_tools-dependency-job-a7b1dc6d = Dependencies finished, but the parent repository, profile or sign-in changed. Prepare its push again.
repo-workflows-repository_tools-dependency-job-f48f9c31 = Publish dependencies before parent review
repo-workflows-repository_tools-remote-config-preview-bba0304c = Remote preview: { $e }
repo-workflows-repository_tools-repository-tool-mutation-988356ee = { $busy } is still running
repo-workflows-repository_tools-repository-tools-act-07fb4089 = Make clone independent
repo-workflows-repository_tools-repository-tools-act-3c1c7a3a = Save publication group
repo-workflows-repository_tools-repository-tools-act-4e161156 = Save remote configuration
repo-workflows-repository_tools-repository-tools-act-66f98362 = Resolve submodule pointer
repo-workflows-repository_tools-repository-tools-act-69a2d9db = Remove replace ref
repo-workflows-repository_tools-repository-tools-act-82d14313 = Save pull and push routes
repo-workflows-repository_tools-repository-tools-act-888e8b45 = Migrate ref storage
repo-workflows-repository_tools-repository-tools-act-a7b0c90c = Join imported histories temporarily
repo-workflows-repository_tools-repository-tools-act-ad133eb8 = Convert legacy grafts to removable replace refs
repo-workflows-repository_tools-show-34b10b8d = Rebase { $name }
repo-workflows-repository_tools-show-9174163c = Isolated restack
repo-workflows-repository_tools-start-send-628a48e6 = Prepared outbox profile/sign-in changed; prepare and approve again
repo-workflows-repository_tools-start-send-994ea6b0 = Send exact queued push
repo-workflows-repository_tools-start-send-e7b0c78d = Autosend opted-in plain outbox push

repo-workflows-review_workflow-review-document-ui-7d9eb7ac = Close

repo-workflows-review_workflow-show-c3812fc4 = Remove

repo-workflows-dco-fix-dialog-1a8aeebd = Sign off, then review push

repo-workflows-dco-fix-dialog-6ce6ab1f = Sign off

repo-workflows-dco-whole-preview = Rewrites { $count ->
        [one] the commit
       *[other] the { $count } commits
    } of { $branch } since { $base } (rebase --signoff), adding "Signed-off-by: { $who }" to each that does not have it. { $unsigned ->
        [one] 1 of them is one
       *[other] { $unsigned } of them are ones
    } the DCO check refuses now:
    { $commits }{ $more ->
        [0] { "" }
       *[other] { "  • and " }{ $more } more{ "\u000A" }
    }{ $others ->
        [0] { "" }
        [one] { "\u000A1 of them is by " }{ $authors }: you become their committer, and your sign-off certifies them. Only go ahead if you may.{ "\u000A" }
       *[other] { "\u000A" }{ $others } of them are by { $authors }: you become their committer, and your sign-off certifies them. Only go ahead if you may.{ "\u000A" }
    }{ $push ->
        [yes] { "\u000AThen opens push preflight for the signed-off commits to " }{ $remote } with a lease: { $seen ->
            [yes] refused if someone pushed there since it was last fetched
           *[no] refused if that branch exists there already
        }. Review and approve that push separately.

    Ctrl+Z restores the local commits. Restoring published remote history requires its own reviewed push.
       *[no] { "\u000AThe branch has no remote branch: nothing is pushed. Pushing it later replaces the old commits there (a force push).\u000A\u000ACtrl+Z puts the commits as they were back." }
    }

repo-workflows-ghost-approval-preview-1426bc08 = { $files-len-1 } working files change

repo-workflows-ghost-approval-preview-675d236b = Discards uncommitted working changes

repo-workflows-ghost-approval-preview-70be46e5 = Leaving-commit list is truncated at 200

repo-workflows-ghost-approval-preview-e74a52f6 = Stops at step { $conflict-step-1 }: { $conflict-files-join-2 }

repo-workflows-ghost-current-7776bacf = HEAD is detached: check out a branch first

repo-workflows-ghost-delete-branch-26d05fbc = { $name } isn't merged into { $what }: Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch.

repo-workflows-ghost-delete-branch-dde58e5a = its upstream

repo-workflows-ghost-delete-branch-fadd1b38 = the checked-out branch

repo-workflows-ghost-diverged-pull = { $branch } and { $tracking } have diverged, so a fast-forward-only pull refuses{ $fetched ->
        [yes] { " (only " }{ $tracking } moves, with what it fetches)
       *[no] { "" }
    }

repo-workflows-ghost-drop-commit-181a54e0 = merges came after the commit, which this would flatten

repo-workflows-ghost-drop-commit-da2d4ead = the first commit has no parent to rebase from

repo-workflows-ghost-dropped-note = { $count ->
        [one] One commit dropped: its change is already there.
       *[other] { $count } commits dropped: their changes are already there.
    }

repo-workflows-ghost-fetched-count = { $count ->
        [one] One commit fetched
       *[other] { $count } commits fetched
    }

repo-workflows-ghost-force-push-2d4460fd = As of the last fetch of { $up-remote-1 }: with the lease, the push is refused if someone pushed since.

repo-workflows-ghost-force-push-a9c01edc = there is no branch { $name } here

repo-workflows-ghost-force-push-c637c04f = { $up-tracking-name-1 } loses nothing: a plain push would do.

repo-workflows-ghost-force-push-f06fee94 = { $name } tracks no branch on a remote

repo-workflows-ghost-ghost-strip-a81f72de = nothing has been done

repo-workflows-ghost-ghost-strip-c4a8b8f8 = Preview: { $title }

repo-workflows-ghost-ghost-strip-d0c20bfd = Working out what it would do…

repo-workflows-ghost-ghost-strip-f637d899 = Can't preview it. { $sentence-one-line-why-1 }

repo-workflows-ghost-listed-683768ab = and { $items-len-listed-1 } more

repo-workflows-ghost-lost-count = { $more ->
        [yes] more than { $count } commits
       *[no] { $count ->
            [one] one commit
           *[other] { $count } commits
        }
    }

repo-workflows-ghost-lost-line-02717c8c = Leaving the remote's branch ({ $count }): { $names }

repo-workflows-ghost-lost-line-811bd7ca = Left on no branch ({ $count }): { $names }

repo-workflows-ghost-lost-line-8ccbdaa0 = Left on no branch ({ $count }, { $o-replaced-1 } of them replaced by new copies): { $names }

repo-workflows-ghost-new-above = { $count ->
        [one] ↑ One new commit is above the rows in view.
       *[other] ↑ { $count } new commits are above the rows in view.
    }

repo-workflows-ghost-new-copies = { $count ->
        [one] One new commit
       *[other] { $count } new commits
    }

repo-workflows-ghost-outcome-lines-02e3b1f7 = No file of the working tree changes.

repo-workflows-ghost-outcome-lines-193731dd = The merge stops

repo-workflows-ghost-outcome-lines-4b54308f = Files of the working tree that change ({ $files-len-1 }): { $listed-files-f-format-f-status-f-2 }

repo-workflows-ghost-outcome-lines-7728bdcc = Stops at step { $c-step-1 } of { $c-of-2 } ({ $describe-commit-3 })

repo-workflows-ghost-outcome-lines-81deb64d = { $name } is made at { $shown-to-1 }

repo-workflows-ghost-outcome-lines-820df962 = Worked out with { $engine-describe-1 }

repo-workflows-ghost-outcome-lines-91af17db = { $name } is deleted

repo-workflows-ghost-outcome-lines-b8d8f166 = Drops { $describe-dropped-1 } — { $dropped-author-2 }

repo-workflows-ghost-outcome-lines-ce73844c = { $name } moves { $short-from-1 } → { $shown-to-2 }

repo-workflows-ghost-outcome-lines-f0e05d95 = the commit becomes empty

repo-workflows-ghost-outcome-lines-f93fde8f = conflicts in { $listed-c-files-f-f-clone-1 }

repo-workflows-ghost-pull-3a19c0c2 = Uncommitted changes are stashed first and put back after.

repo-workflows-ghost-pull-9045dbaa = As of the last fetch of { $up-remote-1 }: what was pushed there since isn't shown.

repo-workflows-ghost-pull-c7917d08 = Fast-forward: the branch moves to its remote's commit.

repo-workflows-ghost-pull-dd489c0a = Already up to date: the branch has everything its remote has.

repo-workflows-ghost-pull-f53705f4 = { $branch-name-1 } tracks no branch on a remote: nothing to pull

repo-workflows-ghost-rebase-1f963916 = Already up to date: nothing to replay, nothing changes.

repo-workflows-ghost-rebase-b1f8a147 = the rebase refuses to start with uncommitted changes: commit or stash them first

repo-workflows-ghost-rebase-e550da8a = Uncommitted changes are stashed first and put back after (rebase.autoStash).

repo-workflows-ghost-replaced-originals = Replaced by new copies ({ $count }; { $number ->
        [one] the original is left on no branch
       *[other] the originals are left on no branch
    }): { $names }

repo-workflows-ghost-reset-hard-3126bbde = Uncommitted changes to these files are thrown away, and can't be brought back.

repo-workflows-ghost-rewrite-3477e4cb = The rebase stops there, asking whether to keep the commit that became empty; the branch doesn't move yet.

repo-workflows-ghost-rewrite-3d8b966c = rebase.rebaseMerges is set and merges are among the commits: git keeps them, which the preview can't replay yet

repo-workflows-ghost-rewrite-af60968a = The files that change aren't listed: git { $self-version-1 } replays in a scratch clone.

repo-workflows-ghost-rewrite-c84070ef = { $w-picks-len-1 } commits to replay, too many to preview (at most { $PICKS-LIMIT })

repo-workflows-ghost-rewrite-e69265a2 = The rebase stops there until the conflicts are resolved; the branch doesn't move yet.

repo-workflows-ghost-rewrite-fb29af2f = Meanwhile HEAD is detached at { $waits }, mid-rebase.

repo-workflows-ghost-shown-4ef6715c = a merge commit

repo-workflows-ghost-timeline-18df3f5f = File-change preview is incomplete: required blobs are unavailable locally or the bounded read stopped. No objects were downloaded.

repo-workflows-ghost-timeline-48223518 = Invalid timeline commit ID

repo-workflows-ghost-timeline-5a1baa67 = Timeline preview is incomplete: required local history is missing or the bounded read stopped. Hover never downloads objects.

repo-workflows-ghost-timeline-6f3d9015 = Uncommitted changes are preserved with --keep; a conflicting checkout will refuse the jump.

repo-workflows-ghost-timeline-c25867a7 = Saved refs keep earlier states recoverable. This preview moves no refs or files.

repo-workflows-local_ci-submit-84f8f92f = Cancelled before starting

repo-workflows-postcards-bounded-graph-8e2ed023 = Postcard rendering cancelled

repo-workflows-saved_views-poll-33647b3e = History worker panicked

repo-workflows-saved_views-poll-da12ac51 = History worker stopped

repo-workflows-saved_views-poll-dc101692 = History read cancelled

repo-workflows-agent-checkout-734452f6 = Checkout { $branch }

repo-workflows-agent-commit-4475f515 = Amend

repo-workflows-agent-commit-82a9c46f = Commit

repo-workflows-agent-create-branch-d2a49cac = Create branch { $name }

repo-workflows-agent-delete-branch-d32d800a = Delete branch { $name }

repo-workflows-agent-plan-0026c505 = Revert

repo-workflows-agent-plan-4c49fdb6 = Cherry-pick

repo-workflows-agent-push-731ce7ed = Push

repo-workflows-agent-rename-branch-5de8c7a7 = Rename { $from } to { $to }

repo-workflows-agent-rewrite-2630fe5c = Move commit down

repo-workflows-agent-rewrite-3d1ccb79 = Move commit up

repo-workflows-agent-rewrite-601e2a07 = Drop commit

repo-workflows-agent-stage-3b4ddea7 = Stage { $paths-label-paths-1 }

repo-workflows-agent-stash-38bdfeaa = Stash

repo-workflows-agent-unstage-eb003390 = Unstage { $paths-label-paths-1 }

repo-workflows-dco-dco-op-done-5d0c9ff1 = Sign off the commits of { $plan-branch-1 } (here only)

repo-workflows-dco-fix-label-dfc76b8d = Sign off the commits of { $plan-branch-1 }

repo-workflows-devcontainer-devcontainer-act-519a489a = Dev container operation

repo-workflows-doctor-scope-choice-0eb26325 = This worktree

repo-workflows-doctor-scope-choice-38429e22 = This repository

repo-workflows-doctor-scope-choice-59b8031f = All my repositories

repo-workflows-doctor-scope-choice-ad944dcb = Everyone on this machine

repo-workflows-doctor-scope-choice-c925a118 = Command line

repo-workflows-email-email-act-c19c9f05 = Email patches · b4

repo-workflows-fence_view-fence-gate-6b730ef5 = Checking against the agent fence

repo-workflows-lfs_locks-lock-job-36f5baee = { $label }: done

repo-workflows-line_dossier-append-dossier-ignore-716419f1 = Appending reviewed ignore revision

repo-workflows-operation-distribute-staged = Distribute staged fixes into { $count ->
        [one] one commit
       *[other] { $count } commits
    }

repo-workflows-operation-finish-hotfix = Finish hotfix { $tag }

repo-workflows-operation-force-unlock = Force unlock { $path }

repo-workflows-operation-force-unlock-count = Force unlock { $count ->
        [one] one file
       *[other] { $count } files
    }

repo-workflows-operation-here-only = { $operation } (here only)

repo-workflows-operation-lfs-prune = LFS: Prune old objects

repo-workflows-operation-lfs-track = Track { $pattern } in LFS

repo-workflows-operation-mark-bisect = Mark in the bisect

repo-workflows-operation-push-hotfix = Push hotfix { $tag }

repo-workflows-operation-push-rebased = Push the rebased { $branch } to { $remote }

repo-workflows-operation-rebase-fork-push = Rebase #{ $number } onto { $base } and push

repo-workflows-operation-snapshot = Snapshot

repo-workflows-operation-ssh-14017 = Create SSH key

repo-workflows-operation-ssh-14019 = Create SSH key there

repo-workflows-operation-ssh-14022 = Add selected SSH key to agent

repo-workflows-operation-ssh-14024 = Add SSH key to agent there

repo-workflows-operation-ssh-14036 = Test selected SSH key

repo-workflows-operation-ssh-14038 = Test selected SSH key there

repo-workflows-operation-ssh-14048 = Register public SSH key there

repo-workflows-operation-start-issue-work = Start work on { $issue }

repo-workflows-partial_stash-label-057347af = Stash staged

repo-workflows-partial_stash-label-598787f5 = Stash files

repo-workflows-push-chain-step-ended-dfa32698 = { $what } failed, so nothing was pushed: { $e }

repo-workflows-push-check-remote-9a7c53eb = Checking { $what }

repo-workflows-push-headline = { $clear ->
        [yes] { $remote } has nothing you don't have now: push again
       *[no] { $remote } has { $count ->
            [one] 1 commit
           *[other] { $count } commits
        } you don't have
    }

repo-workflows-push-push-act-ebd6e3a6 = { $r-branch-1 } pulled from { $r-target-label-2 } and pushed

repo-workflows-push-push-labelled-4933a6c9 = This repository has no remote to push to

repo-workflows-push-push-op-ended-79357f2e = { $label } refused: { $out-trim-1 }

repo-workflows-push-remote-checked-205a41fa = Push refused: { $target-label-1 } is gone

repo-workflows-push-remote-checked-3bbec358 = Push refused: { $banner-headline-r-1 }

repo-workflows-push-remote-checked-c6e528c9 = Push refused, and the remote could not be checked: { $e }

repo-workflows-push-sync-027fe2bd = Sync: { $current } pulled and pushed

repo-workflows-push-sync-5ca83b4b = Sync: { $current } published (it had nothing to pull)

repo-workflows-reflog-apply-stash-93a6ce40 = Apply stash { $short-pick-hash-1 }

repo-workflows-reflog-start-scan-7a1d64fe = The last scan is still ending on that machine; try again in a moment

repo-workflows-rehearse-apply-rehearsal-b09a6f3f = Commit or stash your changes first: the rehearsal was of the commits alone

repo-workflows-rehearse-apply-rehearsal-d63206ac = { $run-branch-1 } is no longer checked out: rehearse again

repo-workflows-rehearse-start-rehearsal-252af634 = Check out a branch to rehearse with it

repo-workflows-rehearse-start-rehearsal-bbfc5a1d = A rehearsal is going on already

repo-workflows-release-release-stopped-1904d12e = Released { $run-plan-tag-1 }

repo-workflows-release-release-stopped-328c7ab5 = { $label }: stopped

repo-workflows-release-release-stopped-a4afd9be = { $label }: { $step-label-1 } failed: { $e }

repo-workflows-release-run-release-d9523bf2 = Wait for the operation running to end

repo-workflows-repository_tools-publish-group-3cc9a2c6 = Publish group

repo-workflows-ssh_setup-background-e4e2ef57 = Working on the selected machine…

repo-workflows-ssh_setup-label-03ff732f = Register public key

repo-workflows-ssh_setup-label-1db4242b = Add to agent

repo-workflows-ssh_setup-label-60083f45 = Test SSH

repo-workflows-ssh_setup-label-b6cde903 = Create key

repo-workflows-ssh_setup-launch-0690e36b = Could not start the selected machine's terminal/tool. Check that OpenSSH/WSL is installed. A reserved new folder, if any, is retained; existing files were not overwritten.

repo-workflows-ssh_setup-launch-2a66f8c5 = { $step-label-1 } is running. Enter any passphrase only in the terminal below.

repo-workflows-ssh_setup-open-ssh-setup-0e2de7fb = SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions.

repo-workflows-ssh_setup-poll-59faaa41 = Checked account: { $account-owner-1 } on { $self-host-2 }. Upload is a separate action.

repo-workflows-ssh_setup-poll-77510276 = This profile now selects that local SSH key. Author/signing and repository-local SSH overrides retain their existing precedence.

repo-workflows-ssh_setup-poll-acae7792 = SSH test exited { $code }. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled.

repo-workflows-ssh_setup-poll-b3badea2 = { $run-step-label-1 } command completed successfully. Key files are retained; continue with the next explicit step.

repo-workflows-ssh_setup-poll-b5bb4cca = { $run-step-label-1 } exited { $code }. Read the terminal's explanation. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine.

repo-workflows-ssh_setup-poll-b927d5a6 = The active profile or its connection choices changed. Check the intended account again before uploading.

repo-workflows-ssh_setup-poll-cf037753 = The selected public key was registered. This does not enable commit signing or change a global account.

repo-workflows-stack-abort-restack-4fe4c0de = Abort restack

repo-workflows-stack-open-publish-3e534e60 = { $layer-parent-1 } is gone: pick what { $layer-branch-2 } is based on first.

repo-workflows-stack-open-publish-a23e7f8b = This repository isn't on a forge gitcraft knows: there is nowhere to publish to.

repo-workflows-stack-open-restack-419e0d8e = Nothing to restack: every layer is on its parent's tip.

repo-workflows-stack-publish-done-6ffb5e8c = { $report-done-len-1 } of { $total } done, then { $branch } failed: { $e }. Publish again to go on: what was done stays, no { $noun } is opened twice.

repo-workflows-stack-remove-parent-4272623a = Couldn't read the stacks: { $e }

repo-workflows-stack-remove-parent-e6733ec1 = Remove { $branch } from its stack

repo-workflows-stack-restack-done-1fc5c0ba = Restack: every branch was already on its parent's tip

repo-workflows-stack-restack-done-33b4b547 = Restack stopped at { $branch } on conflicts: resolve them{ $place }, then Continue restack (or Abort restack)

repo-workflows-stack-restack-done-6cecc855 = { " " }in its worktree at { $wt-display-1 }

repo-workflows-stack-restack-done-7f6cce25 = { $n ->
        [one] Restacked: one branch moved
       *[other] Restacked: { $n } branches moved
    } · Ctrl+Z puts them back

repo-workflows-stack-restack-done-a7222989 = Restack aborted: every branch is back where it was

repo-workflows-stack-restack-done-c27578ee = Restack aborted: every branch is back where it was but { $kept-join-1 }, which got commits after it was restacked and is left as it is

repo-workflows-stack-restack-done-dbfb6ce0 = Restack: 1 branch moved · Ctrl+Z puts it back

repo-workflows-stack-run-stack-op-95c7214f = { $label }: run it again once { $busy } is done

repo-workflows-stack-set-parent-81fef295 = Base { $branch } on { $parent }

repo-workflows-stack-start-publish-c4ffa0c1 = Publish { $noun }s

repo-workflows-stack-start-publish-e0c26cc9 = Publish: run it again once { $busy } is done

repo-workflows-stack-start-restack-9fbc2915 = Restack

repo-workflows-stack-summary-47eeff3f = Publish { $noun }s: nothing to do

repo-workflows-stack-summary-7f75368f = Published { $noun }s: { $parts-join-1 }

repo-workflows-status-dco-local-only = { $branch }’s commits are signed off here, but not pushed{ $target ->
        [yes] { " to " }{ $remote }
       *[no] { "" }
    }: Ctrl+Z puts them back as they were

repo-workflows-status-forks-none-to-remove = No fork remote can go{ $kept ->
        [yes] { "\u000A\u000AKept: " }{ $names }, whose branches have commits no branch here has.
       *[no] { "" }
    }

repo-workflows-status-rewrite-retention = { $operation }: done, but its earlier version couldn’t be kept: { $detail }

repo-workflows-surgery-open-absorb-2cddd12d = Distribute staged fixes: close the editor that is open first

repo-workflows-surgery-open-absorb-3373a6f7 = Distribute staged fixes: HEAD is detached, check out a branch first

repo-workflows-surgery-open-absorb-55d2e258 = Distribute staged fixes: nothing is staged, stage the fixes first

repo-workflows-surgery-open-absorb-55f28458 = Distribute staged fixes: another editor opened meanwhile

repo-workflows-surgery-open-absorb-5ebbefda = Distribute staged fixes: reading…

repo-workflows-surgery-open-absorb-ed3b6cd2 = Distribute staged fixes: { $e }

repo-workflows-surgery-open-absorb-edcc444b = Distribute staged fixes: a { $state-label-to-lowercase-1 } is in progress

repo-workflows-surgery-open-move-9a352cd5 = Move changes: { $why }

repo-workflows-surgery-open-move-9a3c16a9 = Move changes: reading { $short-hash-1 }…

repo-workflows-surgery-open-split-01ee6f3c = Split commit: close the staged fixes editor first

repo-workflows-surgery-open-split-a472a361 = Split commit: reading { $short-hash-1 }…

repo-workflows-surgery-open-split-ad93af6b = Split commit: { $why }

repo-workflows-surgery-run-move-afb8b0ad = Move changes from { $short-hash-1 } to the working changes

repo-workflows-surgery-run-move-b5726a1d = Move changes from { $short-hash-1 } to { $short-t-hash-2 }

repo-workflows-surgery-run-split-ddd763c5 = Split { $short-hash-1 } into { $messages-len-2 }

repo-workflows-test_lane-start-tests-4b88f5c5 = Type the command to test with

repo-workflows-test_lane-start-tests-aa89f17f = Tests are running already: stop them first

repo-workflows-timeline-authors-changed = { $count ->
        [one] one author changed
       *[other] { $count } authors changed
    }

repo-workflows-timeline-commits-signed = { $count ->
        [one] one commit now signed
       *[other] { $count } commits now signed
    }

repo-workflows-timeline-describe-reflog-32fa2259 = Reset to { $to }

repo-workflows-timeline-describe-reflog-43695d49 = Merge { $w-merge-len-1 }

repo-workflows-timeline-describe-reflog-56bb1bb2 = Branch made

repo-workflows-timeline-describe-reflog-8e9d44a6 = Merge commit

repo-workflows-timeline-describe-reflog-99e71f48 = Checkout

repo-workflows-timeline-describe-reflog-9eb5b275 = Rebase

repo-workflows-timeline-describe-reflog-b11c9047 = Moved

repo-workflows-timeline-describe-reflog-b66d0441 = Pull

repo-workflows-timeline-describe-reflog-daee7606 = Reset

repo-workflows-timeline-discard-remove-66f54467 = Discard { $branch }

repo-workflows-timeline-fallback-label-1ac54e67 = Branch { $c-name-1 } made

repo-workflows-timeline-fallback-label-b5bd817d = Branch { $c-name-1 } deleted

repo-workflows-timeline-fallback-label-e48d463d = { $c-name-1 } moved

repo-workflows-timeline-jump-a5337838 = Going there

repo-workflows-timeline-jump-b77dfff9 = { $label }: that would skip “{ $skipped }”, which moved no branch: step through it with { $cmd }+Z / { $cmd }+Y instead

repo-workflows-timeline-jump-target-1e4e36b2 = Go to “{ $self-timeline-get-id-label-1 }”

repo-workflows-timeline-jump-target-bd63e54c = Go to before the timeline

repo-workflows-timeline-messages-changed = { $count ->
        [one] one message changed
       *[other] { $count } messages changed
    }

repo-workflows-timeline-outside-label-d1d10f56 = { $first }, and { $rest-len-1 } more

repo-workflows-timeline-put-branch-back-94acd0c3 = Put { $branch } back at { $crate-repo-short-commit-1 }

repo-workflows-timeline-redo-2b66f1cb = Redo { $entry-label-1 }

repo-workflows-timeline-redo-4aaf2710 = Nothing to redo

repo-workflows-timeline-redo-74273989 = Redo

repo-workflows-timeline-run-remove-6fd826a1 = Pick a name no branch has for the new branch (not “{ $name }”)

repo-workflows-timeline-run-remove-73ccc5e0 = Remove “{ $self-entry-label-plan-id-1 }” from { $plan-branch-2 }, as { $name }

repo-workflows-timeline-sentence-31b36481 = final files identical

repo-workflows-timeline-sentence-360c0cec = messages kept

repo-workflows-timeline-sentence-4a61e684 = too many commits to follow one by one

repo-workflows-timeline-sentence-99e79f74 = final files differ

repo-workflows-timeline-sentence-b5ea0271 = { $old ->
        [one] one commit
       *[other] { $old } commits
    } → { $new }

repo-workflows-timeline-sentence-d30ebcc6 = authors kept

repo-workflows-timeline-show-receipt-e172bf9c = { $label }: done. { $receipt-sentence-1 } (receipt in the timeline, Ctrl+Shift+Z)

repo-workflows-timeline-signatures-33c87627 = signatures replaced, { $a-b-1 } more signed

repo-workflows-timeline-signatures-aa14a1ba = signatures replaced

repo-workflows-timeline-signatures-b59d95ba = signatures replaced, { $b-a-1 } dropped

repo-workflows-timeline-signatures-dropped = { $count ->
        [one] one signature dropped
       *[other] { $count } signatures dropped
    }

repo-workflows-timeline-timeline-op-ended-be0297e1 = { $label } (stopped partway)

repo-workflows-timeline-timeline-unsettled-21705388 = { $what }: waiting for the repository to be read again…

repo-workflows-timeline-timeline-unsettled-3fd5b04c = { $what }: the repository couldn't be read again; reload it (F5) first

repo-workflows-timeline-toggle-timeline-d12d6347 = Finish or close the conflict or the rebase editor first to see the timeline

repo-workflows-timeline-tree-rows-6fa556f4 = Before the oldest entry

repo-workflows-timeline-undo-83290078 = Nothing to undo

repo-workflows-timeline-undo-a8283ade = Undo

repo-workflows-timeline-undo-bf959af0 = Undo { $entry-label-1 }

repo-workflows-tour-tour-view-6df591db = Git LFS

repo-workflows-tour-tour-view-7bfe85f2 = Initialize submodules

repo-workflows-tour-tour-view-952aa9de = Install local Git LFS hooks

repo-workflows-unity-add-attributes-0b9b6295 = Adding lines to .gitattributes

repo-workflows-unity-add-ignores-52181c32 = Adding lines to .gitignore

repo-workflows-unity-choose-tool-34706d9f = Saving the choice

repo-workflows-unity-choose-tool-af82d6a2 = { $file-display-1 } isn't UnityYAMLMerge

repo-workflows-unity-register-driver-5d4e5077 = Registering UnityYAMLMerge

repo-workflows-unity-register-driver-f6c55b48 = UnityYAMLMerge wasn't found: choose it first

repo-workflows-unity-smart-merge-conflict-5a10252d = UnityYAMLMerge wasn't found: choose it in Unity set-up…

repo-workflows-unity-smart-merge-done-7e870a3e = UnityYAMLMerge left { $n } conflicts in { $path } to choose

repo-workflows-unity-smart-merge-done-afe126d0 = UnityYAMLMerge merged { $path }: check the result, then save

repo-workflows-unity-unity-job-bfc4a245 = { $failed }: { $e }

repo-workflows-unity-untrack-generated-b0c9a9c5 = Taking generated folders out of git

repo-workflows-versions-mark-pr-seen-93fe69bf = Mark as seen: #{ $number } isn't in the list of this repository's pull requests

repo-workflows-versions-open-branch-versions-70c06512 = Compare versions: { $e }

repo-workflows-versions-open-pr-since-1e0c2674 = What changed in #{ $number }: it isn't in the list of this repository's pull requests

repo-workflows-versions-open-pr-since-60afdf08 = Reading #{ $number }…

repo-workflows-versions-open-pr-since-e9aaff48 = What changed in #{ $number }: { $e }

repo-workflows-versions-open-pr-since-f8a6567c = #{ $number } hasn't changed since you last looked

repo-workflows-versions-seen-marked-f9ed1e57 = Couldn't remember the version of #{ $number } seen: { $e }

repo-workflows-working_drafts-change-working-draft-278865c0 = Working draft

repo-workflows-working_drafts-poll-working-drafts-7a49e8a4 = Earlier versions: { $e }

repo-workflows-working_drafts-save-working-draft-3e48426a = Saved manually

repo-workflows-forks-checkout-fork-abaaf4c3 = Check out #{ $number } to push to { $info-owner-1 }

repo-workflows-agents-turn-diff-dc601e7b = before turn { $turn-number-1 }

repo-workflows-agents-turn-diff-9021b8b9 = turn { $turn-number-1 }: { $label }

repo-workflows-agents-turn-chip-60c8b8ae = after turn { $turn-number-1 }

repo-workflows-agents-turn-chip-ed5eb9a3 = now

repo-workflows-repository_tools-show-507eaa08 = original { $s-branch-1 }

repo-workflows-repository_tools-show-7bd990ec = candidate { $s-branch-1 }
