interaction-guards-collision-invalid-pair = Could not merge { $a } with { $b } in memory: one of them is not a branch or commit

interaction-guards-collision-merge-empty = Could not merge { $a } with { $b } in memory

interaction-guards-collision-merge-error = Could not merge { $a } with { $b } in memory: { $detail }

interaction-guards-collision-needs-git = The collision matrix needs git 2.38 or later (git merge-tree --write-tree)

interaction-guards-collision-no-history = { $a } and { $b } share no history

interaction-guards-collision-not-branch = { $reference } is not a branch

interaction-guards-collision-not-merged = Not merged

interaction-guards-collision-stopped = Stopped

interaction-guards-drift-git-folder = Could not find the worktree's git folder: { $detail }

interaction-guards-drift-headline = { $files ->
        [one] One file changed
       *[other] { $files } files changed
    }{ $unrelated ->
        [yes] , { $unrelated_files ->
            [one] one file
           *[other] { $unrelated_files } files
        } in { $folders ->
            [one] one folder
           *[other] { $folders } folders
        } the task doesn't mention
       *[no] { "" }
    }, added { $dependencies ->
        [one] one dependency
       *[other] { $dependencies } dependencies
    }, changed { $signatures ->
        [one] one public signature
       *[other] { $signatures } public signatures
    }, { $percent }% formatting only

interaction-guards-drift-issue-error = Could not read issue #{ $issue }: { $detail }

interaction-guards-drift-measure-error = Could not measure the worktree's changes: { $detail }

interaction-guards-drift-missing-tools = No coding agent's command line is installed ({ $tools }): the figures above are the exact part

interaction-guards-drift-no-base = There is no main branch commit to compare the worktree with

interaction-guards-drift-not-revision = { $reference } is not a revision

interaction-guards-drift-read-task = Could not read the worktree's task: { $detail }

interaction-guards-drift-run-error = Could not run { $tool }: { $detail }

interaction-guards-drift-save-task = Could not save the worktree's task: { $detail }

interaction-guards-drift-timeout = { $tool } took longer than { $seconds } s

interaction-guards-drift-tool-failed = { $tool } failed: { $detail }

interaction-guards-drift-unsupported-forge = Issues of { $forge } aren't read yet

interaction-guards-drift-wait-error = Could not wait for { $tool }: { $detail }

interaction-guards-fence-check-error = Could not check { $target } against the agent fence: { $detail }

interaction-guards-fence-deny-list = "permissions.deny" in .claude/settings.local.json isn't a list

interaction-guards-fence-folder-error = Could not create the folder of { $path }: { $detail }

interaction-guards-fence-git-folder = Could not find the worktree's git folder: { $detail }

interaction-guards-fence-how-committed = committed

interaction-guards-fence-how-ignored = ignored by git

interaction-guards-fence-how-uncommitted = not committed

interaction-guards-fence-how-untracked = untracked

interaction-guards-fence-invalid-json = .claude/settings.local.json isn't valid JSON: { $detail }

interaction-guards-fence-line-break = { $entry } has a line break

interaction-guards-fence-no-base = there is no main branch to compare with

interaction-guards-fence-no-exceptions = { $pattern }: a fence takes no exceptions (`!`); list what is fenced instead

interaction-guards-fence-no-folder = the worktree's folder isn't there

interaction-guards-fence-no-history = it shares no history with the main branch

interaction-guards-fence-not-object = .claude/settings.local.json isn't a JSON object

interaction-guards-fence-not-revisions = { $base } or { $tip } is not a revision

interaction-guards-fence-permissions-object = "permissions" in .claude/settings.local.json isn't an object

interaction-guards-fence-reach-error = Could not reach the worktree's folder: { $detail }

interaction-guards-fence-read-error = Could not read { $path }: { $detail }

interaction-guards-fence-read-fence = Could not read the agent fence: { $detail }

interaction-guards-fence-record = the fence's record

interaction-guards-fence-record-error = Could not record the rules written: { $detail }

interaction-guards-fence-save-fence = Could not save the agent fence: { $detail }

interaction-guards-fence-save-targets = Could not save where the fence is written: { $detail }

interaction-guards-fence-write-error = Could not write { $path }: { $detail }
