# Interaction guard presentation only. Raw control strings, receipts, commands,
# source evidence and persisted agent/recovery instructions are intentionally unchanged.
# Numeric variables: counts/line numbers/percentages. OIDs/refs/paths/issue IDs,
# source/config/command lexemes, sizes already formatted by Git, and diagnostics are opaque.

interaction-guards-push-preview-failed = The local push preview could not be completed. Fetch missing history explicitly, then try again; no push was run.

interaction-guards-push-config-read = The local push configuration could not be read; no push was run.

interaction-guards-push-complete-destination = Choose one complete destination ref, such as refs/heads/main.

interaction-guards-push-routing-limit = The push routing configuration exceeds the preview's size limit.

interaction-guards-push-control-characters = An identity or routing value contains control characters. Correct it before opening a push preview.

interaction-guards-push-single-ref = This push option needs an explicit single-ref preview; multi-ref, matching, mirror, atomic and recursive pushes are not sent by this checklist.

interaction-guards-push-one-destination = Preview one destination at a time; no push was run.

interaction-guards-push-no-overrides = Preview explicit refs without temporary push configuration overrides.

interaction-guards-push-finish-operation = Complete this local operation first, then preview its push separately.

interaction-guards-push-one-push = Preview one push at a time; no push was run.

interaction-guards-push-detached-route = Choose an explicit remote and source ref when HEAD is detached.

interaction-guards-push-choose-remote = Choose a remote before pushing this branch.

interaction-guards-push-remote-removed = The push remote is no longer configured; reopen the preview.

interaction-guards-push-mirror = Mirror pushes need their own multi-ref review; no push was run.

interaction-guards-push-address-limit = Preview at most eight configured push addresses; split larger mirror groups into separate remotes.

interaction-guards-push-no-address = The remote has no supported push address.

interaction-guards-push-detached-source = Choose an explicit source ref when HEAD is detached.

interaction-guards-push-multiple-refs = The remote pushes multiple refs. Choose one explicit branch in its menu for preflight.

interaction-guards-push-matching = Matching pushes need a separate preview for each branch.

interaction-guards-push-upstream-name = The upstream branch has a different name; choose its explicit branch push from the menu.

interaction-guards-push-no-wildcards = Choose one complete ref; wildcard pushes are not sent.

interaction-guards-push-delete-upstream = A deletion cannot set an upstream.

interaction-guards-push-delete-lease-required = Deletion needs an exact ref lease

interaction-guards-push-delete-lease = The deletion lease must name this exact ref and object.

interaction-guards-push-delete-target-changed = The fetched target differs from the deletion lease; reopen its original action.

interaction-guards-push-delete-fetch = Fetch this exact target before deleting it; no remote tip was inspected.

interaction-guards-push-commit-limit = The outgoing history exceeds 10,000 commits; split the push or fetch its exact target before checking.

interaction-guards-push-lfs-file = Committed .lfsconfig must be a regular file before its endpoint can be reviewed.

interaction-guards-push-lfs-control = The LFS endpoint contains control characters.

interaction-guards-push-no-target = No push target

interaction-guards-push-checkpoint-limit = Checkpoint reference list exceeds 1 MiB; narrow publication before pushing

interaction-guards-push-transcript = Checkpoint publication may disclose prompts, local paths, assistant summaries and tool activity. This exact object or route contains agent checkpoint history. Review its contents and recipients explicitly before sending; local prompt/session viewing itself never publishes transcripts.

interaction-guards-push-tag-secret = Likely secret in annotated tag message; value withheld. Recreate the tag before pushing.

interaction-guards-push-metadata-limit = Outgoing commit metadata exceeds the bounded preview size; shorten large messages or split the push.

interaction-guards-push-metadata-unsafe = Commit metadata could not be read safely.

interaction-guards-push-unsigned = Commit has no embedded signature; this profile signs commits.

interaction-guards-push-wip = Temporary WIP/fixup message remains in outgoing history.

interaction-guards-push-message-secret = Likely secret in commit message (value withheld).

interaction-guards-push-finding-limit = More than 2,000 findings need review. Repair or split this history before pushing.

interaction-guards-push-object-limit = More than 50,000 outgoing objects need checking. Split the push or fetch the exact target.

interaction-guards-push-blob-size = Unknown outgoing blob size

interaction-guards-push-lfs-hash-unsupported = Unsupported LFS object hash

interaction-guards-push-lfs-hash-malformed = Malformed LFS object hash

interaction-guards-push-config-changed = The push configuration changed while checking. Reopen preflight.

interaction-guards-push-source-changed = The source changed while checking. Reopen preflight.

interaction-guards-push-review-changed = The selected push endpoint, commit or advertised lease changed. Fetch the exact target if needed, then reopen the original push action; nothing was sent.

interaction-guards-push-object-invalid = Invalid outgoing object

interaction-guards-push-scan-failed = The content scan failed or exceeded its finding limit; no push was run.

interaction-guards-push-reopen = Reopen push preflight

interaction-guards-push-expired = This preview expired; reopen push preflight.

interaction-guards-push-lfs-working = Commit or remove .lfsconfig changes, and clear skip/assume-unchanged flags on it before reviewing a push.

interaction-guards-push-lfs-differs = The working .lfsconfig differs from the inspected commit. Commit or stash that routing change before pushing.

interaction-guards-push-branch-moved = The checked-out branch moved. Reopen push preflight.

interaction-guards-push-state-changed = The branch, checkout, identity or push configuration changed. Reopen push preflight; nothing was pushed.

interaction-guards-push-addresses-changed = The push addresses changed. Reopen push preflight.

interaction-guards-push-force-fetch = Fetch this exact target before force pushing: no remote lease was inspected.

interaction-guards-push-not-ancestor = The fetched target is not an ancestor. Keep its commits or deliberately choose force-with-lease.

interaction-guards-push-echo-mismatch = Reviewer echo belongs to a different exact push; prepare again

interaction-guards-push-route-missing = Reviewed route was not captured

interaction-guards-push-completed = Push completed to the reviewed target(s).

interaction-guards-push-clock = No clock

interaction-guards-push-prepare-fix = Could not prepare the history fix

interaction-guards-push-prepare-remote-fix = Could not prepare the remote history fix

interaction-guards-push-no-fix-folder = No fix folder

interaction-guards-push-checkout-fix = Check out this branch before fixing its history, then reopen preflight.

interaction-guards-push-clean-fix = Commit or stash all working changes and untracked files before rewriting outgoing history.

interaction-guards-push-fetch-first = Fetch the target first

interaction-guards-push-merged = Merged the fetched target; reopen preflight. Ctrl+Z undoes this merge.

interaction-guards-push-merge-attention = The merge needs attention. Resolve conflicts or abort it before reopening preflight.

interaction-guards-push-unknown-fix = Unknown history fix

interaction-guards-push-secrets-before-lfs = Resolve likely-secret findings before moving files to LFS; LFS must not conceal a known secret.

interaction-guards-push-rewrite-failed = History rewrite failed before the branch was updated; check signing availability or missing local objects.

interaction-guards-push-malformed-commit = Malformed commit

interaction-guards-push-no-tree = No commit tree

interaction-guards-push-lfs-regular = LFS conversion supports regular files only; no history was changed.

interaction-guards-push-quote-attributes = Could not quote attributes

interaction-guards-push-malformed-author = Malformed author metadata

interaction-guards-push-set-author = Set the expected host author name and email in Profile settings before amending history.

interaction-guards-push-no-commits = There are no outgoing commits to fix

interaction-guards-push-fixed = Outgoing history fixed. Reopen preflight to verify it; Ctrl+Z restores the previous history.

interaction-guards-push-remote-ownership = Automatic remote repair requires completion-aware ownership; no remote repair was dispatched. Preview and reviewed push remain available.

interaction-guards-push-utf8-metadata = Safe repair requires UTF-8 metadata paths

interaction-guards-push-metadata-controls = Safe repair requires metadata paths without control characters

interaction-guards-push-repair-refused = The guarded Git repair command was refused; tool diagnostics were withheld.

interaction-guards-push-index-snapshot = Could not snapshot the original index

interaction-guards-push-index-read = Could not read the original index

interaction-guards-push-index-limit = Index recovery snapshot exceeds 64 MiB

interaction-guards-push-index-save = Could not save the owned index recovery snapshot

interaction-guards-push-index-dependency = Saved index dependency was not an absolute path; repair refused

interaction-guards-push-git-capability = Guarded repair requires Git 2.50 or later plus a successful native HEAD/ref capability proof. No repair reference update was launched.

interaction-guards-push-backend-proof = This Git reference/object backend cannot be proven safe for automatic repair

interaction-guards-push-direct-branch = Automatic repair requires a direct local branch reference

interaction-guards-push-absolute-metadata = Git did not provide absolute metadata paths; safe repair was refused

interaction-guards-push-index-flags = Clear sparse/skip-worktree/assume-unchanged index flags before automatic tree repair

interaction-guards-push-utf8-changed = Safe repair requires UTF-8 changed paths

interaction-guards-push-create-evidence = Could not create owned repair evidence

interaction-guards-push-index-utf8 = Owned index path is not UTF-8

interaction-guards-push-recovery-write = Could not write recovery instructions

interaction-guards-push-hook-write = Could not write the private repair hook

interaction-guards-push-hook-protect = Could not protect repair hook

interaction-guards-push-arm-local = Could not arm local repair

interaction-guards-push-capability-input = Could not write capability input

interaction-guards-push-arm-capability = Could not arm capability proof

interaction-guards-push-hook-command-proof = Could not write owned hook-command proof

interaction-guards-push-capability-protect = Could not protect owned capability hook

interaction-guards-push-hook-stdin = Could not prepare hook stdin proof

interaction-guards-push-hook-support = Git did not prove original-hook command/stdin support; no user update was launched

interaction-guards-push-native-proof = This Git/backend did not prove private hook selection and an atomic current-HEAD/branch update. No user reference update was launched.

interaction-guards-push-missing-proof = Missing native transaction input proof

interaction-guards-push-transaction-proof = The native HEAD/branch transaction result was not proven; no user update was launched

interaction-guards-push-capability-ack = No native checkout/ref capability acknowledgement; repair refused before dispatch

interaction-guards-push-remote-metadata-result = Could not establish the remote repair metadata result; no repair transaction was dispatched

interaction-guards-push-remote-metadata = Remote repair metadata is unavailable or oversized

interaction-guards-push-remote-absolute = Remote repair requires absolute host metadata paths

interaction-guards-push-remote-git = Guarded remote repair requires Git 2.50 or later on the repository host

interaction-guards-push-repair-oid = Repair object identity is invalid

interaction-guards-push-shared-absolute = Shared-index path is not absolute

interaction-guards-push-remote-backend = Unsupported remote repair backend

interaction-guards-push-remote-utf8 = Remote repair requires UTF-8 changed paths

interaction-guards-push-wrapper-limit = Remote repair wrapper exceeds its size limit

interaction-guards-push-index-parent = No index parent

interaction-guards-push-pending = Resolve pending remote repairs before starting another

interaction-guards-push-identity = Author email differs from the profile policy for { $target }.

interaction-guards-push-large = { $size } Git blob; consider Git LFS.

interaction-guards-push-secret-line = Likely secret at line { $line }; value withheld.

interaction-guards-push-remote-history = Force push would drop { $count } remote commits, { $others } by other authors. Based on the fetched target; the lease must still match.

interaction-guards-push-local-uncertain = The local repair did not establish complete owned acknowledgement. Preserve concurrent edits; inspect recovery.txt and saved index/dependency in { $directory }. No automatic rollback overwrote later work.

interaction-guards-push-remote-held = A previous remote repair has no confirmed terminal outcome. Recheck after the host finishes, or inspect recovery in { $directory }. No replacement repair was dispatched.

interaction-guards-push-remote-uncertain = The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in { $directory }. Recheck after the host finishes; no replacement or cleanup was dispatched.

interaction-guards-push-remote-incomplete = Remote repair did not establish complete success. Inspect actual HEAD and concurrent edits, plus recovery in { $directory }; no rollback overwrote later work.

interaction-guards-push-authentication-failed = Authentication failed for the reviewed push target.

interaction-guards-push-local-target = Local repository: { $path }

interaction-guards-push-sensitive-path = [sensitive path]

interaction-guards-drift-no-base = There is no main branch commit to compare the worktree with

interaction-guards-fence-no-base = there is no main branch to compare with

interaction-guards-fence-no-history = it shares no history with the main branch

interaction-guards-fence-no-folder = the worktree's folder isn't there

interaction-guards-fence-not-object = .claude/settings.local.json isn't a JSON object

interaction-guards-fence-permissions-object = "permissions" in .claude/settings.local.json isn't an object

interaction-guards-fence-deny-list = "permissions.deny" in .claude/settings.local.json isn't a list

interaction-guards-collision-stopped = Stopped

interaction-guards-collision-not-merged = Not merged

interaction-guards-collision-needs-git = The collision matrix needs git 2.38 or later (git merge-tree --write-tree)

interaction-guards-authorship-read-all = Could not read who wrote the commits: { $detail }

interaction-guards-authorship-read-one = Could not read who wrote { $oid }: { $detail }

interaction-guards-authorship-figures-error = Could not work out who wrote { $branch }: { $detail }

interaction-guards-authorship-not-branches = { $branch } or { $base } is not a branch

interaction-guards-authorship-not-commit = { $oid } is not a commit

interaction-guards-drift-measure-error = Could not measure the worktree's changes: { $detail }

interaction-guards-drift-not-revision = { $reference } is not a revision

interaction-guards-drift-git-folder = Could not find the worktree's git folder: { $detail }

interaction-guards-drift-read-task = Could not read the worktree's task: { $detail }

interaction-guards-drift-save-task = Could not save the worktree's task: { $detail }

interaction-guards-drift-unsupported-forge = Issues of { $forge } aren't read yet

interaction-guards-drift-issue-error = Could not read issue #{ $issue }: { $detail }

interaction-guards-drift-missing-tools = No coding agent's command line is installed ({ $tools }): the figures above are the exact part

interaction-guards-drift-run-error = Could not run { $tool }: { $detail }

interaction-guards-drift-wait-error = Could not wait for { $tool }: { $detail }

interaction-guards-drift-timeout = { $tool } took longer than { $seconds } s

interaction-guards-drift-tool-failed = { $tool } failed: { $detail }

interaction-guards-fence-no-exceptions = { $pattern }: a fence takes no exceptions (`!`); list what is fenced instead

interaction-guards-fence-line-break = { $entry } has a line break

interaction-guards-fence-check-error = Could not check { $target } against the agent fence: { $detail }

interaction-guards-fence-not-revisions = { $base } or { $tip } is not a revision

interaction-guards-fence-invalid-json = .claude/settings.local.json isn't valid JSON: { $detail }

interaction-guards-fence-git-folder = Could not find the worktree's git folder: { $detail }

interaction-guards-fence-read-error = Could not read { $path }: { $detail }

interaction-guards-fence-folder-error = Could not create the folder of { $path }: { $detail }

interaction-guards-fence-write-error = Could not write { $path }: { $detail }

interaction-guards-fence-reach-error = Could not reach the worktree's folder: { $detail }

interaction-guards-fence-record-error = Could not record the rules written: { $detail }

interaction-guards-fence-read-fence = Could not read the agent fence: { $detail }

interaction-guards-fence-save-fence = Could not save the agent fence: { $detail }

interaction-guards-fence-save-targets = Could not save where the fence is written: { $detail }

interaction-guards-collision-not-branch = { $reference } is not a branch

interaction-guards-collision-invalid-pair = Could not merge { $a } with { $b } in memory: one of them is not a branch or commit

interaction-guards-collision-no-history = { $a } and { $b } share no history

interaction-guards-collision-merge-error = Could not merge { $a } with { $b } in memory: { $detail }

interaction-guards-collision-merge-empty = Could not merge { $a } with { $b } in memory

interaction-guards-weakened-read-error = Could not read the changes to look for weakened checks: { $detail }

interaction-guards-weakened-list-error = Could not list the untracked files to look for weakened checks: { $detail }

interaction-guards-weakened-size-error = These changes are too large to look for weakened checks ({ $megabytes } MB)

interaction-guards-authorship-source-git-ai = git-ai notes

interaction-guards-authorship-source-trace = Agent Trace

interaction-guards-authorship-source-author = an agent's account as the author

interaction-guards-authorship-source-trailer = a Co-authored-by trailer

interaction-guards-authorship-agent = Agent-written, from { $source }

interaction-guards-authorship-agent-agents = Agent-written ({ $agents }), from { $source }

interaction-guards-authorship-agent-lines = Has agent-written lines, from { $source }

interaction-guards-authorship-agent-lines-agents = Has agent-written lines ({ $agents }), from { $source }

interaction-guards-authorship-mixed = Co-written by a person and an agent, from { $source }

interaction-guards-authorship-mixed-agents = Co-written by a person and an agent ({ $agents }), from { $source }

interaction-guards-authorship-mixed-lines = Has lines by agents and by people, from { $source }

interaction-guards-authorship-mixed-lines-agents = Has lines by agents and by people ({ $agents }), from { $source }

interaction-guards-authorship-human = Written by a person, from { $source }

interaction-guards-authorship-human-agents = Written by a person ({ $agents }), from { $source }

interaction-guards-authorship-unknown = Author not recorded, from { $source }

interaction-guards-authorship-unknown-agents = Author not recorded ({ $agents }), from { $source }

interaction-guards-authorship-no-lines = { $branch } adds no lines since it left the base

interaction-guards-authorship-headline = { $branch }: { $percent }% agent-written ({ $agent } of { $total } lines added in { $commits ->
        [one] one commit
       *[other] { $commits } commits
    }){ $has_agent ->
        [yes] , { $edited }% of that since edited by a person
       *[no] { "" }
    }{ $has_mixed ->
        [yes] , { $mixed }% by a person and an agent
       *[no] { "" }
    }{ $has_unknown ->
        [yes] , { $unknown }% not recorded
       *[no] { "" }
    }{ $partial ->
        [yes] { " (the newest commits only)" }
       *[no] { "" }
    }

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

interaction-guards-fence-how-committed = committed

interaction-guards-fence-how-uncommitted = not committed

interaction-guards-fence-how-untracked = untracked

interaction-guards-fence-how-ignored = ignored by git

interaction-guards-weakened-reason-assertions = Removes assertions

interaction-guards-weakened-reason-tests = Removes tests

interaction-guards-weakened-reason-file = Deletes a test file

interaction-guards-weakened-reason-skip = Skips, disables or focuses tests

interaction-guards-weakened-reason-trivial = Adds an assertion that can't fail

interaction-guards-weakened-reason-tolerance = Loosens a tolerance

interaction-guards-weakened-reason-snapshot = Rewrites a snapshot wholesale

interaction-guards-weakened-reason-ci-failure = Lets CI fail and still pass

interaction-guards-weakened-reason-ci-job = Removes a CI job or trigger

interaction-guards-weakened-reason-hooks = Skips git hooks

interaction-guards-weakened-badge = weakens checks

interaction-guards-weakened-line = line { $line }: { $source }

interaction-guards-weakened-old-line = old line { $line }: { $source }

interaction-guards-weakened-line-detail = line { $line } ({ $detail }): { $source }

interaction-guards-weakened-old-line-detail = old line { $line } ({ $detail }): { $source }

interaction-guards-weakened-trigger = the `{ $key }` trigger

interaction-guards-weakened-job = the job `{ $key }`

interaction-guards-weakened-deletes = deletes { $path }

interaction-guards-weakened-deletes-snapshot = deletes the snapshot { $path }

interaction-guards-weakened-replaces = replaces { $removed } of its { $old } lines

interaction-guards-weakened-more = and { $count } more

interaction-guards-weakened-heading = { $reason }:


interaction-guards-fence-record = the fence's record

interaction-guards-push-lfs-attributes-file = .gitattributes is not a regular file; resolve that before converting to LFS.

interaction-guards-push-lfs-no-pointer = git-lfs did not produce a pointer; install git-lfs before converting history.

interaction-guards-push-default-nothing = push.default=nothing: choose an explicit branch push.

interaction-guards-push-lfs-filter = .lfsconfig must not pass through a content filter when reviewing endpoint configuration.
