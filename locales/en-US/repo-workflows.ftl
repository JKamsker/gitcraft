# Repository workflow presentation. Machine identifiers and commands remain literal.

repo-workflows-activity-activity-panel-0a4cf721 = Nothing yet: what this tab runs (fetch, commit, checkout…) is listed here with its git commands and all they printed.

repo-workflows-activity-activity-panel-3756b124 = Forget the operations listed here

repo-workflows-activity-activity-panel-38da1505 = Activity

repo-workflows-activity-activity-panel-6e279c58 = Close the Activity panel

repo-workflows-activity-activity-panel-83b12c22 = Clear

repo-workflows-activity-ago-4d631fe4 = { $s-3600-1 } h

repo-workflows-activity-ago-d2916a22 = { $s } s

repo-workflows-activity-ago-e823f861 = { $s-60-1 } min

repo-workflows-activity-bounded-62a56981 = { $output-head-end-1 }
    … { $left-out } bytes left out …
    { $output-tail-start-2 }

repo-workflows-activity-entry-ui-332e9c9d = Copy the commands and all they printed

repo-workflows-activity-entry-ui-599c9e61 = No git command ran.

repo-workflows-activity-exit-text-64df366e = did not finish

repo-workflows-activity-exit-text-bd2f2ddf = exit { $c }

repo-workflows-activity-exit-text-c22995ad = exit 0

repo-workflows-activity-outcome-text-5d28a90f = failed

repo-workflows-activity-outcome-text-8b47045e = cancelled

repo-workflows-activity-outcome-text-a4c3ed04 = done

repo-workflows-activity-seconds-6f29d45b = { NUMBER($d-as-secs-f32-1, minimumFractionDigits: 1, maximumFractionDigits: 1) } s

repo-workflows-activity-seconds-af37e50f = { $d-as-secs-1 } s

repo-workflows-activity-seconds-fba4d59d = { $d-as-secs-60-1 } min { $d-as-secs-60-2 } s

repo-workflows-activity-short-command-line-c85cf117 = { $command-line-args-shown-args-1 } … { $args-len-shown-args-2 } more

repo-workflows-agent--14b0fc42 = This can't be undone from gitcraft.

repo-workflows-agent-agent-decide-7695bed1 = Don't try it again unless the user asks you to.

repo-workflows-agent-agent-needs-sign-in-dc823c53 = { $label } needs the user to sign in to the remote: gitcraft asked them for it. Try again once they have.

repo-workflows-agent-agent-op-done-16d1e9b2 = { $running-agent-1 }: { $label }: done

repo-workflows-agent-agent-op-done-79a8519c = { $label }: done. { $how }

repo-workflows-agent-agent-op-done-8336fbfe = { $label } failed: { $e }

repo-workflows-agent-agent-op-done-cf3e44cf = It can't be undone from gitcraft.

repo-workflows-agent-agent-op-done-d5e3a3af = It is the latest step of gitcraft's undo list: the user can undo it with Ctrl+Z.

repo-workflows-agent-agent-planned-14b0fc42 = This can't be undone from gitcraft.

repo-workflows-agent-agent-start-917ed8ba = gitcraft couldn't start it: try again

repo-workflows-agent-agent-step-97d87ec1 = Nobody answered gitcraft's approval card within { $agents-approval-wait-as-secs-1 } seconds, so nothing was done. Ask the user to watch for the card in gitcraft before trying again.

repo-workflows-agent-agent-step-c03ca088 = gitcraft was busy for { $queue-wait-as-secs-60-1 } minutes (another operation or an approval card), so nothing was done. Try again.

repo-workflows-agent-branch-7776bacf = HEAD is detached: check out a branch first

repo-workflows-agent-check-branch-name-89d905bf = `{ $name }` can't be a branch name

repo-workflows-agent-checked-paths-0e0e013b = name at least one path

repo-workflows-agent-checkout-31e4be52 = { $branch } is checked out already

repo-workflows-agent-checkout-734452f6 = Checkout { $branch }

repo-workflows-agent-checkout-7a3686d6 = checking out

repo-workflows-agent-checkout-994822a7 = there is no local branch called { $branch } (create it with `create_branch`)

repo-workflows-agent-commit-4475f515 = Amend

repo-workflows-agent-commit-82a9c46f = Commit

repo-workflows-agent-commit-abf5ac58 = there is no commit to amend yet

repo-workflows-agent-commit-d5a3b056 = nothing is staged: stage the changes to commit first (tool `stage`)

repo-workflows-agent-commit-of-8c23b76d = `{ $rev }` is not a commit in this repository

repo-workflows-agent-create-branch-8464d926 = a branch called { $name } already exists

repo-workflows-agent-create-branch-d2a49cac = Create branch { $name }

repo-workflows-agent-delete-branch-55a975e8 = there is no local branch called { $name }

repo-workflows-agent-delete-branch-7ef8245e = Ctrl+Z makes it again at that commit.

repo-workflows-agent-delete-branch-94d21a83 = { $name } is checked out: check out another branch first

repo-workflows-agent-delete-branch-d32d800a = Delete branch { $name }

repo-workflows-agent-hard-ask-19b9c2f0 = The uncommitted changes to 1 tracked file are thrown away for good.

repo-workflows-agent-hard-ask-8de2bf44 = Ctrl+Z moves { $target } back afterwards; the thrown-away changes stay lost.

repo-workflows-agent-hard-ask-996b546d = The uncommitted changes to { $n } tracked files are thrown away for good.

repo-workflows-agent-hard-ask-c05b9dac = There are no uncommitted changes to tracked files now.

repo-workflows-agent-hard-ask-e720ce31 = Hard reset { $target } to { $short-hash-1 }

repo-workflows-agent-mixed-ask-2c1fd48b = Reset { $target } to { $short-hash-1 } (mixed)

repo-workflows-agent-mixed-ask-2ddd09f5 = Ctrl+Z moves { $target } back afterwards, but doesn't stage those changes again.

repo-workflows-agent-not-in-progress-1049b227 = a { $state-label-to-lowercase-1 } is in progress: { $what } would end it, which can't be undone. Finish or abort it first (see `conflicts`)

repo-workflows-agent-paths-label-26d36d06 = { $many-len-1 } paths

repo-workflows-agent-plan-0026c505 = Revert

repo-workflows-agent-plan-4c49fdb6 = Cherry-pick

repo-workflows-agent-push-1d502fc3 = Force push

repo-workflows-agent-push-2d7f2003 = nothing to push: { $branch } has no commits that { $remote }/{ $there } lacks

repo-workflows-agent-push-56d04849 = Overwrite { $to } with { $branch }. { $gone } git refuses if { $to } has changed since it was last fetched.

repo-workflows-agent-push-731ce7ed = Push

repo-workflows-agent-push-747bd6a7 = Send { $branch } to { $remote } as a new branch { $there }, and track it.

repo-workflows-agent-push-7d6400fd = Publish { $branch } on { $remote }

repo-workflows-agent-push-a74bf451 = { $to } has no commits that { $branch } lacks, as of the last fetch.

repo-workflows-agent-push-cb055d06 = Push { $branch } to { $to }

repo-workflows-agent-push-ccafdcaa = Force push { $branch } to { $to }

repo-workflows-agent-push-e5edacbd = { $branch } has no commits to push yet

repo-workflows-agent-push-f86f9688 = this repository has no remote to push to

repo-workflows-agent-read-23b028a2 = Detached HEAD

repo-workflows-agent-read-e72061c5 = Ghost preview unavailable: { $e }

repo-workflows-agent-rename-branch-00b004f0 = there is no local branch called { $from }

repo-workflows-agent-rename-branch-4fe447cd = a branch called { $to } already exists

repo-workflows-agent-rename-branch-5de8c7a7 = Rename { $from } to { $to }

repo-workflows-agent-reset-5247bb50 = { $target } is at { $short-hash-1 } already

repo-workflows-agent-reset-7c10af6a = there are no commits to reset to yet

repo-workflows-agent-reset-c6ebf3c0 = a reset

repo-workflows-agent-rewrite-177f568d = Ctrl+Z puts { $branch } back as it is now.

repo-workflows-agent-rewrite-2630fe5c = Move commit down

repo-workflows-agent-rewrite-3d1ccb79 = Move commit up

repo-workflows-agent-rewrite-601e2a07 = Drop commit

repo-workflows-agent-rewrite-7d959a5b = a { $state-label-to-lowercase-1 } is in progress: finish or abort it first

repo-workflows-agent-rewrite-fc4198eb = Drop commit { $short-hash-1 }

repo-workflows-agent-rewrite-ff973443 = the message is changed with `commit` and `amend`

repo-workflows-agent-stage-17e7b517 = Mark conflicts resolved

repo-workflows-agent-stage-3b4ddea7 = Stage { $paths-label-paths-1 }

repo-workflows-agent-stash-38bdfeaa = Stash

repo-workflows-agent-stash-4fce16d0 = stashing

repo-workflows-agent-stash-a7f01d16 = there are no changes to stash

repo-workflows-agent-stash-ee431f75 = the stash message must be one line of at most 1000 bytes

repo-workflows-agent-unstage-9d879ab6 = { $conflicted-join-1 } conflicted: resolve the conflict first (unstaging would end it)

repo-workflows-agent-unstage-a79ec226 = nothing is staged in { $paths-join-1 }

repo-workflows-agent-unstage-eb003390 = Unstage { $paths-label-paths-1 }

repo-workflows-agents-after-worktree-add-8fb6d5f3 = setup

repo-workflows-agents-after-worktree-add-9fd23075 = The agent checkpoint hooks weren't installed: { $e }

repo-workflows-agents-after-worktree-add-a2ffe669 = The worktree setup recipe wasn't run: { $e }

repo-workflows-agents-agent-act-8fb6d5f3 = setup

repo-workflows-agents-agent-choice-11b39c93 = Agent

repo-workflows-agents-agent-choice-2b8f8124 = { $cli-label-1 } takes no first message: type it there

repo-workflows-agents-agent-choice-36a10f87 = Installs the agent's hooks in the worktree: after each turn they run gitcraft checkpoint, and Uncommitted changes lists the turns, each to diff on its own

repo-workflows-agents-agent-choice-57b9f7ac = Command of my own

repo-workflows-agents-agent-choice-92a24920 = Run by sh in the worktree, e.g. claude --model opus

repo-workflows-agents-agent-choice-c8daf5c0 = Give it the task as its first message

repo-workflows-agents-agent-choice-e383f131 = Save each turn to review on its own

repo-workflows-agents-agent-section-370f346f = AGENTS

repo-workflows-agents-agent-section-736dbccf = No agent tasks. + starts one in a worktree of its own.

repo-workflows-agents-agent-section-a6153a60 = Puts the tasks' worktrees side by side, to assemble the best of them

repo-workflows-agents-agent-section-c37ddff4 = Compare them (bake-off)…

repo-workflows-agents-agent-section-d3a1dde8 = Start agent task…

repo-workflows-agents-agent-turns-94377c68 = AGENT TURNS

repo-workflows-agents-agent-turns-9a299007 = Saved by the agent's hooks after each turn. Click a turn to see what it changed; its menu compares it with the files as they are now

repo-workflows-agents-cancel-only-19766ed6 = Cancel

repo-workflows-agents-forget-turns-1d1d391e = The turns weren't deleted: { $e }

repo-workflows-agents-hover-0b4191eb = Turn { $turn-number-1 } · { $when }
    
    { $prompt }

repo-workflows-agents-hover-eae937e9 = (no prompt recorded)

repo-workflows-agents-install-hooks-2f5d6dc4 = Installing the agent hooks…

repo-workflows-agents-install-hooks-a8f2778c = The agent hooks weren't installed: { $e }

repo-workflows-agents-menu-04eb42a7 = Open a terminal there

repo-workflows-agents-menu-11b6670c = This agent worktree is missing

repo-workflows-agents-menu-13b97894 = Open and review changes

repo-workflows-agents-menu-24b65605 = The worktree and its branch stay; remove them in WORKTREES

repo-workflows-agents-menu-7b4c6a5c = Take off the list

repo-workflows-agents-menu-b1e3c6b9 = This agent worktree is missing or still being prepared

repo-workflows-agents-menu-cbc89e47 = Start the agent again

repo-workflows-agents-menu-d4604b9b = In a new terminal of its worktree, without the setup recipe and without the task as its first message

repo-workflows-agents-note-panel-4254e046 = Write the note first

repo-workflows-agents-note-panel-63565c04 = Add note

repo-workflows-agents-note-panel-961b2a04 = Close (Esc)

repo-workflows-agents-note-panel-bb064d10 = What the agent should change here

repo-workflows-agents-note-panel-d065164e = Kept in this worktree until you send the notes to the agent

repo-workflows-agents-problem-5fbb1fb2 = Name the branch the agent works on

repo-workflows-agents-problem-6eabbce8 = Give the folder's full path on that machine (/home/…): git doesn't read ~

repo-workflows-agents-problem-80f33652 = A branch named { $branch } exists already: pick another name

repo-workflows-agents-problem-8bffbdf3 = A branch name can't start with -

repo-workflows-agents-problem-97c58cf1 = Type the command that starts the agent

repo-workflows-agents-problem-bbc12752 = Pick the folder for the worktree

repo-workflows-agents-problem-c3177bf6 = Say where the branch starts: a branch, tag or commit

repo-workflows-agents-save-recipe-97381b52 = Worktree setup recipe saved

repo-workflows-agents-send-to-agent-4e9f0022 = Couldn't send { $what } to the { $label } terminal: { $e }

repo-workflows-agents-send-to-agent-a6f80242 = No agent terminal in this worktree to send { $what } to: start the agent (Start agent task…), or pick its terminal next to Send to agent

repo-workflows-agents-session-header-16959302 = Review since last looked…

repo-workflows-agents-session-menu-816f49fb = Review everything since last looked…

repo-workflows-agents-session-menu-835a56f7 = The saved turns go; the files stay as they are

repo-workflows-agents-session-menu-c3cada37 = Delete this session's turns

repo-workflows-agents-start-agent-task-d4ec2011 = Start agent task { $new-branch-1 }

repo-workflows-agents-start-agent-task-f80416ff = Wait for the running operation to finish

repo-workflows-agents-start-launch-tracked-d71ae829 = Could not start { $launch-label-1 }: { $e }

repo-workflows-agents-sub-25ba7976 = worktree removed

repo-workflows-agents-sub-308560dc = counting changes…

repo-workflows-agents-sub-32dfcdbc = 1 file changed

repo-workflows-agents-sub-96b72d81 = folder missing: prune it in WORKTREES

repo-workflows-agents-sub-a7109ae6 = { $n } files changed

repo-workflows-agents-sub-b2c35ddf = no uncommitted changes

repo-workflows-agents-terminal-choice-c0d8980e = to { $shown }

repo-workflows-agents-terminal-choice-c22c5ccf = The terminal the notes are typed into

repo-workflows-agents-terminal-choice-f1f46570 = no agent terminal

repo-workflows-agents-tip-1c0ba476 = { "\u000A" }Shown in this tab: its changes are in Uncommitted changes

repo-workflows-agents-tip-5c1b8dd5 = { "\u000A" }Click to open it and review its changes

repo-workflows-agents-tip-7d8e5553 = { "\u000A" }Task: { $row-task-prompt-1 }

repo-workflows-agents-tip-95d5fbf7 = { "\u000A" }Agent: { $row-task-agent-1 }

repo-workflows-agents-tray-act-78e896bb = the notes

repo-workflows-agents-tray-panel-0451c67e = Take this note out

repo-workflows-agents-tray-panel-1b67e032 = NOTES FOR THE AGENT · { $tray-notes-len-1 }

repo-workflows-agents-tray-panel-366ec196 = No agent terminal here: start the agent (Start agent task…), or pick its terminal

repo-workflows-agents-tray-panel-77a860cb = Send to agent

repo-workflows-agents-tray-panel-83b12c22 = Clear

repo-workflows-agents-tray-panel-86de1f26 = Only here: they go to the agent's terminal, nowhere else

repo-workflows-agents-tray-panel-a2c648eb = Types them into the agent's terminal as one message; then they're cleared

repo-workflows-agents-tray-panel-ae4eaa14 = Clear { $tray-notes-len-1 } notes?

repo-workflows-agents-turn-chip-1d35c7ca = Everything since this turn

repo-workflows-agents-turn-chip-60c8b8ae = after turn { $turn-number-1 }

repo-workflows-agents-turn-chip-a246e9f1 = What this turn changed

repo-workflows-agents-turn-chip-a3b0355d = Review hunks / revert only this turn…

repo-workflows-agents-turn-chip-eaf6cf58 = This turn against the files as they are now

repo-workflows-agents-turn-chip-ed5eb9a3 = now

repo-workflows-agents-turn-diff-9021b8b9 = turn { $turn-number-1 }: { $label }

repo-workflows-agents-turn-diff-dc601e7b = before turn { $turn-number-1 }

repo-workflows-agents-ui-136eaccc = Copy from the main worktree, one path per line

repo-workflows-agents-ui-1509f561 = Save

repo-workflows-agents-ui-196e15ae = Runs it as typed, in a terminal of this worktree (it isn't saved)

repo-workflows-agents-ui-19766ed6 = Cancel

repo-workflows-agents-ui-1fe95550 = Run in this worktree

repo-workflows-agents-ui-27cec493 = What the agent should do (optional)

repo-workflows-agents-ui-3553b76b = A branch and a worktree of its own, then a terminal there running the agent.

repo-workflows-agents-ui-44e1cc2b = { $crate-manual-command-1 }+Enter to start

repo-workflows-agents-ui-4bc74b21 = Task

repo-workflows-agents-ui-52656e81 = Branch

repo-workflows-agents-ui-5c81d3ac = Link to the main worktree, one path per line

repo-workflows-agents-ui-6f521989 = Fix what is shown in red first

repo-workflows-agents-ui-7227fcf5 = The script it runs

repo-workflows-agents-ui-74296b58 = If the repository has a worktree setup recipe (Worktree setup… in the palette), it runs first, in the same terminal.

repo-workflows-agents-ui-74b240bb = Setup command (sh -e: it stops at the first command that fails)

repo-workflows-agents-ui-74ccd433 = Folder

repo-workflows-agents-ui-97c3e67d = Start agent task

repo-workflows-agents-ui-beafb9a1 = The recipe is empty: nothing to run

repo-workflows-agents-ui-cbcf7990 = Worktree setup recipe

repo-workflows-agents-ui-cd1b3ecb = Variables, NAME=value per line ($GITCRAFT_WORKTREE_INDEX: the worktree's number, 0 for the main one)

repo-workflows-agents-ui-e4bb9f1e = Start

repo-workflows-agents-ui-e8a96feb = Starting at

repo-workflows-agents-ui-f37424d8 = Open a linked worktree of this repository to try the recipe on it

repo-workflows-agents-ui-f8894f23 = Runs in every worktree gitcraft makes for this repository, for an agent or not: in a terminal, with sh, on the repository's machine.

repo-workflows-ai-ai-message-button-04254124 = Writing the message…

repo-workflows-ai-ai-message-button-05060623 = Write the message with { $settings-tool-short-1 } from the last commit and what is staged
    Right-click for another tool

repo-workflows-ai-ai-message-button-33272cf9 = Write the message with { $settings-tool-short-1 } from the staged changes
    Right-click for another tool

repo-workflows-ai-ai-message-button-9553c66f = Not while a rebase or patch continues: reword the commit once it is done

repo-workflows-ai-ai-message-button-a1323968 = Have an AI tool you have installed write the message

repo-workflows-ai-ai-message-status-48845bff = Dismiss

repo-workflows-ai-ai-review-window-04dc8d04 = Review of { $review-branch-1 }

repo-workflows-ai-elapsed-127483e6 = { $secs } s

repo-workflows-ai-elapsed-fba4d59d = { $secs-60-1 } min { $secs-60-2 } s

repo-workflows-ai-message-answered-00f5dcad = The tool's answer had no summary line.

repo-workflows-ai-review-ui-19766ed6 = Cancel

repo-workflows-ai-review-ui-3ccec26f = { $running-tool-short-1 } is reviewing… { $elapsed-running-started-2 }

repo-workflows-ai-review-ui-b43c02f3 = What { $review-branch-1 } adds to { $review-base-2 } since they diverged

repo-workflows-ai-review-ui-d4ea61c7 = Review again

repo-workflows-ai-review-ui-d8b8392e = Try again

repo-workflows-ai-review-ui-e21f935f = Copy

repo-workflows-ai-review-ui-f8ec77e7 = Stopped.

repo-workflows-ai-start-review-62f579d4 = No AI tool is picked: choose one in Settings → AI, or with the sparkles button of the commit box.

repo-workflows-ai-suggestion-card-0335c833 = Keep mine

repo-workflows-ai-suggestion-card-57a64561 = Use it

repo-workflows-ai-suggestion-card-5a6b5767 = Replace the summary and description with it

repo-workflows-ai-suggestion-card-e3bbc8d9 = The tool's message (yours is kept until you choose):

repo-workflows-ai-tool-menu-06f0e210 = Turn off

repo-workflows-ai-tool-menu-0c0aa296 = { $tool-label-1 } — looking…

repo-workflows-ai-tool-menu-1440bda3 = Look for the tools again, after installing one

repo-workflows-ai-tool-menu-5e2d9c98 = Prompts, a command of your own, how much is sent: Settings → AI

repo-workflows-ai-tool-menu-671a1cd9 = { $tool-label-1 } — set it in Settings → AI

repo-workflows-ai-tool-menu-72f76d6c = AI tool, for messages and reviews (on { $place })

repo-workflows-ai-tool-menu-77cbb6fb = Set a custom command in Settings → AI first

repo-workflows-ai-tool-menu-9950df89 = This tool was not found on the repository's machine

repo-workflows-ai-tool-menu-9b1177a4 = { $tool-label-1 } — not found

repo-workflows-ai-tool-menu-d36feef0 = Look again

repo-workflows-ai-tool-menu-e223809d = this machine

repo-workflows-ai-writing-row-19766ed6 = Cancel

repo-workflows-ai-writing-row-ea07a4e5 = { $running-tool-short-1 } is writing the message… { $elapsed-running-started-2 }

repo-workflows-atlas-atlas-window-1316052d = Hotspot page { $view-page-1-1 } / { $pages }

repo-workflows-atlas-atlas-window-1556feb2 = Author date

repo-workflows-atlas-atlas-window-1af85190 = Size

repo-workflows-atlas-atlas-window-1bfb5492 = Recent churn

repo-workflows-atlas-atlas-window-2657941d = { $label } ({ $if-n-directory-folder-else-file-1 }), { $n-bytes-2 } bytes, { $n-changes-3 } changes, most frequent contributor: { $n-contributor-4 }. { $if-n-directory-zoom-into-folder--5 }

repo-workflows-atlas-atlas-window-34dd32fc = Apply / refresh atlas

repo-workflows-atlas-atlas-window-39c66d83 = Open file history at the pinned revision

repo-workflows-atlas-atlas-window-3a9dfa58 = Colour

repo-workflows-atlas-atlas-window-41bb5a28 = Zoom into folder

repo-workflows-atlas-atlas-window-4562ddd7 = Committer date

repo-workflows-atlas-atlas-window-48c740b7 = latest scanned

repo-workflows-atlas-atlas-window-4b6c521c = { $label }
    { $n-bytes-1 } bytes · { $n-changes-2 } changes
    Most frequent contributor: { $if-n-contributor-is-empty-no-cha-3 }

repo-workflows-atlas-atlas-window-4d800030 = Bytes

repo-workflows-atlas-atlas-window-5769ad13 = No files in this snapshot.

repo-workflows-atlas-atlas-window-611ebb33 = Churn and contributor atlas

repo-workflows-atlas-atlas-window-63bfe85d = From YYYY-MM-DD

repo-workflows-atlas-atlas-window-67be181b = Scanned file history across analyzed refs

repo-workflows-atlas-atlas-window-6aba5092 = Previous hotspots

repo-workflows-atlas-atlas-window-6e795d4d = Period

repo-workflows-atlas-atlas-window-6fd659e0 = Through YYYY-MM-DD

repo-workflows-atlas-atlas-window-86061296 = Next hotspots

repo-workflows-atlas-atlas-window-862161d0 = No changes in selected period

repo-workflows-atlas-atlas-window-8d9ca065 = Open scanned history for this exact byte path

repo-workflows-atlas-atlas-window-a5661a75 = Most frequent contributor: { $node-contributor-1 }

repo-workflows-atlas-atlas-window-a9656539 = Scan more atlas history

repo-workflows-atlas-atlas-window-aef428d1 = { $node-bytes-1 } bytes · { $node-changes-2 } file changes

repo-workflows-atlas-atlas-window-b4ab9b3d = ↑ Repository root

repo-workflows-atlas-atlas-window-ba512e1a = Most frequent contributor

repo-workflows-atlas-atlas-window-bc087fce = Change frequency

repo-workflows-atlas-atlas-window-c2e87ba8 = earliest scanned

repo-workflows-atlas-atlas-window-d16ccfdf = Cancel atlas analysis

repo-workflows-atlas-atlas-window-ef7caa7d = Analyzing pinned history and file sizes…

repo-workflows-atlas-atlas-window-f69e93c6 = Files and historical folders at the same path are separate hotspots. File history uses HEAD for current files or an observed change revision for historical files. Right-click a file for history across analyzed refs; non-UTF-8 paths open that history directly. Long contributor labels are shortened; numbered identities remain separate.

repo-workflows-atlas-new-70ba4563 = File history cancelled

repo-workflows-atlas-read-atlas-02c42736 = Use YYYY-MM-DD dates

repo-workflows-atlas-read-atlas-67b18cbf = Use YYYY-MM-DD dates, with start on or before end

repo-workflows-atlas-show-01efcd54 = Preparing file history…

repo-workflows-atlas-show-076017ad = This list matches this exact path across the analyzed refs, including deletions. It uses the full scanned snapshot, independent of the atlas time window; it does not follow renames to other paths. Open a commit to inspect its changes. For older changes, use Scan more atlas history in the atlas.

repo-workflows-atlas-show-1abc4096 = Next file commits

repo-workflows-atlas-show-56fe4d78 = Previous file commits

repo-workflows-atlas-show-7269bb4e = Scanned file history across refs

repo-workflows-atlas-show-8647a64f = Exact path bytes: { $clipped-self-label-256-1 }

repo-workflows-atlas-show-c256cd01 = No scanned commits touch this exact path. Its history may precede the scan boundary.

repo-workflows-bakeoff-assemble-bar-3766ae43 = Open a file below and take it whole, or some of its hunks, from any attempt

repo-workflows-bakeoff-assemble-bar-5a22f453 = Assembling…

repo-workflows-bakeoff-assemble-bar-8462ef57 = Assemble into

repo-workflows-bakeoff-assemble-bar-86fd9bbb = Assemble ({ $picks } files)

repo-workflows-bakeoff-assemble-bar-916e34d4 = Show in graph

repo-workflows-bakeoff-assemble-bar-aee10357 = A new branch from the base with what is picked; the rest stays as the base has it. Nothing checked out changes

repo-workflows-bakeoff-assemble-bar-ccbdbfe8 = Assemble (1 file)

repo-workflows-bakeoff-assemble-bar-f19adc05 = Name the new branch first

repo-workflows-bakeoff-assemble-c7f55e42 = Made { $branch } at { $crate-repo-short-commit-1 }

repo-workflows-bakeoff-bakeoff-view-9754333a = Comparing the attempts…

repo-workflows-bakeoff-body-bfd30d1e = click one to see it across the attempts and pick from it

repo-workflows-bakeoff-build-06d160c3 = could not prepare the build's status file: { $e }

repo-workflows-bakeoff-build-c18d3706 = Type the command that builds the project first

repo-workflows-bakeoff-build-c9238812 = build · { $name }

repo-workflows-bakeoff-build-cell-17824e4b = The old build may still run. Starts a separate retained snapshot; does not stop or clean up the old launch

repo-workflows-bakeoff-build-cell-26c16489 = not built

repo-workflows-bakeoff-build-cell-301c453c = ✘ fails ({ $code })

repo-workflows-bakeoff-build-cell-3149d8b3 = Runs your command on the exact compared snapshot in a fresh detached worktree

repo-workflows-bakeoff-build-cell-5dac180d = outcome unavailable

repo-workflows-bakeoff-build-cell-63f96d79 = Launch failed, terminal closed/output ended, or receipt is invalid. This does not prove all child processes finished.

repo-workflows-bakeoff-build-cell-6fe4abd4 = Retry in fresh workspace

repo-workflows-bakeoff-build-cell-84b604d3 = The previous outcome is unknown and its processes may still run. Starts another retained workspace; no automatic stop or cleanup

repo-workflows-bakeoff-build-cell-98cd8a0b = preparing snapshot…

repo-workflows-bakeoff-build-cell-adccacb7 = Runs the command in another fresh retained snapshot

repo-workflows-bakeoff-build-cell-af81e9e4 = Build again

repo-workflows-bakeoff-build-cell-bdd254b6 = Build

repo-workflows-bakeoff-build-cell-d07e6c18 = ✔ builds

repo-workflows-bakeoff-build-cell-ef24cfb5 = Its output is in the terminal tab

repo-workflows-bakeoff-column-2ed1ade8 = No hunks to pick (a binary file): take it whole

repo-workflows-bakeoff-column-6e86718f = Replaces the hunks picked; click again to take it back

repo-workflows-bakeoff-column-772841f1 = Take the deletion

repo-workflows-bakeoff-column-a533487d = Take the whole file

repo-workflows-bakeoff-column-badecf50 = Take the new file

repo-workflows-bakeoff-files-51aec897 = { $a-name-1 } left it as it was

repo-workflows-bakeoff-files-7e17d722 = None of the attempts changed anything

repo-workflows-bakeoff-files-83d88f06 = { $a-name-1 } changed it

repo-workflows-bakeoff-files-fe616a60 = { $path }
    Open it side by side across the attempts

repo-workflows-bakeoff-grid-1af85190 = Size

repo-workflows-bakeoff-grid-1f19288a = yes, compared too

repo-workflows-bakeoff-grid-6263bbf3 = Open this run's complete saved output in the configured editor

repo-workflows-bakeoff-grid-6a3fe887 = Builds

repo-workflows-bakeoff-grid-7a44bde0 = Uncommitted work

repo-workflows-bakeoff-grid-7cbda0dd = Build output…

repo-workflows-bakeoff-grid-9c37d9c9 = New dependencies

repo-workflows-bakeoff-grid-ce199c9e = Files touched

repo-workflows-bakeoff-header-0e2a45b6 = Other attempts…

repo-workflows-bakeoff-header-175f9ff6 = Build all

repo-workflows-bakeoff-header-43c40423 = Bake-off

repo-workflows-bakeoff-header-46334791 = Build with

repo-workflows-bakeoff-header-502f6dbd = Compare again

repo-workflows-bakeoff-header-50cd264e = from { $crate-repo-short-cmp-base-1 }

repo-workflows-bakeoff-header-7d9eb7ac = Close

repo-workflows-bakeoff-header-862a0723 = Explicitly runs this command on each compared snapshot in a fresh detached worktree. No setup recipe runs. Workspaces remain in Worktrees for inspection and removal.

repo-workflows-bakeoff-header-9cec669f = Compare the attempts first

repo-workflows-bakeoff-header-a195ce68 = Type the command that builds the project

repo-workflows-bakeoff-header-acb0a622 = ⚠ { $elsewhere-join-1 } started elsewhere

repo-workflows-bakeoff-header-b1ce0225 = Reads the attempts again, uncommitted work included

repo-workflows-bakeoff-header-bb683fa4 = The commit they all start from (their merge base)

repo-workflows-bakeoff-header-c38e956d = Kept for the repository

repo-workflows-bakeoff-header-dd4159a5 = It didn't start from the same commit as the others: its column also shows what came before it started

repo-workflows-bakeoff-hunk-label-0cf292e8 = At the top

repo-workflows-bakeoff-hunk-label-4d9d8a81 = Line { $hunk-old-start-1 }

repo-workflows-bakeoff-hunk-label-7647677f = After line { $hunk-old-start-1 }

repo-workflows-bakeoff-hunk-label-a3052fdf = Lines { $hunk-old-start-1 }–{ $hunk-old-start-n-1-2 }

repo-workflows-bakeoff-load-bakeoff-c6f043db = Picks cleared: the attempts changed since they were made

repo-workflows-bakeoff-pick-label-77dbc108 = ← whole file from { $name-n-1 }

repo-workflows-bakeoff-pick-label-c708de81 = ← { $list-len-1 } hunks

repo-workflows-bakeoff-pick-label-d3d98b5d = ← 1 hunk

repo-workflows-bakeoff-side-by-side-47c58b04 = Unchanged in { $unchanged-join-1 }

repo-workflows-bakeoff-side-by-side-6bdfa343 = Clear pick

repo-workflows-bakeoff-side-by-side-76215eb4 = The assembled branch keeps the base's version of this file

repo-workflows-bakeoff-side-by-side-f280944f = Hunks of two attempts picked here change the same lines: untick one of them

repo-workflows-bakeoff-ui-09eb5aca = Pick { $MIN } to { $MAX } attempts at the same task to compare: worktrees (with what is uncommitted in them) or branches.

repo-workflows-bakeoff-ui-19766ed6 = Cancel

repo-workflows-bakeoff-ui-43c40423 = Bake-off

repo-workflows-bakeoff-ui-6217d161 = worktree · { $crate-remote-display-w-1 }

repo-workflows-bakeoff-ui-638e249f = Filter

repo-workflows-bakeoff-ui-ae96d29a = Pick { $MIN } to { $MAX } attempts

repo-workflows-bakeoff-ui-c51e8eaa = No worktrees or other branches to compare

repo-workflows-bakeoff-ui-cd8d844d = No match

repo-workflows-bakeoff-ui-e888155d = Compare

repo-workflows-bakeoff-ui-fbbc3b24 = At most { $MAX } attempts

repo-workflows-bisect--512af075 = Wait for the run to end, or stop it

repo-workflows-bisect--e5e3ecc5 = Wait for the operation going on to finish

repo-workflows-bisect-bisect-blocker-cff28fea = Wait for { $busy } to finish first.

repo-workflows-bisect-bisect-buttons-0599842f = git bisect reset: checks out { $branch } again

repo-workflows-bisect-bisect-buttons-12c0910a = { $current } can't be tested: try another one

repo-workflows-bisect-bisect-buttons-7cf0c74e = git bisect run: a command tests each commit for you

repo-workflows-bisect-bisect-buttons-7f573858 = { $current } has the change

repo-workflows-bisect-bisect-buttons-a28c79f7 = git bisect reset: checks out again what was checked out when it began

repo-workflows-bisect-bisect-buttons-a6c7a84b = Finish

repo-workflows-bisect-bisect-buttons-a9a77839 = { $current } is from before the change

repo-workflows-bisect-bisect-buttons-c3efc1be = Mark a good and a bad commit first

repo-workflows-bisect-bisect-buttons-cae7d57b = Stop

repo-workflows-bisect-bisect-buttons-eded3d42 = Run a command…

repo-workflows-bisect-bisect-menu-items-43fa49b5 = Start bisect here…

repo-workflows-bisect-bisect-menu-items-5cc56687 = Find the commit that brought a change in, by testing commits between a good and a bad one

repo-workflows-bisect-bisect-menu-items-5f1847b9 = Mark { $at } as { $bisect-word-mark-bad-1 }

repo-workflows-bisect-bisect-menu-items-72281c73 = Skip { $at } in the bisect

repo-workflows-bisect-bisect-palette-items-011b904c = Bisect: skip { $at }

repo-workflows-bisect-bisect-palette-items-23b8f037 = Bisect: stop

repo-workflows-bisect-bisect-palette-items-34a86ddf = Bisect: finish

repo-workflows-bisect-bisect-palette-items-417fbb2c = Bisect: mark { $at } as { $bisect-word-mark-bad-1 }

repo-workflows-bisect-bisect-palette-items-96774cb9 = Bisect: run a command…

repo-workflows-bisect-bisect-palette-items-ba89bd47 = Start bisect…

repo-workflows-bisect-bisect-status-675fd45e = Bisecting

repo-workflows-bisect-bisect-step-done-8336fbfe = { $label } failed: { $e }

repo-workflows-bisect-commit-link-63633913 = Show this commit

repo-workflows-bisect-hash-link-63633913 = Show this commit

repo-workflows-bisect-mark-bisect-15306802 = Skip this commit

repo-workflows-bisect-mark-bisect-22f6b91b = Mark { $short-rev-1 } as { $bisect-word-mark-2 }

repo-workflows-bisect-mark-bisect-9b0f3138 = Mark as { $bisect-word-mark-1 }

repo-workflows-bisect-mark-bisect-a1c6c771 = No bisect is going on

repo-workflows-bisect-reset-bisect-0ddac35c = Stop bisect

repo-workflows-bisect-reset-bisect-1bdcc155 = Finish bisect

repo-workflows-bisect-reset-bisect-b63b6486 = back on the commit it began at

repo-workflows-bisect-reset-bisect-fd58b76b = back on { $branch }

repo-workflows-bisect-run-log-534a3a34 = ended with exit code { $c }

repo-workflows-bisect-run-log-5fd804ed = failed: { $e }

repo-workflows-bisect-run-log-64876c47 = Hide output

repo-workflows-bisect-run-log-7d9eb7ac = Close

repo-workflows-bisect-run-log-8d23f4ee = Stops the command and what it started; the bisect stays where it got to

repo-workflows-bisect-run-log-9dbbb249 = Show output

repo-workflows-bisect-run-log-b7ec68a2 = Stop run

repo-workflows-bisect-run-log-bb5e4bf5 = ended by a signal

repo-workflows-bisect-run-message-3a24171a = Bisect run failed: { $e }

repo-workflows-bisect-run-message-96334386 = Bisect run stopped

repo-workflows-bisect-run-message-b1d63bd2 = exit code { $c }

repo-workflows-bisect-run-message-b4eba258 = Bisect run ended with { $code }: { $last-line }

repo-workflows-bisect-run-message-dc9c7971 = Bisect run: only skipped commits are left, the first bad commit is one of them

repo-workflows-bisect-run-message-de2346a5 = Bisect run: found the first bad commit

repo-workflows-bisect-run-message-e8e581bc = a signal

repo-workflows-bisect-start-bisect-5c82e3e0 = Can't start a bisect: { $why }

repo-workflows-bisect-start-bisect-db936451 = Start bisect

repo-workflows-bisect-start-bisect-run-d300d73e = Bisect run

repo-workflows-bisect-steps-text-14999716 = About { $n } steps left

repo-workflows-bisect-steps-text-263b5d8b = About 1 step left

repo-workflows-bisect-steps-text-2d1513e9 = The last step

repo-workflows-bisect-steps-text-5ccbfd65 = { $n } commits after this one

repo-workflows-bisect-steps-text-9bbc8458 = 1 commit after this one

repo-workflows-bisect-steps-text-a4e9b171 = no commits after this one

repo-workflows-bisect-stop-bisect-action-0ddac35c = Stop bisect

repo-workflows-bisect-stop-bisect-action-c380662a = Stop bisecting and check out { $back } again? The commits marked so far are forgotten.

repo-workflows-bisect-stop-bisect-action-fff2c8f7 = what was checked out when it began

repo-workflows-bisect-waiting-2d4ce59a = Mark a { $word } commit ({ $meaning }): right-click it in the graph, or use { $capitalised-word-1 } for the one checked out.

repo-workflows-bisect-where-it-is-25230d8d = Only skipped commits are left: the first bad commit is one of these { $possible-len-1 }:

repo-workflows-bisect-where-it-is-4c0b8e59 = git finds nothing to test between the marks (a good commit after the bad one?): mark others, or Stop.

repo-workflows-bisect-where-it-is-683768ab = and { $possible-len-max-named-1 } more

repo-workflows-bisect-where-it-is-7d7c30e4 = · Testing

repo-workflows-bisect-where-it-is-8b3812c0 = First bad commit:

repo-workflows-bisect-where-it-is-94883ca6 = one from before the change

repo-workflows-bisect-where-it-is-d5ebaa0a = one with the change

repo-workflows-ci_view-by-start-ba35f0c4 = Not started

repo-workflows-ci_view-ci-act-07fe25a7 = Cancelled { $what }

repo-workflows-ci_view-ci-act-da60943b = Re-ran the failed jobs of { $what }

repo-workflows-ci_view-ci-view-ui-90db5328 = No CI ran on this commit.

repo-workflows-ci_view-ci-view-ui-b431c0d7 = { $noun } of { $short-view-sha-1 }

repo-workflows-ci_view-ci-view-ui-c454bec0 = Workflow runs

repo-workflows-ci_view-ci-view-ui-d1a8fffe = Pipelines

repo-workflows-ci_view-ci-view-ui-e50c77eb = Some projects could not be checked for builds.

repo-workflows-ci_view-job-card-0098c77e = job { $job-name-1 }

repo-workflows-ci_view-job-card-f4ed9e36 = { $job-name-1 }
    Double-click to open it in the browser

repo-workflows-ci_view-load-ci-view-4c1d2d4e = This repository isn't on GitHub or GitLab.

repo-workflows-ci_view-open-ci-680d5c5f = interactive rebase

repo-workflows-ci_view-open-ci-991ae978 = Finish or close the { $editor } first

repo-workflows-ci_view-open-ci-d509b2ee = conflict editor

repo-workflows-ci_view-page-04ec3970 = LOG OF THE FAILED JOBS

repo-workflows-ci_view-page-20681c9f = the log

repo-workflows-ci_view-page-30fc4b32 = STEPS OF { $job-name-to-uppercase-1 }

repo-workflows-ci_view-page-66f52811 = No jobs.

repo-workflows-ci_view-page-7261125c = The log is empty.

repo-workflows-ci_view-page-77a860cb = Send to agent

repo-workflows-ci_view-page-da532fb6 = Types the end of this log into the agent's terminal, to fix the failure

repo-workflows-ci_view-page-header-8c6c9cf3 = Back (Esc)

repo-workflows-ci_view-page-header-a7eac6b5 = Open in the browser

repo-workflows-ci_view-page-header-d62bc4a3 = Load again

repo-workflows-ci_view-run-actions-19766ed6 = Cancel

repo-workflows-ci_view-run-actions-1c703119 = Show the failed log

repo-workflows-ci_view-run-actions-46677f1c = Runs the jobs that failed or were cancelled again

repo-workflows-ci_view-run-actions-7b68b715 = Hide the log

repo-workflows-ci_view-run-actions-8c40a11a = Re-run failed jobs

repo-workflows-ci_view-run-header-0b4ad7c1 = Show the commit in the graph

repo-workflows-ci_view-run-header-396f73af = attempt { $run-attempt-1 }

repo-workflows-ci_view-run-header-420fc08d = running for { $ci-duration-label-seconds-1 }

repo-workflows-ci_view-run-header-4e8bc57f = · started { $git-relative-time-started-now-1 }

repo-workflows-ci_view-run-header-ac6a17db = took { $ci-duration-label-seconds-1 }

repo-workflows-ci_view-run-header-b368daf0 = skipped or cancelled

repo-workflows-ci_view-timeline-709e9121 = GitLab doesn't list the steps of a job: its log is on its page.

repo-workflows-ci_view-timeline-7a7400f4 = not started

repo-workflows-ci_view-timeline-99fa228f = Not started yet.

repo-workflows-ci_view-timeline-b59cc5d9 = This job ran no steps.

repo-workflows-ci_view-timeline-ee459f80 = This job has no steps to show yet.

repo-workflows-collision_view-body-282052b1 = Each pair of branches merged in memory (git merge-tree), and each with { $onto }: nothing is checked out, no branch moves. Branches of the linked worktrees are ticked.

repo-workflows-collision_view-body-5c35d043 = the branch they land on

repo-workflows-collision_view-body-7d9eb7ac = Close

repo-workflows-collision_view-body-94846973 = Merging in memory… { $done } of { $run-total-1 }

repo-workflows-collision_view-body-cae7d57b = Stop

repo-workflows-collision_view-body-cf642735 = Collision matrix

repo-workflows-collision_view-cannot-check-42af2e23 = Pick the branch they land on

repo-workflows-collision_view-cannot-check-76fb9e14 = A check is running

repo-workflows-collision_view-cannot-check-8bcfe01d = Tick the branches to check

repo-workflows-collision_view-cannot-check-c1d9426d = At most { $MOST } branches at a time

repo-workflows-collision_view-cell-tip-81bf5184 = { "\u000A  " }and { $n-listed-1 } more

repo-workflows-collision_view-cell-tip-9b99b61c = { "\u000A" }Click to compare them

repo-workflows-collision_view-cell-tip-b3f28cca = { $a } and { $b } merge cleanly

repo-workflows-collision_view-check-collisions-96d2bcac = Stopped; nothing new to show

repo-workflows-collision_view-matrix-f5346a60 = Merged into { $m-target-1 }

repo-workflows-collision_view-pick-branches-3bc09abe = Lands on

repo-workflows-collision_view-pick-branches-6de3bf71 = There is no other local branch.

repo-workflows-collision_view-pick-branches-fb40a427 = Pick one…

repo-workflows-collision_view-results-45512235 = Some pairs couldn't be merged in memory: { $e }

repo-workflows-collision_view-results-87f3fc81 = → is each branch merged into { $m-target-1 }. A number is how many files conflict: hover for them, click to compare the two.

repo-workflows-collision_view-results-9b24a27f = Suggested landing order

repo-workflows-collision_view-step-note-5896a7b7 = couldn't be merged with { $unknown-join-1 } in memory

repo-workflows-collision_view-step-note-5ef5ed3f = conflicts with { $parts-join-1 }

repo-workflows-collision_view-step-note-cb1d05dc = merges cleanly with each one before it

repo-workflows-content_recovery-content-inspect-e3ceaa94 = Invalid content selection

repo-workflows-content_recovery-content-perform-9d57ebb2 = Content recovery: { $e }

repo-workflows-content_recovery-content-perform-fbdf23f8 = Recover content

repo-workflows-content_recovery-origin-53bdf21a = Reflog / history

repo-workflows-content_recovery-origin-856aca6d = Current stash list

repo-workflows-content_recovery-origin-d8ee810d = Unreachable commit

repo-workflows-content_recovery-origin-ebcaaeb6 = Dropped stash

repo-workflows-content_recovery-show-11f20956 = Text you remember, such as parse_header

repo-workflows-content_recovery-show-1762ea22 = This is a shallow repository; older content may not be present locally.

repo-workflows-content_recovery-show-1e53ab9e = Limits: 500 commits, 10000 objects, 15000 file entries, 2 MiB per blob, 64 MiB scanned content, 300 hits, 120 seconds. Binary/non-UTF-8 and oversized content is skipped.

repo-workflows-content_recovery-show-1f3ee4b2 = Preview branch recovery…

repo-workflows-content_recovery-show-30ea379c = Cancel read

repo-workflows-content_recovery-show-334a8ddd = Displayed text is clipped to 2000 lines and 400 characters per line. Recovery uses the complete saved bytes.

repo-workflows-content_recovery-show-34ac4d52 = { $hit-provenance-len-1 } known places / versions

repo-workflows-content_recovery-show-4c76d94b = Search literal text

repo-workflows-content_recovery-show-50b4d1c2 = New branch:

repo-workflows-content_recovery-show-52545e10 = { $report-hits-len-1 } matches · { $report-scanned-2 } blobs / { $report-bytes-3 } bytes read · { $report-binary-4 } binary, { $report-large-5 } oversized, { $report-missing-6 } missing skipped

repo-workflows-content_recovery-show-55bda87c = Lost and found by content

repo-workflows-content_recovery-show-5ec718b0 = Reading local objects · { $self-began-map-or-0-at-at-elapse-1 }s

repo-workflows-content_recovery-show-67802d1b = Find literal text in reflog history, unreachable commits, dropped stashes and unattached staged blobs. Search reads local objects without downloading or changing files.

repo-workflows-content_recovery-show-6f803fc9 = Time unknown

repo-workflows-content_recovery-show-7d9eb7ac = Close

repo-workflows-content_recovery-show-8cd42b70 = Creates a new regular file on the repository’s machine. Existing, tracked, ignored and linked destinations are refused; staged contents stay unchanged.

repo-workflows-content_recovery-show-8d5a103e = Create branch { $name } at the saved commit. The complete saved commit is included; HEAD and working files stay unchanged.

repo-workflows-content_recovery-show-8ef50fb8 = Search reached a budget or skipped unsupported paths/provenance. These results are incomplete.

repo-workflows-content_recovery-show-93547d7d = Recover now

repo-workflows-content_recovery-show-9cba18de = Create { $prepared-preview-identity-root-p-1 }/{ $path } with { $prepared-preview-bytes-len-2 } raw bytes. Nothing existing will be replaced.

repo-workflows-content_recovery-show-a6dcec6a = Unattached content: original filename and staging time are unknown. It can be recovered as a new file.

repo-workflows-content_recovery-show-a963e860 = { $date } · Unattached staged content

repo-workflows-content_recovery-show-abadbc35 = Timeline is newest first where a commit date is known. Unattached blobs appear last.

repo-workflows-content_recovery-show-beb0df0b = Back to matches

repo-workflows-content_recovery-show-c006906c = Saved content · { $preview-bytes-len-1 } bytes · { $preview-blob-12-2 }

repo-workflows-content_recovery-show-c8d6eb2f = Preview return to stash list…

repo-workflows-content_recovery-show-cc6d8221 = New file in this repository:

repo-workflows-content_recovery-show-da1ba0df = Preview file recovery…

repo-workflows-content_recovery-show-eff2c65f = Return this dropped stash to the stash list, preserving the current stash. Do not apply it or change staged/working files.

repo-workflows-content_recovery-show-fbdaf564 = (unavailable root)

repo-workflows-custom_commands-custom-act-5bd548f5 = { $self-name-1 }: { $commands } repository command(s) now offered

repo-workflows-custom_commands-custom-file-read-4533a718 = { $self-name-1 } has its own custom commands: review them (palette or a menu) to use them

repo-workflows-custom_commands-custom-file-read-cbe7f164 = { $self-name-1 }: { $cc-repo-file-2 } can't be used. Review repository commands… says why

repo-workflows-custom_commands-custom-palette-items-0841b1ff = repository command

repo-workflows-custom_commands-custom-palette-items-b8af1810 = Review repository commands…

repo-workflows-custom_commands-custom-palette-items-cd972ec3 = custom command

repo-workflows-custom_commands-menu-items-7bbc1546 = { $cc-repo-file-1 } has commands not trusted yet (or can't be read): see them first

repo-workflows-custom_commands-menu-items-8f4e08cd = Custom command (Settings)

repo-workflows-custom_commands-menu-items-b8af1810 = Review repository commands…

repo-workflows-custom_commands-menu-items-cb87a7d9 = Custom command of this repository

repo-workflows-custom_commands-read-file-55a45ca4 = { $cc-repo-file-1 } isn't UTF-8 text

repo-workflows-custom_commands-read-file-851cebee = { $cc-repo-file-1 } can't be read: { $e }

repo-workflows-custom_commands-read-here-28b7ef24 = it is larger than { $max-1024-1 } KiB

repo-workflows-custom_commands-read-here-50cdcf6c = it isn't a file

repo-workflows-custom_commands-read-here-f4fd09c1 = it is a symbolic link

repo-workflows-custom_commands-run-custom-44771367 = ${ $name } can't hold a NUL character

repo-workflows-custom_commands-run-custom-captured-471b45de = { $name } couldn't be run: { $e }

repo-workflows-custom_commands-run-custom-captured-5215b5ab = Running { $name }…

repo-workflows-custom_commands-run-custom-in-terminal-f33b19b3 = { $name } couldn't be started: { $e }

repo-workflows-dco--5e1a6b4e = Adds Signed-off-by with your name and email (DCO)

repo-workflows-dco-commit-note-72e49d72 = A DCO check refuses a commit without a sign-off of its author or committer

repo-workflows-dco-commit-note-c87e4ef3 = No Signed-off-by from its author (DCO)

repo-workflows-dco-commit-note-fccbd96e = ✔ Signed off by its author (DCO)

repo-workflows-dco-commits-count-4eb116dc = { $n } commits

repo-workflows-dco-commits-count-acfd7b9e = 1 commit

repo-workflows-dco-dco-act-10fe4a7c = Couldn't save Sign off for this repository (git config { $signoff-sign-off-key-1 }): { $e }

repo-workflows-dco-dco-op-done-55d2b32b = Push the signed-off commits of { $plan-branch-1 }

repo-workflows-dco-dco-op-done-5d0c9ff1 = Sign off the commits of { $plan-branch-1 } (here only)

repo-workflows-dco-fix-button-04f5829a = Adds your Signed-off-by to each of the branch's commits (rebase --signoff) and force-pushes them with a lease, after showing what it will do. Ctrl+Z undoes it.

repo-workflows-dco-fix-button-344ac70e = Sign off the branch's commits…

repo-workflows-dco-fix-button-6d181602 = Check out { $details-head-branch-1 } first (or open it in a worktree): the commits signed off are those of the branch checked out, at the pull request's latest commit

repo-workflows-dco-fix-dialog-1a8aeebd = Sign off and push

repo-workflows-dco-fix-dialog-6ce6ab1f = Sign off

repo-workflows-dco-fix-label-dfc76b8d = Sign off the commits of { $plan-branch-1 }

repo-workflows-dco-fix-preview-1cc2b37b = refused if someone pushed there since it was last fetched

repo-workflows-dco-fix-preview-b12258fd = refused if that branch exists there already

repo-workflows-dco-plan-0ac1ebd9 = The branch and its base have no commit in common: { $e }

repo-workflows-dco-plan-4c71d659 = { $branch } has no commits of its own since its base

repo-workflows-dco-plan-589b41e2 = { $branch } is at { $crate-repo-short-head-1 }, the pull request at { $crate-repo-short-expected-2 }: pull or push it first, so that the commits signed off are the pull request's

repo-workflows-dco-plan-927db158 = { $label } has commits { $branch } hasn't: pull them first, or the push would throw them away

repo-workflows-dco-plan-a16c4b9b = Every commit of { $branch } is already signed off

repo-workflows-dco-plan-cfb9aeb9 = Commit or stash your changes first: signing off rebases the branch

repo-workflows-dco-plan-d7cbaa06 = { $branch } has merge commits since its base, which a rebase would flatten. Sign it off in a terminal (git rebase --signoff --rebase-merges { $crate-repo-short-merge-base-1 }), or ask its authors to

repo-workflows-dco-plan-d866c6e0 = Check out the pull request's branch first: HEAD is detached

repo-workflows-dco-pr-section-15029601 = No commits for the DCO check to look at.

repo-workflows-dco-pr-section-9ecdacc9 = Waiting for the pull request's commits to load…

repo-workflows-dco-pr-section-e93d65c8 = Couldn't read the commits' sign-offs: { $e }

repo-workflows-dco-text-dc4915c9 = the DCO check of #{ $n } fails

repo-workflows-dco-text-e5458838 = this repository has a { $file } file

repo-workflows-dco-toggle-6ce6ab1f = Sign off

repo-workflows-dco-toggle-9ba5d60e = The commits being replayed keep their own messages

repo-workflows-detached--bff74cc9 = You're looking at an old version; new commits here belong to no branch.

repo-workflows-detached-detached-act-70dad4a3 = Wait for { $busy } to finish, then check out again

repo-workflows-detached-detached-banner-15bbfb59 = Create branch here

repo-workflows-detached-detached-banner-3d7f6bc2 = HEAD is detached at { $short-head-1 }: it is on no branch. Commits made now are on no branch either, and checking out something else leaves them behind.

repo-workflows-detached-detached-banner-f16cf1bf = A branch at this commit, checked out: new commits go on it

repo-workflows-detached-detached-banner-f5cd526a = Check { $branch } out

repo-workflows-detached-detached-banner-f76b5669 = Back to { $branch }

repo-workflows-detached-keep-steps-d2a49cac = Create branch { $name }

repo-workflows-detached-leave-to-007a4c0e = Didn't check out { $leave-label-1 }: { $busy } started meanwhile

repo-workflows-detached-leave-to-28ae5898 = Checkout { $leave-label-1 }: wait for { $busy } to finish

repo-workflows-detached-leave-to-48b0ce4c = Didn't check out { $leave-label-1 }: couldn't list the commits it would leave on no branch ({ $e })

repo-workflows-detached-leave-to-c6023785 = the listing stopped unexpectedly

repo-workflows-devcontainer-container-id-a1ced98b = Docker must return a complete 64-character container ID

repo-workflows-devcontainer-detect-01821b94 = Dev container configuration must be readable inside the worktree and at most 1 MiB

repo-workflows-devcontainer-detect-764255b7 = Dev container configuration is not UTF-8

repo-workflows-devcontainer-devcontainer-act-519a489a = Dev container operation

repo-workflows-devcontainer-devcontainer-act-5474eef8 = Working…

repo-workflows-devcontainer-devcontainer-act-84f8f92f = Cancelled before starting

repo-workflows-devcontainer-devcontainer-window-05be654b = Start environment

repo-workflows-devcontainer-devcontainer-window-0c79d376 = Worker ended or was cancelled. A container may remain; inspect this worktree's labels before retrying.

repo-workflows-devcontainer-devcontainer-window-20441b4a = Rebuild displayed environment

repo-workflows-devcontainer-devcontainer-window-22f9e7ca = No .devcontainer/devcontainer.json detected. Remote worktrees require the CLI on their own machine; this integration supports local worktrees.

repo-workflows-devcontainer-devcontainer-window-2d5a4455 = Terminal inside container

repo-workflows-devcontainer-devcontainer-window-3675738c = Worktree: { $config-path-display-1 }
    Ownership: { $config-label-2 }
    Configuration SHA256: { $model-hash-config-source-as-byte-3 }

repo-workflows-devcontainer-devcontainer-window-3a6ea0e2 = Starting runs this repository's Docker/build/features and lifecycle commands. Review the repository and its referenced files first. Rebuilding deletes only the displayed owned container; unsaved container-only files are lost.

repo-workflows-devcontainer-devcontainer-window-3c898437 = Uses installed Docker and the Dev Container CLI on this machine. Linux containers with an exact worktree bind mount are supported. Linked Git worktrees require standard relative POSIX metadata and the native common-directory mount. Compose or combined custom workspaceFolder/workspaceMount linked layouts are refused; metadata is never rewritten. Containers persist after closing this window.

repo-workflows-devcontainer-devcontainer-window-4227dbe9 = Open container in VS Code

repo-workflows-devcontainer-devcontainer-window-437e57b5 = Read worktree configuration again

repo-workflows-devcontainer-devcontainer-window-4564ef3d = Terminal handoff expired or was cancelled; open it again

repo-workflows-devcontainer-devcontainer-window-473f0412 = Container configuration SHA256: { $model-hash-binding-config-source-1 }

repo-workflows-devcontainer-devcontainer-window-5d2a90a9 = Container: { $binding-id-1 }
    Docker context: { $binding-engine-context-2 }
    Daemon: { $binding-engine-id-3 }
    Workspace: { $binding-folder-4 }
    User: { $binding-user-5 }

repo-workflows-devcontainer-devcontainer-window-6e69284d = Configuration changed: rebuild the displayed container before terminal/editor use. Containers from older app sessions/configurations can remain in Docker.

repo-workflows-devcontainer-devcontainer-window-7cb7e0b2 = Profile changed or operation cancelled. A created container may remain; inspect its displayed worktree labels before retrying.

repo-workflows-devcontainer-devcontainer-window-8f14be4f = Delete this container and rebuild (container-only files will be lost)

repo-workflows-devcontainer-devcontainer-window-a79d7c99 = Configuration detected; review it before starting

repo-workflows-devcontainer-devcontainer-window-b64a0d4f = Cancel operation

repo-workflows-devcontainer-devcontainer-window-b64ae277 = Worktree environment · Dev Containers

repo-workflows-devcontainer-devcontainer-window-bbd55e3e = Opened /bin/sh inside the displayed container

repo-workflows-devcontainer-devcontainer-window-c1f95bd8 = I trust this worktree's container configuration and lifecycle commands

repo-workflows-devcontainer-devcontainer-window-d66d64d1 = Environment started and its worktree mount/ownership checked

repo-workflows-devcontainer-devcontainer-window-d8db68fe = { $error }
    Start/rebuild may have partly completed. Check Docker availability, installed Dev Container CLI, build files and lifecycle-command errors above. No global cleanup is performed; inspect this worktree's labels before retrying.

repo-workflows-devcontainer-devcontainer-window-df4a39c8 = devcontainer.json (JSON with comments supported by native CLI)

repo-workflows-devcontainer-devcontainer-window-dfcb3d90 = Terminal uses Docker exec with this native remote user and /bin/sh; editor requires VS Code with its Dev Containers extension.

repo-workflows-devcontainer-editor-47c00405 = Container stopped; start the environment before editor handoff

repo-workflows-devcontainer-editor-806df56f = Handed the bound container workspace to VS Code. Its Dev Containers extension must be installed.

repo-workflows-devcontainer-engine-0af39fb5 = Invalid Docker context/daemon identity

repo-workflows-devcontainer-engine-161bfc12 = Missing Docker endpoint

repo-workflows-devcontainer-engine-1cfeec69 = Select a local Docker context and unset DOCKER_HOST before using this integration

repo-workflows-devcontainer-engine-a5b6c95b = This integration supports local Docker sockets. SSH/TCP Docker hosts require a separate native setup; no automatic remote handoff is attempted.

repo-workflows-devcontainer-engine-f6690c9e = Missing Docker daemon identity

repo-workflows-devcontainer-gitdir-pointer-a66f1615 = Unsupported .git pointer

repo-workflows-devcontainer-guard-7103bfb5 = Dev container configuration disappeared

repo-workflows-devcontainer-guard-d353a99e = Cancelled

repo-workflows-devcontainer-inspect-2227e9c4 = Invalid ownership label

repo-workflows-devcontainer-inspect-2e29e769 = Container belongs to another worktree, Docker daemon or selected profile

repo-workflows-devcontainer-inspect-3c611856 = Docker inspect must return an array

repo-workflows-devcontainer-inspect-85ded6e6 = Container ID or worktree/configuration ownership labels no longer match; refusing this target

repo-workflows-devcontainer-inspect-85fd3a35 = Container does not bind this exact worktree into its displayed workspace. Volume-only or parent-directory workspace mounts are currently unsupported.

repo-workflows-devcontainer-inspect-d934c6ee = Expected exactly one bound container

repo-workflows-devcontainer-json-0d639c4b = Container CLI result exceeds 8 MiB

repo-workflows-devcontainer-json-edd11b13 = Invalid { $tool } JSON result: { $e }

repo-workflows-devcontainer-linked-222d5e0e = Native common-directory mounting supports only the standard relative common/worktrees/name layout

repo-workflows-devcontainer-linked-7c9c7625 = Missing common Git directory

repo-workflows-devcontainer-linked-9990fafd = Linked Git metadata backlink does not point to this physical worktree

repo-workflows-devcontainer-linked-cdb7b1a0 = Missing Git metadata parent

repo-workflows-devcontainer-metadata-0a13f60e = Missing worktree metadata name

repo-workflows-devcontainer-metadata-735e6123 = This linked/symlink Git layout is unsupported by native container metadata mounting

repo-workflows-devcontainer-metadata-b2bab91f = Symlinked or nonstandard common Git metadata layout is unsupported

repo-workflows-devcontainer-metadata-b9388ff2 = Container requires a physical worktree root: { $e }

repo-workflows-devcontainer-metadata-file-a1a99ba5 = Git metadata pointer is not UTF-8

repo-workflows-devcontainer-metadata-file-bd4adcf1 = Linked metadata pointers must be regular files

repo-workflows-devcontainer-metadata-file-ddb7553c = Git metadata pointer exceeds 8 KiB

repo-workflows-devcontainer-metadata-ready-4d28a9de = Missing physical worktree mount

repo-workflows-devcontainer-metadata-ready-7575b58d = Missing worktree destination

repo-workflows-devcontainer-metadata-ready-8dad9e0c = Linked worktree Git metadata is not mounted read/write at its required relative location; environment is not ready

repo-workflows-devcontainer-metadata-ready-fea2d3e9 = Missing container mounts

repo-workflows-devcontainer-native-configuration-5aec3885 = Native CLI did not plan the required linked Git common-directory mount; environment cannot start or rebuild

repo-workflows-devcontainer-native-configuration-90cc8e0e = Linked worktree metadata mounting is unsupported for Compose or combined workspaceFolder/workspaceMount configurations; native CLI skips the required common-directory mount

repo-workflows-devcontainer-native-configuration-969b53cd = CLI did not return parsed configuration for linked worktree

repo-workflows-devcontainer-native-configuration-b328a4b3 = Invalid native workspaceFolder

repo-workflows-devcontainer-normalize-3fe3c368 = Git metadata path escapes the filesystem root

repo-workflows-devcontainer-relative-fe173813 = Linked containers require relative POSIX Git metadata paths; this layout is unsupported. Git metadata was not changed.

repo-workflows-devcontainer-same-97743985 = Worktree, configuration or selected profile changed; read configuration again

repo-workflows-devcontainer-start-6a0249a9 = Docker context/daemon changed since display; refusing rebuild

repo-workflows-devcontainer-start-87b3fed2 = { $pause-why-1 }; starting an environment can pull images/features

repo-workflows-devcontainer-started-046dacc8 = no success result

repo-workflows-devcontainer-started-19964d1c = Missing containerId

repo-workflows-devcontainer-started-43acb4ec = Missing remoteUser

repo-workflows-devcontainer-started-8c6f6a65 = Unsupported container workspace/user returned by CLI

repo-workflows-devcontainer-started-d1a64c4b = Missing remoteWorkspaceFolder

repo-workflows-devcontainer-started-f9fa046f = Dev Container CLI did not start the environment: { $value-description-as-str-unwrap--1 }

repo-workflows-devcontainer-terminal-77e417ed = Container { $binding-id-12-1 } · { $binding-folder-2 }

repo-workflows-devcontainer-terminal-b74a6469 = Container stopped; start the environment again before opening a terminal

repo-workflows-devcontainer-terminal-f171832a = Cancelled before terminal start

repo-workflows-doctor-ago-4603eba2 = { $secs-3600-1 } h ago

repo-workflows-doctor-ago-7ddb44d8 = just now

repo-workflows-doctor-ago-a0c430f4 = { $secs-60-1 } min ago

repo-workflows-doctor-attributes-2176c800 = diffs it

repo-workflows-doctor-attributes-4af8906f = filter={ $name } names a filter this configuration doesn't define: git stores and checks the file out as it is.

repo-workflows-doctor-attributes-68cb5f93 = Line endings · in the repository: { $eol-word-eol-index-1 } · in the working tree: { $eol-word-eol-worktree-2 } · attributes: { $if-eol-attr-is-empty-else-eol-at-3 }

repo-workflows-doctor-attributes-82806016 = filter=lfs, but Git LFS isn't set up here (git lfs install): checkouts leave pointer files, and files you add go into git itself, not LFS.

repo-workflows-doctor-attributes-c6b25148 = { $attr }={ $name } names a driver this configuration doesn't define ({ $attr }.{ $name }.*): git { $if-attr-merge-merges-it-else-dif-1 } as usual.

repo-workflows-doctor-attributes-cb2060cc = No attributes: git guesses text or binary for diffs and merges, and core.autocrlf alone decides line endings.

repo-workflows-doctor-attributes-ec97a4d2 = filter={ $name } is required but has no clean or smudge command here: git refuses to add or check out this file.

repo-workflows-doctor-attributes-f31e8d7f = merges it

repo-workflows-doctor-clear-7f76d2cf = Clear the flag of { $one-path-1 }

repo-workflows-doctor-clear-83b12c22 = Clear

repo-workflows-doctor-clear-909d7fba = Clear the flags?

repo-workflows-doctor-clear-ac70d25c = { $files-len-1 } files

repo-workflows-doctor-clear-ae587a37 = Git will look at { $what } again: local edits show under Changes, where Discard would throw them away. Which files were flagged isn't kept, so this can't be undone here.

repo-workflows-doctor-clear-e318945b = Clear the flags of { $files-len-1 } files

repo-workflows-doctor-confirm-modal-19766ed6 = Cancel

repo-workflows-doctor-confirm-modal-3b13192b = Discard your changes?

repo-workflows-doctor-confirm-modal-c324130d = Your changes to { $file } aren't saved: closing the doctor drops them.

repo-workflows-doctor-confirm-modal-e76fd2ad = Keep editing

repo-workflows-doctor-confirm-modal-eb1a70e3 = Discard

repo-workflows-doctor-details-ba8d6068 = No file sets it: git uses its built-in default.

repo-workflows-doctor-doctor-act-9bab8d95 = Wait for the running operation to end

repo-workflows-doctor-doctor-ui-3095e533 = Repository doctor

repo-workflows-doctor-doctor-ui-8c6c9cf3 = Back (Esc)

repo-workflows-doctor-doctor-ui-d8cb92fa = Read again

repo-workflows-doctor-editor-0ba2a573 = Nothing changed

repo-workflows-doctor-editor-1509f561 = Save

repo-workflows-doctor-editor-1ee81c8e = { $e-file-label-1 } isn't a file of this repository: it applies to every repository that reads it. Click again to save it.

repo-workflows-doctor-editor-23e39291 = Saving…

repo-workflows-doctor-editor-324b134f = Preview

repo-workflows-doctor-editor-52448408 = Editing { $e-file-label-1 }

repo-workflows-doctor-editor-5902680c = Save outside the repository

repo-workflows-doctor-editor-6c1adba2 = Read it again

repo-workflows-doctor-editor-7d9eb7ac = Close

repo-workflows-doctor-editor-a875327e = Change the rules first

repo-workflows-doctor-editor-b5bfc677 = Editing { $e-file-label-1 } · not saved

repo-workflows-doctor-editor-b5c82673 = Wait for the running operation

repo-workflows-doctor-editor-f9bfa3dc = Discard changes

repo-workflows-doctor-editor-fca046cc = Saved.

repo-workflows-doctor-eol-notes-384973dd = No text or eol attribute: each machine's core.autocrlf decides its line endings, which is how line-ending churn starts; `* text=auto` in .gitattributes makes it the same everywhere.

repo-workflows-doctor-eol-notes-868d3e72 = Stored in the repository with CRLF or mixed line endings: whoever has core.autocrlf set will see the whole file change.

repo-workflows-doctor-eol-word-ec3e00be = no line breaks

repo-workflows-doctor-explain-card-3b61e4e9 = Explain

repo-workflows-doctor-explain-card-7486f5b7 = Explain a file

repo-workflows-doctor-explained-55b528ba = Save or discard the changes being edited first

repo-workflows-doctor-explained-98353ed7 = Edit this file

repo-workflows-doctor-folder-card-05f75db2 = Set by core.hooksPath ({ $path }), instead of .git/hooks.

repo-workflows-doctor-folder-card-34845b15 = The repository's own (.git/hooks): no core.hooksPath is set.

repo-workflows-doctor-folder-card-548df737 = Hooks folder

repo-workflows-doctor-folder-card-5cc7fab9 = manages the hooks here: { $m-advice-1 }

repo-workflows-doctor-folder-card-605924cc = { $n } .sample files: examples git ships, never run.

repo-workflows-doctor-folder-card-63c1e34d = 1 .sample file: an example git ships, never run.

repo-workflows-doctor-folder-card-8fa06071 = core.hooksPath is set to nothing: git runs no hook at all. Unset it (Which configuration wins) to run the hooks again.

repo-workflows-doctor-history-card-06c896b6 = History in the graph

repo-workflows-doctor-history-card-5a389005 = Each reload reads the history again and rebuilds the graph: with a commit-graph, git reads it several times faster.

repo-workflows-doctor-history-card-cd12e41d = This repository has { $group-h-commits-unwrap-or-defaul-1 } commits, more than the graph shows: the oldest ones are only reachable from the terminal (git log) or a file's history. Hiding branches or soloing some narrows the graph to theirs.

repo-workflows-doctor-hook-row-432f4eba = Convert to LF

repo-workflows-doctor-hook-row-433c4667 = runs on { $on }

repo-workflows-doctor-hook-row-96c0aa61 = Make executable

repo-workflows-doctor-hooks-act-63222fea = made executable

repo-workflows-doctor-hooks-act-fdf85121 = saved with LF line endings

repo-workflows-doctor-hooks-card-0dfb736a = Other files (never run)

repo-workflows-doctor-hooks-card-52f0510d = 1 hook

repo-workflows-doctor-hooks-card-69d27675 = Git runs no hook of these names: a typo, or a copy kept aside.

repo-workflows-doctor-hooks-card-6c856ad1 = No hook runs: every operation goes ahead without one.

repo-workflows-doctor-hooks-card-84fbcccd = No active hooks

repo-workflows-doctor-hooks-card-d028a324 = { $n } hooks

repo-workflows-doctor-ignored-card-0384b20e = Ignored files

repo-workflows-doctor-ignored-card-1303e9b5 = Nothing is ignored.

repo-workflows-doctor-ignored-card-37abda7f = Ignored files ({ $total })

repo-workflows-doctor-ignored-card-683768ab = and { $total-list-len-1 } more

repo-workflows-doctor-ignored-card-cdd2ef27 = Explain it

repo-workflows-doctor-ignored-card-fb0ae18f = Untracked files git ignores; a folder stands for all it holds. Click one to explain it.

repo-workflows-doctor-manual-card-14faa67e = Clear the { $ticked-len-1 } ticked

repo-workflows-doctor-manual-card-261a3c3d = No flagged files

repo-workflows-doctor-manual-card-29a390f9 = Clear all

repo-workflows-doctor-manual-card-6c8cdf68 = Clear ticked

repo-workflows-doctor-manual-card-70727cd6 = { $n } flagged files

repo-workflows-doctor-manual-card-98cd282b = Tick the files to clear first

repo-workflows-doctor-manual-card-b5c82673 = Wait for the running operation

repo-workflows-doctor-manual-card-bf185132 = Its name isn't valid UTF-8: clear it with git update-index in a terminal

repo-workflows-doctor-manual-card-eb376b6a = No tracked file is marked assume-unchanged or skip-worktree.

repo-workflows-doctor-manual-card-f01d6b0e = 1 flagged file

repo-workflows-doctor-manual-card-febfc014 = Git looks at every one of these files again

repo-workflows-doctor-more-683768ab = and { $total-shown-1 } more

repo-workflows-doctor-open-doctor-745454bd = Finish or close the conflict editor or the interactive rebase first

repo-workflows-doctor-origin-label-4cf3860a = gitcraft profile

repo-workflows-doctor-other-worktrees-1aef9399 = not set

repo-workflows-doctor-other-worktrees-965a2b09 = Compare with the other worktrees

repo-workflows-doctor-output-card-01623799 = Output of the last operations

repo-workflows-doctor-output-card-109b47bc = … { $lines-len-shown-len-1 } lines above

repo-workflows-doctor-output-card-50f9b973 = Nothing yet: no operation gitcraft ran that runs hooks printed anything (what runs in the terminal isn't kept).

repo-workflows-doctor-output-card-ae5bfc74 = What git and its hooks printed during the last operations gitcraft ran in this repository (commit, merge, push, checkout…), the last dozen. Kept in memory only, with passwords in URLs hidden.

repo-workflows-doctor-preview-0f241c6b = No longer ignored

repo-workflows-doctor-preview-63666b35 = Newly ignored

repo-workflows-doctor-preview-683768ab = and { $paths-len-preview-shown-1 } more

repo-workflows-doctor-preview-8c8b9b2a = Tracked files this file's rules match (they stay tracked)

repo-workflows-doctor-preview-bbb0c0cc = This change wouldn't start or stop ignoring any untracked file.

repo-workflows-doctor-preview-cd2605b4 = By this file's rules alone: rules in other files are left out, so a ! rule here that takes back what another file ignores doesn't show, and another file may still ignore a path listed here.

repo-workflows-doctor-remove-confirm-9861b6eb = Remove { $origin-key-1 }?

repo-workflows-doctor-remove-confirm-b0b26eef = Remove { $origin-key-1 } ({ $shortened-origin-shown-value-2 }) from { $file }

repo-workflows-doctor-remove-confirm-c3812fc4 = Remove

repo-workflows-doctor-rules-card-379ee96f = Edit one, preview what the change would ignore, then save it.

repo-workflows-doctor-rules-card-464c4ffd = Edit

repo-workflows-doctor-rules-card-55b528ba = Save or discard the changes being edited first

repo-workflows-doctor-rules-card-91b1b197 = Files of ignore rules

repo-workflows-doctor-scope-choice-0eb26325 = This worktree

repo-workflows-doctor-scope-choice-38429e22 = This repository

repo-workflows-doctor-scope-choice-59b8031f = All my repositories

repo-workflows-doctor-scope-choice-ad944dcb = Everyone on this machine

repo-workflows-doctor-scope-choice-c925a118 = Command line

repo-workflows-doctor-set-row-2b7c9578 = Your global git config on that machine (usually ~/.gitconfig), for all your repositories there.

repo-workflows-doctor-set-row-30a28bf1 = Choose a value first

repo-workflows-doctor-set-row-3da50e6f = Git's system file, for every user of this machine; usually needs administrator rights.

repo-workflows-doctor-set-row-46f01635 = Type a value first

repo-workflows-doctor-set-row-6ec46474 = Needs extensions.worktreeConfig = true in this repository: without it git refuses when there are other worktrees, and with only one writes .git/config

repo-workflows-doctor-set-row-85b9cd3d = Set it for

repo-workflows-doctor-set-row-921f564b = Set { $setting-key-1 } to { $shortened-value-2 } for { $scope-choice-scope-to-lowercase-3 }

repo-workflows-doctor-set-row-a215cc6f = Set { $setting-key-1 } for { $scope-choice-scope-to-lowercase-2 }?

repo-workflows-doctor-set-row-ad30af45 = Your global git config (usually ~/.gitconfig), for all your repositories. A gitcraft profile that sets the same thing still wins over it.

repo-workflows-doctor-set-row-b5c82673 = Wait for the running operation

repo-workflows-doctor-set-row-b6f6f3ad = Set

repo-workflows-doctor-set-row-cadb5751 = Only this worktree (its config.worktree file).

repo-workflows-doctor-set-row-cc3edd84 = git's system file, for every user of this machine

repo-workflows-doctor-set-row-ccc71902 = { $setting-key-1 } = { $value } goes into { $if-scope-scope-global-your-globa-2 }. The value it replaces there isn't kept.

repo-workflows-doctor-set-row-e0a790f9 = your global git config, for all your repositories

repo-workflows-doctor-set-row-e0d5ce0a = Only this repository (.git/config). An include later in that file that sets it still wins.

repo-workflows-doctor-setting-row-2224c772 = not set: git's default

repo-workflows-doctor-setting-row-8f8c7445 = overrides { $values-len-1-1 }

repo-workflows-doctor-setting-row-8ffd7cc3 = Git default

repo-workflows-doctor-size-8333f24e = { NUMBER($kib-as-f64-1024-0-1, minimumFractionDigits: 1, maximumFractionDigits: 1) } MiB

repo-workflows-doctor-size-95886483 = { $kib } KiB

repo-workflows-doctor-size-9d2b746f = { NUMBER($kib-as-f64-1-048-576-0-1, minimumFractionDigits: 2, maximumFractionDigits: 2) } GiB

repo-workflows-doctor-size-card-1af85190 = Size

repo-workflows-doctor-size-card-1f2fe3f1 = { $o-packed-1 } in { $o-packs-2 } packs, { $size-o-pack-kib-3 }

repo-workflows-doctor-size-card-35195164 = Packed objects

repo-workflows-doctor-size-card-52c9d559 = { $o-garbage-1 } files in the objects folder aren't objects ({ $size-o-garbage-kib-2 }): left by an interrupted operation; git gc reports them.

repo-workflows-doctor-size-card-8757152b = Loose objects

repo-workflows-doctor-size-card-b83a7e1d = Loose and also packed

repo-workflows-doctor-size-card-d09648c6 = Commits

repo-workflows-doctor-sparse-card-04bc71e9 = Left out by sparse-checkout ({ $left-out-len-1 })

repo-workflows-doctor-sparse-card-7bb29a8c = Git marks these skip-worktree too, but they are sparse-checkout's: change what is checked out with git sparse-checkout (set, add, disable), not by clearing flags.

repo-workflows-doctor-sparse-card-9cff4083 = Sparse-checkout is on in cone mode with only the files at the top checked out.

repo-workflows-doctor-sparse-card-ce4ee135 = Sparse-checkout is on in cone mode; checked out: { $dirs-join-1 }.

repo-workflows-doctor-sparse-card-f364e530 = Sparse-checkout is on with patterns: every skip-worktree file counts as left out by them, so none of them is cleared here.

repo-workflows-doctor-speed-card-0ad7db50 = git status: { $took-as-millis-1 } ms (gitcraft runs it on every change it sees).

repo-workflows-doctor-speed-card-2154776b = How long reading it takes

repo-workflows-doctor-speed-card-8636e9ce = { $d-as-millis-1 } ms

repo-workflows-doctor-speed-card-aebc38ca = The tab's last reloads (branches, history, changes, all at once): { $times-join-1 }.

repo-workflows-doctor-speed-card-ffe6b51a = No reload of the tab timed yet.

repo-workflows-doctor-suggestion-2b7f0cfe = not available here

repo-workflows-doctor-suggestion-5a1f096a = Turn on

repo-workflows-doctor-suggestion-5a853d12 = Pack the objects

repo-workflows-doctor-suggestion-a96e0fc8 = git's own monitor exists on Windows and macOS only (and not on network drives unless fsmonitor.allowRemote is set); on Linux a Watchman hook can stand in (core.fsmonitor naming it).

repo-workflows-doctor-suggestion-b5c82673 = Wait for the running operation

repo-workflows-doctor-suggestion-bd63228b = File system monitor

repo-workflows-doctor-suggestions-card-e930084c = What would make it faster

repo-workflows-doctor-title-1e6bed5e = Hidden changes

repo-workflows-doctor-title-8b2d57fa = Which configuration wins

repo-workflows-doctor-title-8d6acfc3 = Health and maintenance

repo-workflows-doctor-title-94ec061c = Ignore rules and attributes

repo-workflows-doctor-title-d412a062 = Hooks

repo-workflows-doctor-turn-on-action-3c97132f = Schedule maintenance?

repo-workflows-doctor-turn-on-action-7cde0a5e = { $commands-join-1 } adds this repository to maintenance.repo in your own global git config, sets maintenance.auto=false and maintenance.strategy=incremental in its config, and schedules git to run in the background: every hour it fetches from the remotes into refs/prefetch. The scheduled runs read your own global config, not a gitcraft profile's. git maintenance unregister undoes it for this repository (and git config --unset maintenance.auto brings automatic gc back); git maintenance stop removes the schedule for every repository.

repo-workflows-doctor-turn-on-action-d8e3e02d = Turn on: { $s-title-to-lowercase-1 }

repo-workflows-doctor-turn-on-action-f4830a1d = Schedule

repo-workflows-doctor-turn-on-action-f688c962 = Scheduled maintenance

repo-workflows-doctor-ui-089db75c = Show settings nobody set

repo-workflows-doctor-ui-11781882 = Hooks are programs git runs at points of an operation: before a commit, before a push, after a checkout. A hook that fails stops the operation, and one that can't run is skipped or fails with a cryptic message.

repo-workflows-doctor-ui-195ee0f7 = Files changed on disk that never show under Changes are often flagged in git's index. Both flags hide a file's changes; they mean different things, and git supports neither as a way to ignore changes to tracked files.

repo-workflows-doctor-ui-1f9897b7 = Git reads the system file, then your global one, then this repository's and this worktree's, then values given on the command line; for a setting with one value, the last value read wins. With a gitcraft profile, the global file is the profile's: it reads ~/.gitconfig first, then its own values on top (git run outside gitcraft doesn't see it). Values from include or includeIf files show under their own file. Only these settings are shown: nothing credential-related.

repo-workflows-doctor-ui-282e3905 = assume-unchanged: a promise to git that the file won't change, so it doesn't check it, for speed on slow disks. Git may still overwrite the file (a pull, a checkout) and drop the flag; it is not for keeping local edits.

repo-workflows-doctor-ui-2bfe98f3 = No setting here matches.

repo-workflows-doctor-ui-4e47a723 = Filter settings

repo-workflows-doctor-ui-56146558 = The two flags

repo-workflows-doctor-ui-678f7b82 = Why a file seems to vanish, why line endings keep changing, why a merge driver doesn't run: the rule that ignores a file and the attributes git gives it, as the working tree has them. A file's menu (Ignore rules and attributes) brings it here.

repo-workflows-doctor-ui-99079609 = skip-worktree: git keeps the working-tree version and doesn't compare it with the index. Sparse-checkout uses it; people also use it to keep local edits to a tracked file, which git warns isn't supported: a pull, checkout or merge that changes the file may refuse to run until the flag is cleared. For local settings, an untracked override file is safer.

repo-workflows-doctor-ui-9b3b999d = A fix is being made

repo-workflows-doctor-ui-b5c82673 = Wait for the running operation

repo-workflows-doctor-ui-c7deeea1 = How big the repository is, how long reading it takes, and what would make git faster here. Every speed-up is git's own, set for this repository: it helps git everywhere, not only in gitcraft.

repo-workflows-doctor-value-row-29f009c0 = Remove every { $origin-key-1 } from this file; the next value down (or git's default) then applies

repo-workflows-doctor-value-row-4b67153d = In effect

repo-workflows-doctor-value-row-4d8ff0fe = Given on the command line or in GIT_CONFIG_PARAMETERS: not in a file

repo-workflows-doctor-value-row-62a03f09 = Overridden

repo-workflows-doctor-value-row-b5c82673 = Wait for the running operation

repo-workflows-doctor-value-row-c3812fc4 = Remove

repo-workflows-doctor-value-row-fd2cc5af = gitcraft writes this file from Settings → Profile: change it there

repo-workflows-doctor-verdict-050f8d52 = Tracked: ignore rules don't apply to files git already tracks, so this rule has no effect on it. To have it ignored, stop tracking it (git rm --cached; the file stays on disk).

repo-workflows-doctor-verdict-2459673a = No ignore rule matches it.

repo-workflows-doctor-verdict-765462d4 = Its folder is ignored: git doesn't look inside, so a ! rule for a file in it has no effect. Un-ignore the folder (!folder/), or ignore folder/* instead.

repo-workflows-doctor-verdict-a1c6264a = Tracked, and no ignore rule matches it.

repo-workflows-doctor-verdict-a292bdad = Not ignored: this ! rule is the last one that matches it.

repo-workflows-doctor-verdict-b78474c9 = A folder: the rule shown decides for everything in it (git doesn't look inside an ignored folder).

repo-workflows-doctor-verdict-b796f495 = Ignored by this rule.

repo-workflows-doctor-verdict-c3b9d549 = There is no such file here: the rules are read as for a file of that name (a rule for folders, ending in /, can't match it).

repo-workflows-doctor-verdict-f0d742e1 = Tracked, and the last rule that matches it is this ! rule.

repo-workflows-drift_view-body-31794226 = Measure again

repo-workflows-drift_view-body-3e453620 = Ask a coding agent (optional)

repo-workflows-drift_view-body-42a9eb31 = Save task

repo-workflows-drift_view-body-4bc74b21 = Task

repo-workflows-drift_view-body-65fb660d = What the agent was asked to do (kept with the worktree, never committed)

repo-workflows-drift_view-body-7d9eb7ac = Close

repo-workflows-drift_view-body-a8164b33 = Scope drift: { $view-worktree-name-1 } ({ $branch })

repo-workflows-drift_view-body-cdc38d02 = Linked issue #{ $n }: { $text-lines-next-unwrap-or-defaul-1 }

repo-workflows-drift_view-body-e039d62e = Runs claude -p (or codex exec) here with the task and the paths, no file contents, and shows which items it thinks the task doesn't need

repo-workflows-drift_view-body-fa4f662d = Linked issue #{ $n }: { $e }

repo-workflows-drift_view-item-518baa09 = Not committed in that worktree: opens it, where its changes are

repo-workflows-drift_view-item-bccfdf69 = Opens its diff

repo-workflows-drift_view-item-d91938be = Opens:
    { $tip-join-n-1 }

repo-workflows-drift_view-measure-drift-27f1506d = The repository's forge isn't known

repo-workflows-drift_view-report-3ecc0ca7 = Asked: { $task-lines-next-unwrap-or-defaul-1 }

repo-workflows-drift_view-report-57f4cf0a = No task to go by: write one above (or link an issue) to tell the folders apart.

repo-workflows-drift_view-save-task-a40b28e7 = Saved beside the worktree's git data

repo-workflows-drift_view-sections-14063185 = New dependencies ({ $report-dependencies-len-1 })

repo-workflows-drift_view-sections-1bb434ba = Folders the task doesn't mention ({ $unrelated-len-1 })

repo-workflows-drift_view-sections-4cf1b0e4 = (top level)

repo-workflows-drift_view-sections-619bb740 = Every file changed ({ $report-files-len-1 })

repo-workflows-drift_view-sections-819e9567 = { $dep-name-1 }  in { $dep-manifest-2 }

repo-workflows-drift_view-sections-a8299dbd = Public signatures changed ({ $report-signatures-len-1 })

repo-workflows-email-apply-61c57973 = Invalid mailbox path

repo-workflows-email-compare-6921220f = Commit or stash worktree changes first

repo-workflows-email-compare-b5921a15 = Worktree HEAD, branch, changes, configuration or selected profile changed; read/preview again

repo-workflows-email-email-act-3eece223 = Recipients changed; generate a new preview

repo-workflows-email-email-act-5474eef8 = Working…

repo-workflows-email-email-act-84f8f92f = Cancelled before starting

repo-workflows-email-email-act-c19c9f05 = Email patches · b4

repo-workflows-email-email-act-fa043d04 = Not enrolled or b4 unavailable: { $e }

repo-workflows-email-email-window-035719bf = Previous sent Message-ID

repo-workflows-email-email-window-03712771 = Receive and review

repo-workflows-email-email-window-1219b3d1 = Set numbered revision

repo-workflows-email-email-window-15173718 = Fetch series with b4

repo-workflows-email-email-window-179e0bb7 = Public-inbox HTTPS URL or Message-ID

repo-workflows-email-email-window-1e3943ad = Read prepared branch and cover

repo-workflows-email-email-window-2a42475c = Preview generated without sending mail

repo-workflows-email-email-window-2e516d68 = Revision

repo-workflows-email-email-window-381b0db7 = Author and reroll

repo-workflows-email-email-window-3d343b32 = Generate outgoing preview (no mail)

repo-workflows-email-email-window-49077514 = Revision (0 = latest)

repo-workflows-email-email-window-54194499 = I understand b4 preparation can rewrite this branch and change its metadata

repo-workflows-email-email-window-54cdddfe = Preview and send

repo-workflows-email-email-window-582e1946 = Save cover with b4

repo-workflows-email-email-window-624d6ba0 = Series fetched; review the cover, patches and trailers before applying

repo-workflows-email-email-window-632693ae = { $error }
    If cancelled or partially completed, inspect Git/b4/mail state before retrying. Apply conflicts use the existing Continue/Abort operation controls.

repo-workflows-email-email-window-6ad695fc = I reviewed every recipient and message and want to send real mail

repo-workflows-email-email-window-8c1d9c9f = Profile changed. Operation cancelled; inspect repository and mail state before retrying.

repo-workflows-email-email-window-8c3ad598 = Requires the supported b4 source build ce4c1158 (0.17-dev), installed with its Python entrypoint in a virtual environment on this machine. The Python helper uses isolated mode, so user-site/PYTHONPATH-only installs are unsupported. Other builds are refused until their contract is reviewed. Uses b4's configured identity, public-inbox and sending transport. No credentials are exported by gitcraft.

repo-workflows-email-email-window-99fe2a7b = Sending regenerates volatile Date, Message-ID and signature headers. Recipients, subjects and decoded MIME content are checked against this preview. b4's successful-send tagging/reroll behavior applies. Native failed-preflight or unknown prompts abort; only the final send confirmation is answered by this explicit approval.

repo-workflows-email-email-window-9cb42639 = Fetched for HEAD { $received-snapshot-head-1 } · { $received-messages-len-2 } patches

repo-workflows-email-email-window-b0fcf938 = Additional recipients. b4 also derives recipients from its cover/configuration and review trailers; inspect every generated message below.

repo-workflows-email-email-window-b28037e9 = Cc

repo-workflows-email-email-window-b64a0d4f = Cancel operation

repo-workflows-email-email-window-b8c5cd6e = Prepared branch read

repo-workflows-email-email-window-bc346e4f = Enroll current branch

repo-workflows-email-email-window-c19c9f05 = Email patches · b4

repo-workflows-email-email-window-c6755006 = Worker ended or was cancelled. Inspect repository/mail state before retrying; mail may have been delivered.

repo-workflows-email-email-window-c962be80 = Fork point

repo-workflows-email-email-window-d9adba22 = Cover letter

repo-workflows-email-email-window-e6884e31 = Send reviewed series through b4

repo-workflows-email-email-window-e6ec08b2 = Reroll from sent series

repo-workflows-email-email-window-e70c5648 = Apply reviewed series (git am --3way)

repo-workflows-email-email-window-f4b06ef6 = To

repo-workflows-email-guard-d353a99e = Cancelled

repo-workflows-email-interpreter-409abb66 = Installed b4 must have a Python script entrypoint

repo-workflows-email-interpreter-d5e7d057 = Unsupported b4 entrypoint; use the supported source build's Python entrypoint

repo-workflows-email-interpreter-fdf4c44a = b4 entrypoint has no Python shebang

repo-workflows-email-invoke-b4-8728290f = Install b4 in an absolute executable PATH directory (a Python virtual environment is supported)

repo-workflows-email-invoke-b4-c9d943f4 = Cannot read installed b4 entrypoint: { $e }

repo-workflows-email-mailbox-10ff6da2 = Series exceeds 32 MiB

repo-workflows-email-mailbox-52374212 = Expected b4's git-am mailbox envelope

repo-workflows-email-mailbox-ede3c4f9 = More than 1024 messages

repo-workflows-email-messages-07ed6ccf = Mail

repo-workflows-email-messages-1edf47d5 = Subject:

repo-workflows-email-messages-8e0d8018 = Message { $index-1-1 } · { $message-headers-lines-find-l-l-s-2 }

repo-workflows-email-messages-a6252664 = Raw RFC822 message

repo-workflows-email-new-21656993 = Cannot allocate private mail directory

repo-workflows-email-parse-8107bb41 = Mail exceeds 32 MiB

repo-workflows-email-parse-ff511962 = Mail has no Subject header

repo-workflows-email-parse-ff6e2278 = Incomplete mbox envelope

repo-workflows-email-parts-18af1d8d = [{ $mail-ctype-mimetype-1 } attachment: { $raw-len-2 } decoded bytes]{ "\u000A" }

repo-workflows-email-parts-93b357ba = Mail MIME nesting exceeds 24 levels

repo-workflows-email-prepare-7fa1230f = Expected a positive revision number

repo-workflows-email-prepare-ad82832b = Unknown preparation action

repo-workflows-email-prepare-b8cd601f = Enter a fork-point revision

repo-workflows-email-prepare-d287cfde = Revision must be positive

repo-workflows-email-preview-2345df14 = Cannot read bounded outgoing message

repo-workflows-email-preview-7a897da3 = Invalid outgoing filename

repo-workflows-email-preview-ca11bd96 = More than 1024 outgoing messages

repo-workflows-email-preview-d135ecfa = Outgoing series exceeds 32 MiB

repo-workflows-email-preview-ff2f2c5c = b4 produced no outgoing messages

repo-workflows-email-receive-3b985b29 = Cannot read bounded mailbox

repo-workflows-email-receive-4350d1be = b4 produced no mailbox

repo-workflows-email-receive-61e11166 = Invalid mailbox name

repo-workflows-email-receive-7fcd485d = Non-UTF8 b4 output filename

repo-workflows-email-receive-9820d0e4 = Cannot read bounded cover letter

repo-workflows-email-receive-b0772e5b = b4 produced several mailboxes; select a single series

repo-workflows-email-recipients-5fb6b41e = Invalid recipient list

repo-workflows-email-recipients-c706c00a = Empty recipient list

repo-workflows-email-recipients-e0cee753 = Recipient list: { $e }

repo-workflows-email-reroll-id-17d9d618 = Unbalanced Message-ID brackets

repo-workflows-email-reroll-id-bbeca305 = Use the previous sent Message-ID, not a URL

repo-workflows-email-send-9753828b = Send did not report success: { $e }. Some mail may already have been delivered. Inspect b4/mail state before retrying.

repo-workflows-email-send-a590b131 = b4 reported a successful send. Inspect native sent tags and revision before sending again.
    { $output }

repo-workflows-email-send-reviewed-e9663ecb = Outgoing recipients or content changed; preview again before sending

repo-workflows-email-snapshot-2627ca9e = .b4-config must be readable inside the worktree and at most 1 MiB

repo-workflows-email-snapshot-a55ce185 = Email patches currently require a worktree on this machine and the supported b4 ce4c1158 (0.17-dev) source build installed there

repo-workflows-email-source-034af213 = Enter a public-inbox HTTPS URL or Message-ID

repo-workflows-email-source-51ea03e4 = Inbox URL cannot contain credentials

repo-workflows-email-source-60f601ea = Expected a HTTPS URL or Message-ID

repo-workflows-fence_view-body-7d9eb7ac = Close

repo-workflows-fence_view-body-803f082f = Agent fence

repo-workflows-fence_view-body-98912c89 = Also write a section of { $fence-agents-1 } (Codex)

repo-workflows-fence_view-body-a8aa1101 = Save and write to agents

repo-workflows-fence_view-body-b31ba75f = A section of its own; a file the repository usually tracks, so it shows as changed

repo-workflows-fence_view-body-b42a9a6f = Add suggestions

repo-workflows-fence_view-body-c408ae8c = Also write { $fence-cursor-1 }

repo-workflows-fence_view-body-cac5a24a = Not written into every worktree's configs:
    { $write-errors-join-n-1 }

repo-workflows-fence_view-body-d10df00e = A block of its own; a file the repository may track, so it shows as changed

repo-workflows-fence_view-body-d1953461 = Paths agents must stay out of, one per line, in .gitignore's syntax without exceptions (secrets/, .env, *.pem, /Cargo.lock), any case. Saved in this repository's git config, shared by its worktrees, and written into each worktree's Claude Code deny rules ({ $fence-claude-1 }, kept out of git), now and for worktrees added later. Rules can leak, so every linked worktree is checked too: one whose commits, changes, untracked or ignored files touch the fence (or that can't be checked) turns red. A merge, fast-forward or rebase onto any branch is checked first and asks when its commits or its worktree touched the fence.

repo-workflows-fence_view-confirm-body-e537b5dc = { $branch } (checked out in the worktree { $wt })

repo-workflows-fence_view-fence-gate-214a3091 = Merge anyway

repo-workflows-fence_view-fence-gate-2b8e4f3b = Checking against the agent fence…

repo-workflows-fence_view-fence-gate-6b730ef5 = Checking against the agent fence

repo-workflows-fence_view-list-violations-a6d5a69b = { "  " }and { $found-len-listed-1 } more

repo-workflows-fence_view-save-7bfa9a3b = Saved, but not everywhere:
    { $errors-join-n-1 }

repo-workflows-file_comparison-assemble-1641af0b = More than 50,000 patch lines. Compare a smaller range.

repo-workflows-file_comparison-assemble-56abde67 = Moved-code comparison cancelled

repo-workflows-file_comparison-boundary-26efe746 = Binary or non-UTF8 versions: compare the byte preview; line staging is unavailable

repo-workflows-file_comparison-boundary-5421cc1b = The raw patch is not UTF-8

repo-workflows-file_comparison-boundary-6732715f = Git could not produce a raw text patch for these differing bytes; use the normal patch view

repo-workflows-file_comparison-boundary-b861cbb0 = Git could not align these raw versions

repo-workflows-file_comparison-boundary-button-033f86f9 = Unstage

repo-workflows-file_comparison-boundary-button-0cadc525 = { $text } lens row { $index-1-1 }

repo-workflows-file_comparison-boundary-button-de838855 = Stage

repo-workflows-file_comparison-boundary-e44610d7 = More than 20,000 lines in a version; use the normal patch view

repo-workflows-file_comparison-comparison-pick-28dc27ab = { $pick-verb-1 } { $snapshot-spec-path-2 } from staging lens

repo-workflows-file_comparison-file-comparison-view-04166b68 = Files are rows; pinned branch commits are columns. V-markers identify equal bytes, including across rows. Missing cells and file modes are separate. Click two cells to open their content diff.

repo-workflows-file_comparison-file-comparison-view-103acf25 = Read three versions

repo-workflows-file_comparison-file-comparison-view-1e2e1383 = Choose branches ({ $state-selected-len-1 } selected, maximum 8)

repo-workflows-file_comparison-file-comparison-view-21819769 = From

repo-workflows-file_comparison-file-comparison-view-22d5cf54 = Raw snapshots · refresh after repository changes · no automatic downloads

repo-workflows-file_comparison-file-comparison-view-2ad51b3c = Code moved across files

repo-workflows-file_comparison-file-comparison-view-2d2cd49c = Staged moves

repo-workflows-file_comparison-file-comparison-view-327070c2 = HEAD → staged → working copy. Stage or unstage lines at either boundary; refresh if the file changes. Binary files and files requiring Git conversions stay read-only here.

repo-workflows-file_comparison-file-comparison-view-3f64b850 = Refresh file view

repo-workflows-file_comparison-file-comparison-view-40379eb3 = Open normal patch

repo-workflows-file_comparison-file-comparison-view-50009ce1 = File

repo-workflows-file_comparison-file-comparison-view-51d4ac7c = Commit → working copy

repo-workflows-file_comparison-file-comparison-view-5fa4365c = Reading immutable versions…

repo-workflows-file_comparison-file-comparison-view-725513de = Comparison cancelled. Refresh to try again.

repo-workflows-file_comparison-file-comparison-view-74ccd433 = Folder

repo-workflows-file_comparison-file-comparison-view-7b8bea18 = Close file view

repo-workflows-file_comparison-file-comparison-view-82a9c46f = Commit

repo-workflows-file_comparison-file-comparison-view-86154135 = Find file migrations

repo-workflows-file_comparison-file-comparison-view-89fa7940 = To / commit

repo-workflows-file_comparison-file-comparison-view-90e2e8fa = File versions across branches

repo-workflows-file_comparison-file-comparison-view-aa2b8150 = Git detects renames/copies at 50% similarity, within 2,000 candidates. Stage reorganized files first to include newly named working files. Content edits open separately from the move.

repo-workflows-file_comparison-file-comparison-view-ca74fb37 = Cancel comparison

repo-workflows-file_comparison-file-comparison-view-d1d29085 = Working copy

repo-workflows-file_comparison-file-comparison-view-daec459a = Staged

repo-workflows-file_comparison-file-comparison-view-dfc3916d = Find moved code

repo-workflows-file_comparison-file-comparison-view-edc82098 = File migration map

repo-workflows-file_comparison-file-comparison-view-efa19808 = Commit moves

repo-workflows-file_comparison-file-comparison-view-f08307c5 = Compare deleted and added blocks across tracked files; whitespace around lines is ignored. Stage newly created files first. Analysis is bounded to 300 files, 50,000 lines and 40 seconds; encrypted files are excluded.

repo-workflows-file_comparison-file-comparison-view-f2473f24 = Between commits

repo-workflows-file_comparison-file-comparison-view-f315d745 = Repository root

repo-workflows-file_comparison-file-comparison-view-f4b06ef6 = To

repo-workflows-file_comparison-file-comparison-view-fb60b8e0 = Three-state staging lens

repo-workflows-file_comparison-file-comparison-view-fdc19d89 = Compare branch versions

repo-workflows-file_comparison-invalidate-81f92ce0 = Repository changed. Refresh the file view.

repo-workflows-file_comparison-lens-3a1781b9 = Long lines are abbreviated at 2,000 characters; boundary actions use the complete raw line. Use the normal patch to inspect it.

repo-workflows-file_comparison-lens-4902e29c = Working copy matches HEAD, but the next commit still changes this file. Working edits undo the staged edit.

repo-workflows-file_comparison-lens-6e60c4a1 = Unstage entire boundary

repo-workflows-file_comparison-lens-75673075 = ← unstage

repo-workflows-file_comparison-lens-8f600488 = Stage entire boundary

repo-workflows-file_comparison-lens-92bc93d0 = ← stage

repo-workflows-file_comparison-lens-b72fb6c1 = undoes staged

repo-workflows-file_comparison-lens-bfae8a90 = HEAD uses { $previous }; content-only unstaging keeps the staged move. Change the move itself in the normal patch.

repo-workflows-file_comparison-lens-d1d29085 = Working copy

repo-workflows-file_comparison-lens-daec459a = Staged

repo-workflows-file_comparison-matrix-50009ce1 = File

repo-workflows-file_comparison-matrix-6be36ca4 = Missing

repo-workflows-file_comparison-matrix-c81d97c3 = { $label }
    Pinned { $c-oid-1 }

repo-workflows-file_comparison-matrix-ebfcc1d5 = { $data-paths-len-1 } files · select another cell after the first

repo-workflows-file_comparison-migration-06869a99 = similarity unknown

repo-workflows-file_comparison-migration-208e67e7 = { $m-before-name-1 } → { $m-after-name-2 } · { $if-m-copy-copy-else-rename-3 } { $m-similarity-map-s-format-s-unwr-4 }

repo-workflows-file_comparison-migration-3064d79a = Rename

repo-workflows-file_comparison-migration-35255dde = Git detection warning (some moves may be unknown): { $warning }

repo-workflows-file_comparison-migration-41aec147 = { $data-moves-len-1 } renames/copies · { $data-other-changes-2 } other changes

repo-workflows-file_comparison-migration-a419e1f7 = No Git-detected renames or copies. Added/deleted paths are not assumed to be moves.

repo-workflows-file_comparison-migration-e21f935f = Copy

repo-workflows-file_comparison-open-file-comparison-a6cb5dc1 = Finish or close the editor first

repo-workflows-file_comparison-paths-28803ea3 = Resolve conflicts before analyzing moved code

repo-workflows-file_comparison-paths-58fb2d6a = Missing change status

repo-workflows-file_comparison-paths-59a504f9 = { $files-display-path-path-1 }: non-UTF8 path skipped

repo-workflows-file_comparison-paths-ae3de607 = Unexpected changed-file status

repo-workflows-file_comparison-paths-d4097e54 = More than 300 changed files. Compare a smaller range.

repo-workflows-file_comparison-paths-e3561990 = Incomplete changed-file list

repo-workflows-file_comparison-prepare-da3978aa = Invalid comparison sides

repo-workflows-file_comparison-read-0378d2b4 = Staged changes · HEAD → index

repo-workflows-file_comparison-read-259fd376 = HEAD changed during analysis. Refresh.

repo-workflows-file_comparison-read-3fbe0e2d = Unknown moved-code source

repo-workflows-file_comparison-read-431f5828 = Moved-code analysis cancelled or exceeded 40 seconds

repo-workflows-file_comparison-read-5e3dd301 = Commit { $oid-8-1 } · first parent

repo-workflows-file_comparison-read-6289d4ba = { $files-display-path-path-as-bytes-1 }: encrypted content excluded

repo-workflows-file_comparison-read-6adc8019 = File content changed during analysis. Refresh.

repo-workflows-file_comparison-read-a6767f12 = Changed patches exceed 6 MiB. Compare a smaller range.

repo-workflows-file_comparison-read-bf031806 = { $oid-8-1 } → working copy (tracked files)

repo-workflows-file_comparison-read-f42f63e5 = { $files-display-path-path-as-bytes-1 }: non-UTF8 patch skipped

repo-workflows-file_comparison-read-fc054b54 = Changed paths changed during analysis. Refresh.

repo-workflows-file_comparison-read-fcc615d7 = Working copy · index → working copy (tracked files)

repo-workflows-file_comparison-show-70bfd7fb = Wrap

repo-workflows-file_comparison-show-pair-06ebe02e = Byte preview (first512 bytes per side; full sizes shown above)

repo-workflows-file_comparison-show-pair-2f04c026 = No content edits: identical bytes. Names, presence and file modes remain separate.

repo-workflows-file_comparison-show-pair-7b68fe55 = After

repo-workflows-file_comparison-show-pair-98154a15 = Content edits only

repo-workflows-file_comparison-show-pair-9bb72500 = Before

repo-workflows-file_comparison-show-pair-e3ccb1fe = Before: { $pair-versions-0-label-1 } · After: { $pair-versions-1-label-2 }

repo-workflows-file_comparison-view-4699e093 = Open { $if-side-0-source-else-destinatio-1 } { $l-label-2 }

repo-workflows-file_comparison-view-746eb9db = { $data-locations-len-1 } blocks moved across files · shared M-markers pair both locations

repo-workflows-file_comparison-view-8bb733fc = M{ $i-1-1 } · Moved to…

repo-workflows-file_comparison-view-a47d33a8 = No cross-file moved blocks found (minimum 20 letters/digits). Same-file moves remain in the normal patch.

repo-workflows-file_comparison-view-b8b66b62 = unchanged / re-indented

repo-workflows-file_comparison-view-baa2bc90 = edited during move

repo-workflows-forge_link-deliver-5add4905 = Couldn't open the browser for { $link-url-1 }: { $e }

repo-workflows-forge_link-deliver-8d525e5f = Copied

repo-workflows-forge_link-deliver-aab63f85 = Open link

repo-workflows-forge_link-deliver-b19fb8d1 = Opened

repo-workflows-forge_link-deliver-c5e2849f = { $done } the link, but { $warning }: { $link-url-1 }

repo-workflows-forge_link-deliver-dbf362d4 = Copy link

repo-workflows-forge_link-deliver-f9eecf25 = Not a web address: { $link-url-1 }

repo-workflows-forge_link-items-44c5ece2 = Open { $what } on

repo-workflows-forge_link-items-55932e34 = On { $c-forge-1 }. The branch tracks no remote and there is no origin: pick one

repo-workflows-forge_link-items-6bb03074 = A permanent link (by the commit's hash), on { $one-remote-1 } ({ $one-place-2 })

repo-workflows-forge_link-items-78bef62f = Open { $what } on { $one-forge-1 }

repo-workflows-forge_link-items-a5232678 = Copy link to { $what }

repo-workflows-forge_link-items-ea6a597e = The branch's page on { $one-remote-1 } ({ $one-place-2 }): it follows the branch as it moves

repo-workflows-forks-allow-edits-6bb2bbb0 = Allow edits by maintainers

repo-workflows-forks-allow-edits-9e60d9b6 = Allow commits from members who can merge to the target branch

repo-workflows-forks-allow-edits-ddc30ba0 = Allow edits from maintainers

repo-workflows-forks-approve-runs-282082c7 = Couldn't approve #{ $number }'s workflow runs: { $e }

repo-workflows-forks-approve-runs-2f659b28 = Approving #{ $number }'s workflow runs…

repo-workflows-forks-approve-runs-a151ad19 = #{ $number }'s workflow runs are approved

repo-workflows-forks-approve-runs-a77eca22 = Approving runs the contributor's code on your CI: look through #{ $number }'s changes first, then choose Approve and run again

repo-workflows-forks-badges-42d0a9e0 = can edit

repo-workflows-forks-badges-84e3eb92 = awaiting approval

repo-workflows-forks-badges-aea3a983 = { $info-owner-1 } lets maintainers push to their branch { $info-branch-2 }: Check out to push to { $info-owner-3 } in its menu

repo-workflows-forks-badges-e3d8fbdd = { $info-owner-1 }'s first contribution to the repository

repo-workflows-forks-checkout-00c3592c = #{ $info-number-1 }'s branch name ({ $info-branch-2 }) can't be used as it is

repo-workflows-forks-checkout-13fc08d7 = #{ $info-number-1 }'s branch is in this repository: check it out as any branch

repo-workflows-forks-checkout-43a1fda6 = The forge didn't say where { $info-owner-1 }'s fork is

repo-workflows-forks-checkout-625f9e12 = { $info-owner-1 } doesn't let maintainers push to #{ $info-number-2 }'s branch

repo-workflows-forks-checkout-d005bf20 = { $info-branch-1 }, { $info-owner-2 }/{ $info-branch-3 } and pr-{ $info-number-4 } are all branches here already

repo-workflows-forks-checkout-fork-5ecb3bac = Where #{ $number } comes from isn't known yet

repo-workflows-forks-checkout-fork-abaaf4c3 = Check out #{ $number } to push to { $info-owner-1 }

repo-workflows-forks-count-082131a5 = unexpected count: { $e }

repo-workflows-forks-fork-items-2b48edeb = Check out { $info-owner-1 }'s branch to push to them first

repo-workflows-forks-fork-items-3450c709 = Rebase onto { $info-base-1 } and push to { $info-owner-2 }

repo-workflows-forks-fork-items-6c1751f8 = { $info-owner-1 } doesn't let maintainers push to { $info-branch-2 }: ask them to tick “{ $allow-edits-self-forge-3 }”

repo-workflows-forks-fork-items-7e44168f = Set up already: a push goes to { $info-owner-1 }'s { $info-branch-2 }

repo-workflows-forks-fork-items-9862a39c = The branch can't be checked out to push to: its repository is gone

repo-workflows-forks-fork-items-99b20173 = Approve and run ({ $waiting })…

repo-workflows-forks-fork-items-9e40421a = Check out { $b-name-1 } (pushes to { $info-owner-2 })

repo-workflows-forks-fork-items-a0b3b5ed = Fetches { $info-base-1 } and { $info-owner-2 }'s branch, rebases it onto { $info-base-3 }, then force-pushes it to them with a lease, after showing what it will do

repo-workflows-forks-fork-items-a33fed38 = Adds { $info-owner-1 }'s fork as a remote (unless one is it already), checks their branch { $info-branch-2 } out tracking it, and sets the branch's pushRemote so that Push goes back to them. Ctrl+Z undoes it

repo-workflows-forks-fork-items-afc89aab = Approves the workflow runs of its latest commit, which then run the contributor's code on your CI: look through its changes first

repo-workflows-forks-fork-items-bdee4479 = Its fork was deleted

repo-workflows-forks-fork-items-c8fd7141 = Check out to push to { $info-owner-1 }

repo-workflows-forks-forks-op-done-8287ad6b = { $plan-local-1 } is rebased here, but not pushed to { $plan-fork-2 }: Ctrl+Z puts it back as it was

repo-workflows-forks-forks-op-done-9c338e90 = The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it (or abort it)

repo-workflows-forks-offer-push-after-conflict-731ce7ed = Push

repo-workflows-forks-offer-push-after-conflict-dcc709d6 = The rebase is done. Force-pushes { $plan-local-1 } to { $plan-fork-2 }'s { $plan-branch-3 } with a lease: refused if they pushed there since it was fetched. Ctrl+Z puts their branch back as it was (and yours with the next Ctrl+Z).

repo-workflows-forks-plan-fork-rebase-1abac341 = This repository has no remote to rebase onto

repo-workflows-forks-plan-fork-rebase-a3647cfa = Rebase and push

repo-workflows-forks-plan-fork-rebase-ac71056e = Fetching #{ $number }'s branch and its base…

repo-workflows-forks-plan-rebase-6ccf1bc5 = { $local } already has every commit of { $upstream }/{ $info-base-1 }

repo-workflows-forks-plan-rebase-a0b078e0 = Check out #{ $info-number-1 }'s branch to push to { $info-owner-2 } first

repo-workflows-forks-plan-rebase-b597bd2f = Check out #{ $info-number-1 }'s branch to push to { $info-owner-2 } first: no remote is their fork

repo-workflows-forks-plan-rebase-e28bbaf9 = Commit or stash your changes first: this rebases the branch

repo-workflows-forks-plan-tidy-a7e148ae = Remove unused fork remotes

repo-workflows-forks-plan-tidy-adf73b14 = Couldn't read the remotes: { $e }

repo-workflows-forks-plan-tidy-b732e7ba = Removes the remotes gitcraft added for forks that no branch pulls from or pushes to any more: { $unused-join-1 }, with their remote-tracking branches. Ctrl+Z adds them back (fetch them for their branches).{ $kept }

repo-workflows-forks-plan-tidy-c3812fc4 = Remove

repo-workflows-forks-plan-tidy-da56e1d6 = No fork remote can go{ $kept }

repo-workflows-ghost-approval-preview-1426bc08 = { $files-len-1 } working files change

repo-workflows-ghost-approval-preview-675d236b = Discards uncommitted working changes

repo-workflows-ghost-approval-preview-70be46e5 = Leaving-commit list is truncated at 200

repo-workflows-ghost-approval-preview-e74a52f6 = Stops at step { $conflict-step-1 }: { $conflict-files-join-2 }

repo-workflows-ghost-current-7776bacf = HEAD is detached: check out a branch first

repo-workflows-ghost-delete-branch-26d05fbc = { $name } isn't merged into { $what }: Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch.

repo-workflows-ghost-delete-branch-a9c01edc = there is no branch { $name } here

repo-workflows-ghost-delete-branch-dde58e5a = its upstream

repo-workflows-ghost-delete-branch-fadd1b38 = the checked-out branch

repo-workflows-ghost-drop-commit-181a54e0 = merges came after the commit, which this would flatten

repo-workflows-ghost-drop-commit-3a19c0c2 = Uncommitted changes are stashed first and put back after.

repo-workflows-ghost-drop-commit-da2d4ead = the first commit has no parent to rebase from

repo-workflows-ghost-force-push-2d4460fd = As of the last fetch of { $up-remote-1 }: with the lease, the push is refused if someone pushed since.

repo-workflows-ghost-force-push-a9c01edc = there is no branch { $name } here

repo-workflows-ghost-force-push-c637c04f = { $up-tracking-name-1 } loses nothing: a plain push would do.

repo-workflows-ghost-force-push-f06fee94 = { $name } tracks no branch on a remote

repo-workflows-ghost-ghost-strip-a81f72de = nothing has been done

repo-workflows-ghost-ghost-strip-b69b3d5d = ↑ { $plural-above-new-commit-is-new-c-1 } above the rows in view.

repo-workflows-ghost-ghost-strip-c4a8b8f8 = Preview: { $title }

repo-workflows-ghost-ghost-strip-d0c20bfd = Working out what it would do…

repo-workflows-ghost-ghost-strip-f637d899 = Can't preview it. { $sentence-one-line-why-1 }

repo-workflows-ghost-listed-683768ab = and { $items-len-listed-1 } more

repo-workflows-ghost-lost-line-02717c8c = Leaving the remote's branch ({ $count }): { $names }

repo-workflows-ghost-lost-line-2cd6d050 = Replaced by new copies ({ $count }; { $originals } left on no branch): { $names }

repo-workflows-ghost-lost-line-50788b96 = more than { $o-lost-len-1 } commits

repo-workflows-ghost-lost-line-811bd7ca = Left on no branch ({ $count }): { $names }

repo-workflows-ghost-lost-line-8ccbdaa0 = Left on no branch ({ $count }, { $o-replaced-1 } of them replaced by new copies): { $names }

repo-workflows-ghost-outcome-lines-02e3b1f7 = No file of the working tree changes.

repo-workflows-ghost-outcome-lines-193731dd = The merge stops

repo-workflows-ghost-outcome-lines-4b54308f = Files of the working tree that change ({ $files-len-1 }): { $listed-files-f-format-f-status-f-2 }

repo-workflows-ghost-outcome-lines-7728bdcc = Stops at step { $c-step-1 } of { $c-of-2 } ({ $describe-commit-3 })

repo-workflows-ghost-outcome-lines-81deb64d = { $name } is made at { $shown-to-1 }

repo-workflows-ghost-outcome-lines-820df962 = Worked out with { $engine-describe-1 }

repo-workflows-ghost-outcome-lines-91af17db = { $name } is deleted

repo-workflows-ghost-outcome-lines-b8d8f166 = Drops { $describe-dropped-1 } — { $dropped-author-2 }

repo-workflows-ghost-outcome-lines-ce73844c = { $name } moves { $short-from-1 } → { $shown-to-2 }

repo-workflows-ghost-outcome-lines-dfa9e9b6 = { $plural-fetched-commit-commits-1 } from the remote

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

repo-workflows-ghost-title-20596011 = Undo tree: { $label }

repo-workflows-ghost-title-2d281f6b = Rebase { $current } onto { $label }

repo-workflows-ghost-title-62fc0c1a = Delete { $branch }

repo-workflows-ghost-title-ae583650 = Hard reset of { $current } to { $short-to-1 }

repo-workflows-ghost-title-b193a29c = Force push of { $branch }

repo-workflows-ghost-title-baaedb85 = { $mode-label-1 } into { $current }

repo-workflows-ghost-title-c58e3395 = Delete { $branch } here and on its remote

repo-workflows-ghost-title-fc4198eb = Drop commit { $short-hash-1 }

repo-workflows-guide--21dad442 = Tips turned off: Settings → General → Tips turns them back on

repo-workflows-guide--2ee88cd5 = Needs the first commit

repo-workflows-guide--b77d9539 = Set your name and email first (Settings → Profile, or git config user.name / user.email)

repo-workflows-guide--b7acfeab = Turn off tips and first-time hints (Settings → General → Tips turns them back on)

repo-workflows-guide-ask-empty-commit-348d9f75 = HEAD is detached: check out a branch to make the first commit on

repo-workflows-guide-confirm-dialog-82a9c46f = Commit

repo-workflows-guide-confirm-dialog-91b6a6f9 = Make an empty commit named “{ $EMPTY-COMMIT-MESSAGE }” on { $name }? It gives the repository a branch to publish and to start others from. You can undo it until it is published.

repo-workflows-guide-first-commit-step-02da0750 = Nothing to commit yet: files put in the repository's folder show up at the top of the graph by themselves. Or start with an empty commit.

repo-workflows-guide-first-commit-step-37bb706f = Your files are ready: write a summary and commit them.

repo-workflows-guide-first-commit-step-6221ab4e = A commit with no files, “{ $EMPTY-COMMIT-MESSAGE }”

repo-workflows-guide-first-commit-step-9416811c = Make an empty commit…

repo-workflows-guide-first-commit-step-cfa86831 = Write the message

repo-workflows-guide-first-commit-step-d0ee469e = Opens the changes in the right pane, at the commit summary

repo-workflows-guide-first-commit-step-e2f5a327 = Make the first commit

repo-workflows-guide-first-commit-step-ed251864 = Done.

repo-workflows-guide-first-steps-3c4e21be = Add a remote first

repo-workflows-guide-first-steps-76a2896d = Check out a branch first

repo-workflows-guide-first-steps-view-a5b4a2a3 = Hidden until this repository is opened again

repo-workflows-guide-first-steps-view-d4b85ef0 = Hide these steps

repo-workflows-guide-header-5cec6b07 = This repository has no commits yet

repo-workflows-guide-header-a9174a7c = Three steps to get it going:

repo-workflows-guide-header-bb24101f = Next steps for this repository

repo-workflows-guide-header-cd9905c0 = It is only on this machine so far:

repo-workflows-guide-hint-status-2c0ee188 = Close the hint

repo-workflows-guide-publish-step-2ce79937 = Push the branch to the remote and track it, so Push and Pull just work.

repo-workflows-guide-publish-step-859390eb = Publish

repo-workflows-guide-publish-step-93d6376b = Push { $b } to the remote and track it, so Push and Pull just work.

repo-workflows-guide-publish-step-ed251864 = Done.

repo-workflows-guide-remote-step-06906287 = Where it will be published, such as a new, empty repository on a forge.

repo-workflows-guide-remote-step-25a81e6a = Done: { $remotes-join-1 }.

repo-workflows-guide-remote-step-d20b3285 = Add a remote…

repo-workflows-guide-remote-step-ecd12a48 = Add a remote

repo-workflows-guide-start-first-commit-881f72cb = Nothing to commit yet: add or change files in the repository's folder

repo-workflows-guide-text-414ed52e = Hint: { $cmd }+click another commit to compare the two

repo-workflows-guide-text-aa7b18c5 = Hint: “Solo” in a branch's menu shows only that branch

repo-workflows-guide-text-c09ad9f4 = Hint: F7 or Alt+↓ goes to the next change, Shift+F7 or Alt+↑ to the previous one

repo-workflows-guide-text-e94f4321 = Hint: double-click a pill to check that branch out

repo-workflows-guide-tip-bar-3e9b3541 = Did you know?

repo-workflows-guide-tip-bar-c1567088 = Next tip

repo-workflows-guide-undo-refusal-6cb7f998 = { $name } is published: undoing the empty first commit would delete a branch the remote has. Commit on top of it instead.

repo-workflows-history_activity-activity-view-04b6c8fb = Apply period

repo-workflows-history_activity-activity-view-0e407d89 = Contributor statistics

repo-workflows-history_activity-activity-view-101c65f2 = { " " }· shallow clone (earlier history unavailable)

repo-workflows-history_activity-activity-view-126236b2 = All scanned dates

repo-workflows-history_activity-activity-view-1556feb2 = Author date

repo-workflows-history_activity-activity-view-21819769 = From

repo-workflows-history_activity-activity-view-4562ddd7 = Committer date

repo-workflows-history_activity-activity-view-4a7fda65 = Refresh activity

repo-workflows-history_activity-activity-view-4d167723 = Reached the 100,000 commit limit; counts are partial.

repo-workflows-history_activity-activity-view-60b300fd = Activity calendar

repo-workflows-history_activity-activity-view-6561da04 = Scanned dates: { $first } – { $last }

repo-workflows-history_activity-activity-view-7c0879db = Snapshot of local/remote branches, tags and HEAD; refresh after changes. Stash and app-only refs are excluded.

repo-workflows-history_activity-activity-view-7e33d4ef = Release to apply the selected period.

repo-workflows-history_activity-activity-view-8efbb1ca = Close activity

repo-workflows-history_activity-activity-view-956a51f6 = Group by

repo-workflows-history_activity-activity-view-a20fa1bb = Scan more history

repo-workflows-history_activity-activity-view-ab181f77 = History activity

repo-workflows-history_activity-activity-view-ac3896c6 = Days use the timezone recorded on each commit. Date ranges include both endpoints.

repo-workflows-history_activity-activity-view-afc6abac = Through

repo-workflows-history_activity-activity-view-baa24799 = No commits in this repository.

repo-workflows-history_activity-activity-view-cc4e9aaf = Cancel analysis

repo-workflows-history_activity-activity-view-d402be35 = Stopped at the 64 MiB analysis limit; counts cover only the scanned commits.

repo-workflows-history_activity-activity-view-e136531e = { $coverage } · { $history-commits-len-1 } unique commits · { $history-tips-2 } reference tips{ $if-history-shallow-shallow-clone-3 }

repo-workflows-history_activity-activity-view-e23552d7 = Analyzing history… { $view-progress-load-ordering-rela-1 } commits read

repo-workflows-history_activity-activity-view-e6f89512 = At the shallow boundary, files in the first available tree count as additions.

repo-workflows-history_activity-activity-view-fd008e90 = All reachable history

repo-workflows-history_activity-activity-view-fe26c97a = Partial history

repo-workflows-history_activity-authors-626a79db = Show all authors

repo-workflows-history_activity-authors-65845cf6 = No activity in this period.

repo-workflows-history_activity-authors-6cbebfa2 = Active days

repo-workflows-history_activity-authors-6e25b1da = Distinct paths touched, active days and first–last activity within the selected period.

repo-workflows-history_activity-authors-76cb89fa = Authors follow Git’s current .mailmap. Renames touch the old and new paths; merge changes are compared with the first parent.

repo-workflows-history_activity-authors-ce199c9e = Files touched

repo-workflows-history_activity-authors-d09648c6 = Commits

repo-workflows-history_activity-authors-d95082a2 = Author

repo-workflows-history_activity-authors-e707bedf = Active period

repo-workflows-history_activity-calendar-370eebba = { $month-year-1 }-{ $m }
    { $n } commits

repo-workflows-history_activity-calendar-58339f45 = Wed

repo-workflows-history_activity-calendar-66dab40c = Fri

repo-workflows-history_activity-calendar-6a276950 = Previous month

repo-workflows-history_activity-calendar-74e53211 = Next month

repo-workflows-history_activity-calendar-7da11212 = Thu

repo-workflows-history_activity-calendar-91e33e3b = Previous year

repo-workflows-history_activity-calendar-a21a5dc6 = Next year

repo-workflows-history_activity-calendar-a54ddac5 = Browse months

repo-workflows-history_activity-calendar-c60e09cb = { $day }
    { $n } commits

repo-workflows-history_activity-calendar-d1eb39b0 = Tue

repo-workflows-history_activity-calendar-db18f17f = Sun

repo-workflows-history_activity-calendar-f40d7f51 = Mon

repo-workflows-history_activity-calendar-fdeb71b5 = Sat

repo-workflows-history_activity-cancel-ddfe1c43 = Analysis cancelled. Refresh to try again.

repo-workflows-history_activity-commits-640e56ec = { $selected-len-1 } commits in selected period

repo-workflows-history_activity-commits-f0d98513 = Open commit detail

repo-workflows-history_activity-history-activity-window-0ad38473 = Repository history activity

repo-workflows-history_activity-open-activity-a6cb5dc1 = Finish or close the editor first

repo-workflows-history_activity-page-1208ec01 = Previous page

repo-workflows-history_activity-page-c08ac736 = Next page

repo-workflows-history_activity-page-dbaceb71 = Page { $page-1-1 } of { $len-div-ceil-100-max-1-2 }

repo-workflows-history_activity-requested-range-25f6acd0 = The start date must be on or before the end date

repo-workflows-history_activity-requested-range-4d18caf5 = Use dates such as 2026-11-07

repo-workflows-hotfix-cancel-19766ed6 = Cancel

repo-workflows-hotfix-contents-08251562 = Signed

repo-workflows-hotfix-contents-0e86de5c = This git can't tell beforehand whether the merges conflict (2.38 or later can): if one does, it stops there

repo-workflows-hotfix-contents-0f2dabe6 = Another operation is running: wait for it

repo-workflows-hotfix-contents-1503916a = Tag

repo-workflows-hotfix-contents-296ee3d7 = Finish hotfix

repo-workflows-hotfix-contents-3b7c3548 = The tag's notes (Markdown)

repo-workflows-hotfix-contents-3d5a5069 = WHAT IT WILL DO

repo-workflows-hotfix-contents-89b9dd15 = Ctrl+Z puts the branches and the tag back here as they were; what was pushed stays.

repo-workflows-hotfix-contents-96176159 = Delete { $plan-hotfix-name-1 } here

repo-workflows-hotfix-contents-9d164a95 = No development branch here (develop, or git-flow's)

repo-workflows-hotfix-contents-a2e52e9b = Push to { $r }

repo-workflows-hotfix-contents-ba6c71e9 = And into { $d-name-1 }

repo-workflows-hotfix-contents-c2cdf43f = git tag -s, with your signing key; otherwise annotated

repo-workflows-hotfix-contents-c5ecd20d = Push (this repository has no remote)

repo-workflows-hotfix-contents-dd94532f = { " " }since { $t }

repo-workflows-hotfix-contents-f3077f1d = { $plan-hotfix-name-1 } · { $plan-changes-len-2 } change{ $if-plan-changes-len-1-else-s-3 }{ $plan-base-tag-as-deref-map-t-for-4 }

repo-workflows-hotfix-contents-f5346a60 = Merged into { $plan-main-name-1 }

repo-workflows-hotfix-hotfix-act-06b55025 = { $tag } isn't a version to start a hotfix from

repo-workflows-hotfix-hotfix-op-done-5b11af18 = { " " }— Ctrl+Z puts back what it did

repo-workflows-hotfix-hotfix-op-done-fbdef87e = { $run-label-1 } (stopped part-way)

repo-workflows-hotfix-plan-8f734f01 = { $branch } has no commits of its own to release

repo-workflows-hotfix-plan-97906966 = There is no main branch here (main, master, or git-flow's)

repo-workflows-hotfix-plan-a798cdeb = Check out the hotfix branch ({ $names-prefix-1 }…) first

repo-workflows-hotfix-plan-e373acd0 = Commit or stash your changes first: finishing checks other branches out

repo-workflows-hotfix-plan-eeb1f496 = { $t-name-1 } is checked out in another worktree: check something else out there first

repo-workflows-hotfix-preview-4aeab9fa = Deletes { $h-name-1 } here

repo-workflows-hotfix-preview-523cd105 = Makes the { $if-c-sign-signed-else-annotated-1 } tag { $c-tag-trim-2 } on that merge into { $self-main-name-3 }

repo-workflows-hotfix-preview-7df6bedd = Then pushes { $names-join-and-1 } and the tag to { $remote }, all or nothing (git push --atomic)

repo-workflows-hotfix-preview-ab279563 = Merges { $h-name-1 } ({ $crate-repo-short-h-head-2 }) into { $t-name-3 } (git merge --no-ff)

repo-workflows-hotfix-preview-e3ee915a = { " " }and{ " " }

repo-workflows-hotfix-problem-0225cdbc = Merging it into { $into } would stop on conflicts in { $files-join-1 }: merge it by hand{ $or }

repo-workflows-hotfix-problem-a56c91b5 = , or untick it

repo-workflows-issue_work-card-1887e67c = as first read

repo-workflows-issue_work-card-1c245f79 = Hide { $what }

repo-workflows-issue_work-card-1e7fbd27 = Read the issue again: the card shows it as it was { $when }

repo-workflows-issue_work-card-53611908 = on { $link-read-1 }

repo-workflows-issue_work-card-57fa7fe2 = No acceptance criteria are recorded for this issue.

repo-workflows-issue_work-card-5fa29511 = Show { $what }

repo-workflows-issue_work-card-6d2189c3 = when work on it started

repo-workflows-issue_work-card-7fe3e1e0 = Open the issue in the browser

repo-workflows-issue_work-card-9b1e287d = as of { $link-read-1 }

repo-workflows-issue_work-card-b9210e97 = The issue says nothing more than its title.

repo-workflows-issue_work-card-fb6318e9 = Forget which issue this branch is for

repo-workflows-issue_work-choose-104ab921 = Work

repo-workflows-issue_work-choose-52656e81 = Branch

repo-workflows-issue_work-choose-561b5212 = The first { $ISSUES-MAX } came: an issue that isn't among them is found on the forge's page.

repo-workflows-issue_work-choose-723dfd05 = No issue matches the filter.

repo-workflows-issue_work-choose-7dce44e8 = Filter by number, title or label

repo-workflows-issue_work-choose-aa3b1760 = Here

repo-workflows-issue_work-choose-e8f4aaee = In a new worktree

repo-workflows-issue_work-issue-plan-0dd9c97f = { $branch } is the default branch: the work on an issue goes on a branch of its own

repo-workflows-issue_work-issue-plan-1f8c993f = { $remote } is there: a branch tracking it, { $where- }.

repo-workflows-issue_work-issue-plan-2bd468f0 = the repository has no commits yet: commit something to start work from

repo-workflows-issue_work-issue-plan-38012887 = A new branch from { $said }, { $where- }.

repo-workflows-issue_work-issue-plan-8dbd7c09 = { $branch } is for { $other-key-1 } already: starting makes it the branch of { $key } instead

repo-workflows-issue_work-issue-plan-952a7458 = { $branch } is checked out here already: it's linked to { $key }.

repo-workflows-issue_work-issue-plan-ae567fc0 = in a new worktree, { $folder-open-1 }

repo-workflows-issue_work-issue-plan-b341ef66 = { $branch } is checked out in { $folder-open-1 } already: it's linked to { $key }, and that worktree opens.

repo-workflows-issue_work-issue-plan-c9fafe01 = The working tree has changes: they go along to { $branch }, unless they'd be overwritten (then nothing is done)

repo-workflows-issue_work-issue-plan-cf90f159 = { $branch } exists already: work goes on on it as it is, { $where- }.

repo-workflows-issue_work-issue-plan-dbeefca3 = checked out here

repo-workflows-issue_work-issue-row-91765782 = Issue { $issue-number-1 }: { $issue-title-2 }

repo-workflows-issue_work-issue-start-point-212218a3 = { $tracking }, fetched first

repo-workflows-issue_work-issue-start-point-3c20246c = HEAD ({ $super-short-self-refs-head-1 }): there's no { $name } here or on the remote to start from

repo-workflows-issue_work-issue-start-point-497395b3 = { $name } (there is no remote)

repo-workflows-issue_work-issue-start-point-ceee14ee = { $tracking } (the remote's default branch), fetched first

repo-workflows-issue_work-issue-start-point-d844f1e9 = { $name } as it is here ({ $remote } has no { $name })

repo-workflows-issue_work-name-problem-4b6fdeb4 = The branch needs a name

repo-workflows-issue_work-name-problem-4be6bde2 = A branch's name can't have { $c } in it

repo-workflows-issue_work-name-problem-5e1d1dfc = A branch's name can't start with -

repo-workflows-issue_work-name-problem-64d737b0 = No part of a branch's name (between /) can end with .lock

repo-workflows-issue_work-name-problem-72447597 = A branch's name can't have { $two } in it

repo-workflows-issue_work-name-problem-7b27dd13 = { $name } is a name git keeps for itself

repo-workflows-issue_work-name-problem-8e7a7abd = A branch's name can't end with .

repo-workflows-issue_work-name-problem-b19e092f = No part of a branch's name (between /) can start with .

repo-workflows-issue_work-name-problem-b9131e75 = A branch's name can't have control characters in it

repo-workflows-issue_work-name-problem-e2570ee3 = A branch's name can't start or end with /, nor have // in it

repo-workflows-issue_work-open-issue-picker-2a41bd2f = No repository forge detected. Select Jira or Linear above, or configure the repository remote

repo-workflows-issue_work-open-issue-picker-3a89bbe3 = Repository forge

repo-workflows-issue_work-refresh-issue-0c6fd7a6 = The repository forge is unavailable

repo-workflows-issue_work-refresh-issue-8336fbfe = { $label } failed: { $e }

repo-workflows-issue_work-refresh-issue-a0e9ad2a = { $label }: wait for { $busy }

repo-workflows-issue_work-show-19766ed6 = Cancel

repo-workflows-issue_work-show-3a89bbe3 = Repository forge

repo-workflows-issue_work-show-6a7da0d2 = Start work

repo-workflows-issue_work-show-8bd32aa3 = The open issues assigned to you on { $picker-forge-1 }

repo-workflows-issue_work-show-b32a9827 = Asking { $picker-forge-1 }…

repo-workflows-issue_work-show-b37705f6 = Wait for { $busy } to finish first

repo-workflows-issue_work-show-b6db3846 = Profile changed. Reload this source to start work.

repo-workflows-issue_work-show-d3965ab6 = No open issue of this repository is assigned to you.

repo-workflows-issue_work-show-d8b8392e = Try again

repo-workflows-issue_work-show-fd3baaea = Start work from an issue

repo-workflows-issue_work-start-issue-563140a4 = Tracker account changed; reload the issue source

repo-workflows-issue_work-start-issue-c14dd822 = Start work on { $issue-key-1 }: { $why }

repo-workflows-issue_work-start-issue-e2b80c1b = Start work on { $issue-key-1 }: wait for { $busy }

repo-workflows-issue_work-start-ops-4c405286 = couldn't fetch { $remote }, so it starts from { $remote }/{ $base } as last fetched

repo-workflows-issue_work-take-issue-read-bbcbbb59 = Couldn't read the branches' issues: { $e }

repo-workflows-issue_work-unlink-issue-6df1900e = Forget the issue of { $branch } failed: { $e }

repo-workflows-issue_work-what-3e69f7c1 = the task list

repo-workflows-issue_work-what-68e64f0f = the acceptance criteria

repo-workflows-issue_work-what-734b09bc = the description

repo-workflows-lfs_locks-caught-ff2cd16f = Internal error: { $said }

repo-workflows-lfs_locks-filters-53518a4c = Filter by path or owner

repo-workflows-lfs_locks-filters-5da4f78e = Only mine

repo-workflows-lfs_locks-header-2e532790 = Read the locks again

repo-workflows-lfs_locks-header-961b2a04 = Close (Esc)

repo-workflows-lfs_locks-header-b815cb09 = LFS locks

repo-workflows-lfs_locks-list-04f767bd = { $a } and { $b }

repo-workflows-lfs_locks-list-e6b59dc4 = { $a }, { $b } and { $rest-len-1 } more

repo-workflows-lfs_locks-lock-banners-03d23d29 = LFS locks…

repo-workflows-lfs_locks-lock-banners-11ae36a8 = { $if-one-owner-is-empty-someone-el-1 } holds the lock on { $one-path-2 }: your push of it will be refused until the lock is given back.

repo-workflows-lfs_locks-lock-banners-2737026b = Locked by someone else: { $list-names-1 }. Your push of them will be refused until the locks are given back.

repo-workflows-lfs_locks-lock-banners-3233cb04 = Someone else

repo-workflows-lfs_locks-lock-banners-379e383e = Unlock all

repo-workflows-lfs_locks-lock-banners-4ac709aa = Unlock

repo-workflows-lfs_locks-lock-banners-4caac7c4 = You pushed { $many-len-1 } files and still hold their locks: give them back so others can change them?

repo-workflows-lfs_locks-lock-banners-4fa40890 = You pushed { $one-path-1 } and still hold its lock: give it back so others can change it?

repo-workflows-lfs_locks-lock-banners-6df5e70f = Keep them locked

repo-workflows-lfs_locks-lock-banners-82b7be09 = { $one } can be locked and nobody holds it. Lock it so nobody else pushes changes to it while you work.

repo-workflows-lfs_locks-lock-banners-942087cc = Retry

repo-workflows-lfs_locks-lock-banners-96ed4625 = { $many-len-1 } changed files can be locked and nobody holds them. Lock them so nobody else pushes changes to them while you work.

repo-workflows-lfs_locks-lock-banners-ac5bb7e1 = Lock all

repo-workflows-lfs_locks-lock-banners-bb83044b = Keep it locked

repo-workflows-lfs_locks-lock-banners-db44b8db = Lock

repo-workflows-lfs_locks-lock-banners-f8368582 = Couldn't read the LFS locks, so who holds your changed files isn't known.

repo-workflows-lfs_locks-lock-job-35b33376 = { $label }: done. Commit .gitattributes to share it.

repo-workflows-lfs_locks-lock-job-36f5baee = { $label }: done

repo-workflows-lfs_locks-lock-job-8336fbfe = { $label } failed: { $e }

repo-workflows-lfs_locks-lock-label-398e9b81 = Locked by { $who } { $git-relative-time-at-now-1 }

repo-workflows-lfs_locks-lock-label-6329feb6 = Locked by { $who }

repo-workflows-lfs_locks-lock-list-1a9f2707 = None of the locks is yours.

repo-workflows-lfs_locks-lock-list-3ef08b56 = No lock matches the filter.

repo-workflows-lfs_locks-lock-list-5619b99f = Nobody holds a lock.

repo-workflows-lfs_locks-lock-list-e1890cf8 = Reading the locks…

repo-workflows-lfs_locks-lock-menu-items-03d23d29 = LFS locks…

repo-workflows-lfs_locks-lock-menu-items-052a679c = Give the LFS lock back so others can push changes to the file

repo-workflows-lfs_locks-lock-menu-items-0c28d273 = The server can't say whose lock this is: unlocking fails if it isn't yours

repo-workflows-lfs_locks-lock-menu-items-3f09bd22 = Take the file's LFS lock: nobody else can push changes to it

repo-workflows-lfs_locks-lock-menu-items-4ac709aa = Unlock

repo-workflows-lfs_locks-lock-menu-items-6526301c = git-lfs won't give back the lock of a file with changes: commit them first

repo-workflows-lfs_locks-lock-menu-items-b8fc75d5 = An LFS lock operation is running

repo-workflows-lfs_locks-lock-menu-items-bafa9d65 = Someone else holds this file’s lock

repo-workflows-lfs_locks-lock-menu-items-bea7de48 = Commit the file’s changes before unlocking it

repo-workflows-lfs_locks-lock-menu-items-db44b8db = Lock

repo-workflows-lfs_locks-locks-read-05347473 = Could not read the LFS locks: { $e }

repo-workflows-lfs_locks-patterns-119fe07d = git lfs track --lockable: keeps them in LFS, written to .gitattributes

repo-workflows-lfs_locks-patterns-6105d5e3 = Make lockable

repo-workflows-lfs_locks-patterns-abf43528 = Lockable files

repo-workflows-lfs_locks-patterns-ddfb1d6f = Files matching these patterns can be locked; git-lfs keeps them read-only until you do. Commit .gitattributes to share a new pattern.

repo-workflows-lfs_locks-patterns-ed7c75cc = No pattern in the root .gitattributes is lockable.

repo-workflows-lfs_locks-unlock-buttons-19766ed6 = Cancel

repo-workflows-lfs_locks-unlock-buttons-24110cc4 = Unlock { $n }

repo-workflows-lfs_locks-unlock-buttons-2978e628 = its owner loses the lock

repo-workflows-lfs_locks-unlock-buttons-4ac709aa = Unlock

repo-workflows-lfs_locks-unlock-buttons-6021bccb = Force unlock { $n }…

repo-workflows-lfs_locks-unlock-buttons-62509394 = the owners lose their locks

repo-workflows-lfs_locks-unlock-buttons-97edf7d6 = its owner

repo-workflows-lfs_locks-unlock-buttons-9971cec2 = Take { $n } locks from their owners

repo-workflows-lfs_locks-unlock-buttons-b57b0be8 = Take the lock from { $owner }

repo-workflows-lfs_locks-unlock-buttons-c214395e = Takes the lock from whoever holds it. Only admins may do this; the owner isn't told, and their unpushed changes to the file may clash with someone else's.

repo-workflows-lfs_locks-unlock-buttons-ccf3d9ea = Pick all

repo-workflows-lfs_locks-unlock-buttons-e213bc66 = Click again to force: { $who }.

repo-workflows-lfs_locks-unlock-buttons-e2d5673c = Pick none

repo-workflows-lfs_locks-unlock-buttons-f16646da = Force unlock…

repo-workflows-lfs_storage-caught-ff2cd16f = Internal error: { $said }

repo-workflows-lfs_storage-checkout-1716d186 = Only pointers ({ $missing-len-1 })

repo-workflows-lfs_storage-checkout-4cd9a422 = git lfs pull of each of them, whatever the folders below say

repo-workflows-lfs_storage-checkout-5f0a6ba0 = Download all ({ $paths-len-1 })

repo-workflows-lfs_storage-checkout-683768ab = and { $missing-len-listed-1 } more

repo-workflows-lfs_storage-checkout-87bcf309 = git lfs pull, as far as the folders below allow ({ $e })

repo-workflows-lfs_storage-checkout-a5d7a59b = { $n } files in LFS, all downloaded.

repo-workflows-lfs_storage-checkout-b627e471 = No file of the checkout is kept in LFS.

repo-workflows-lfs_storage-checkout-d6eafe82 = Download

repo-workflows-lfs_storage-checkout-f4b00836 = Listing the LFS files…

repo-workflows-lfs_storage-checkout-f5a54014 = 1 file in LFS, downloaded.

repo-workflows-lfs_storage-download-action-43432746 = “{ $bad }” starts or ends with a space, which git-lfs drops: use LFS: Pull to download it

repo-workflows-lfs_storage-download-action-e9f327fc = { $bad } has a comma in its name, which git-lfs reads as two patterns: use LFS: Pull to download it

repo-workflows-lfs_storage-fetch-only-3f558a4c = LFS now fetches only { $names-join-1 }

repo-workflows-lfs_storage-fetch-only-e3be5d1f = Could not set what LFS fetches: { $e }

repo-workflows-lfs_storage-filters-49e74a0b = Instead of what the repository's .lfsconfig sets: { $names-join-1 }

repo-workflows-lfs_storage-filters-72d4ac36 = Could not read what LFS fetches: { $e }

repo-workflows-lfs_storage-filters-8758c8bb = Only these folders

repo-workflows-lfs_storage-filters-9777b8b2 = git-lfs downloads only the folders included (every one when none is), leaving out those excluded, on fetch, pull and checkout. Set for this repository only.

repo-workflows-lfs_storage-filters-9fd728c6 = Add

repo-workflows-lfs_storage-filters-c621d797 = The repository's .lfsconfig sets: { $names-join-1 }

repo-workflows-lfs_storage-filters-cd8bf912 = Never these folders

repo-workflows-lfs_storage-filters-dc013b99 = Take { $name } out

repo-workflows-lfs_storage-header-59ef044f = LFS storage

repo-workflows-lfs_storage-header-961b2a04 = Close (Esc)

repo-workflows-lfs_storage-header-d8cb92fa = Read again

repo-workflows-lfs_storage-list-lfs-files-8d1bad49 = Could not list the LFS files: { $e }

repo-workflows-lfs_storage-pointer-mark-efdc14b1 = Only its LFS pointer is here (not downloaded). Right-click → LFS: Download.

repo-workflows-lfs_storage-prune-19766ed6 = Cancel

repo-workflows-lfs_storage-prune-30967fda = 1 object

repo-workflows-lfs_storage-prune-407859ca = Click again to delete { $objects }

repo-workflows-lfs_storage-prune-5fc3fdf9 = Delete { $objects } ({ $human-size-plan-bytes-1 })

repo-workflows-lfs_storage-prune-659493e2 = Nothing to prune.

repo-workflows-lfs_storage-prune-8bfaae7c = Pruning deletes local LFS objects that no recent commit, the checkout or unpushed work needs; they can be downloaded again as long as the remote still has them.

repo-workflows-lfs_storage-prune-8f8978b1 = { $plan-objects-1 } objects

repo-workflows-lfs_storage-prune-ebe86463 = Check what pruning frees

repo-workflows-lfs_storage-prune-f0bcc399 = Working out what pruning frees…

repo-workflows-lfs_storage-set-filter-b41c110a = Could not save it: { $e }

repo-workflows-lfs_storage-storage-menu-items-1b8b17ee = git lfs track: writes the pattern to .gitattributes. Files already committed stay ordinary git files until changed and staged again; commit .gitattributes to share it.

repo-workflows-lfs_storage-storage-menu-items-1de3e10c = LFS: Download

repo-workflows-lfs_storage-storage-menu-items-31a0cb12 = Track { $pattern } in LFS

repo-workflows-lfs_storage-storage-menu-items-4f441605 = Track { $path } in LFS

repo-workflows-lfs_storage-storage-menu-items-5ed965af = LFS: Fetch only { $dir }

repo-workflows-lfs_storage-storage-menu-items-e2cb4df2 = Download this file's LFS object and check it out

repo-workflows-lfs_storage-storage-menu-items-ea226348 = From now on git-lfs downloads only this folder (and any others set in LFS: Storage…) on fetch, pull and checkout; other files stay pointers. Set in this repository's config.

repo-workflows-lfs_storage-storage-panel-6ce01b6d = Old objects

repo-workflows-lfs_storage-storage-panel-a99e2b3d = This checkout

repo-workflows-lfs_storage-storage-panel-b05f298a = What LFS fetches

repo-workflows-lfs_storage-usage-18b3c0d1 = No LFS objects on disk yet

repo-workflows-lfs_storage-usage-9aea26d2 = { $human-size-u-bytes-1 } in 1 LFS object on disk

repo-workflows-lfs_storage-usage-a5b8a0cd = { $human-size-u-bytes-1 } in { $n } LFS objects on disk

repo-workflows-lfs_storage-usage-aca1a392 = Measuring the LFS objects…

repo-workflows-lfs_storage-usage-bc295532 = Could not measure the LFS objects: { $e }

repo-workflows-line_dossier-line-dossier-view-82a9c46f = Commit

repo-workflows-owners-contents-3203b0df = Couldn't list the repository's files: { $e }

repo-workflows-owners-contents-343bb384 = 1 rule

repo-workflows-owners-contents-4adeeb91 = Sections: { $sections-join-1 }

repo-workflows-owners-contents-4b99390e = In your editor

repo-workflows-owners-contents-4d726f4a = { $n } rules

repo-workflows-owners-contents-64feb239 = LINES THE FORGE READS DIFFERENTLY ({ $file-problems-len-1 })

repo-workflows-owners-contents-7d9eb7ac = Close

repo-workflows-owners-contents-8f8353f4 = line { $p-line-1 }: { $p-text-2 }

repo-workflows-owners-contents-96978c8c = { $file-path-1 } at HEAD, read as { $file-flavor-name-2 } reads it · { $rules }

repo-workflows-owners-contents-bd5f6b1d = Open the file

repo-workflows-owners-contents-bd73a6b1 = None: the forge reads every line as written.

repo-workflows-owners-contents-c72a165d = Couldn't read it again lately ({ $e }): this is the one read before.

repo-workflows-owners-contents-e1e62224 = Check CODEOWNERS

repo-workflows-owners-owners-act-9cd4150e = This repository has no CODEOWNERS file

repo-workflows-owners-owners-act-b11dc59d = checking it failed

repo-workflows-owners-owners-summary-db29b096 = From { $path } at HEAD ({ $rules-count-1 })

repo-workflows-owners-read-owners-cf60f83c = reading it failed

repo-workflows-owners-results-26e406d1 = None: every file has an owner.

repo-workflows-owners-results-683768ab = and { $lint-unowned-len-listed-1 } more

repo-workflows-owners-results-85d177f5 = PATTERNS THAT MATCH NO FILE ({ $lint-unmatched-len-1 })

repo-workflows-owners-results-9c0433a9 = None: every pattern matches a file.

repo-workflows-owners-results-a6551c12 = line { $n }: { $pattern }

repo-workflows-owners-results-a92df9d7 = FILES WITHOUT AN OWNER ({ $lint-unowned-len-1 } of { $lint-files-2 })

repo-workflows-owners-reviewers-for-d68c1cfb = { $base } isn't a branch here

repo-workflows-owners-reviewers-ui-796e0df8 = Ask for reviews (from CODEOWNERS) once it isn't a draft

repo-workflows-owners-reviewers-ui-7a8a3ca3 = A team: the forge asks its members

repo-workflows-owners-reviewers-ui-d8e1731e = An account

repo-workflows-owners-reviewers-ui-da35c68e = Ask for reviews (from CODEOWNERS)

repo-workflows-owners-rules-343bb384 = 1 rule

repo-workflows-owners-rules-4d726f4a = { $n } rules

repo-workflows-owners-summary-text-3ff108bf = { $unowned } files without an owner

repo-workflows-owners-summary-text-4339bb43 = No owner

repo-workflows-owners-summary-text-6a5cbe9f = 1 file without an owner

repo-workflows-owners-summary-text-9655d0de = No file here has an owner

repo-workflows-owners-summary-text-a198627d = Needs { $codeowners-and-list-short-names--1 }

repo-workflows-package-buttons-0e916101 = Refresh

repo-workflows-package-buttons-7d9eb7ac = Close

repo-workflows-package-buttons-8a24ec7c = Lists the packages and their files again, after a change to them

repo-workflows-package-contents-59de121d = Package

repo-workflows-package-contents-89bee303 = Couldn't list the package: { $e }

repo-workflows-package-contents-8ed27415 = Nothing here is published: package.json is private, or every crate has publish = false.

repo-workflows-package-contents-a2e9d64e = From { $pkg-ecosystem-tool-1 }, run in { $if-pkg-dir-is-empty-the-reposito-2 }

repo-workflows-package-contents-b32a9827 = Asking { $pkg-ecosystem-tool-1 }…

repo-workflows-package-contents-b60559a9 = What a package will publish

repo-workflows-package-contents-d5728cc3 = the repository's root

repo-workflows-package-contents-e237505e = Couldn't find the packages: { $e }

repo-workflows-package-more-683768ab = and { $len-shown-1 } more

repo-workflows-package-notes-adea78e6 = None found.

repo-workflows-package-report-ui-47ec594b = In { $p-version-1 }, not in this one

repo-workflows-package-report-ui-6c125757 = New since { $p-version-1 }

repo-workflows-package-report-ui-74f3a93b = Couldn't compare: { $e }

repo-workflows-package-report-ui-b2667be4 = COMPARED WITH { $registry-to-uppercase-1 }

repo-workflows-package-report-ui-b413d4db = Asking { $registry }…

repo-workflows-package-report-ui-b47c6e02 = Never published on { $registry }.

repo-workflows-package-report-ui-d4826304 = FILES ({ $report-files-len-1 })

repo-workflows-package-report-ui-ef333981 = Same files as { $p-version-1 } on { $registry }.

repo-workflows-package-report-ui-fa5b38db = USUALLY SHIPPED, LEFT OUT

repo-workflows-package-report-ui-fefc6039 = LOOKS SHIPPED BY ACCIDENT

repo-workflows-parallel_bisect-hash-link-63633913 = Show this commit

repo-workflows-parallel_bisect-parallel-buttons-19766ed6 = Cancel

repo-workflows-parallel_bisect-parallel-buttons-7d9eb7ac = Close

repo-workflows-parallel_bisect-parallel-buttons-e5e3ecc5 = Wait for the operation going on to finish

repo-workflows-parallel_bisect-parallel-link-63633913 = Show this commit

repo-workflows-parallel_bisect-parallel-outcome-683768ab = and { $hashes-len-max-named-1 } more

repo-workflows-parallel_bisect-parallel-outcome-8b3812c0 = First bad commit:

repo-workflows-pr_review-verdict-buttons-eb1a70e3 = Discard

repo-workflows-pr_view-dismiss-box-19766ed6 = Cancel

repo-workflows-pr_view-entry-header-48845bff = Dismiss

repo-workflows-push-dismiss-48845bff = Dismiss

repo-workflows-push-rejected-683768ab = and { $r-behind-named-1 } more

repo-workflows-reflog-recover-items-4c49fdb6 = Cherry-pick

repo-workflows-reflog-reflog-menu-items-916e34d4 = Show in graph

repo-workflows-reflog-scanning-19766ed6 = Cancel

repo-workflows-rehearse-rehearse-window-cae7d57b = Stop

repo-workflows-release-close-button-7d9eb7ac = Close

repo-workflows-repository_tools-act-08cd49af = Close isolated proposal

repo-workflows-repository_tools-act-27cc6eb8 = Subtree preparation used a previous profile/sign-in; prepare again

repo-workflows-repository_tools-act-346c061a = Undo accepted isolated replay

repo-workflows-repository_tools-act-494336ed = Publish exact subtree split

repo-workflows-repository_tools-act-519202af = Outbox preparation used a previous profile/sign-in; prepare again

repo-workflows-repository_tools-act-551afaa9 = Unshelve one changelist

repo-workflows-repository_tools-act-5bd57c48 = Replay in isolated worktrees

repo-workflows-repository_tools-act-63403bc6 = Undo virtual branch operation

repo-workflows-repository_tools-act-65d162a8 = Continue isolated replay

repo-workflows-repository_tools-act-6dc9a943 = Choose one saved turn to preview its inverse

repo-workflows-repository_tools-act-7f694b62 = Remove outbox entry

repo-workflows-repository_tools-act-88c16034 = Subtree operation

repo-workflows-repository_tools-act-901dccbe = Save virtual branch ownership

repo-workflows-repository_tools-act-a3cbb206 = Save changelist

repo-workflows-repository_tools-act-b4e12052 = Turn reverted. Staged contents retained. Undo from Earlier versions: { $id-draft-reference-1 }. { $id-evidence-2 }

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

repo-workflows-repository_tools-act-e3e83f28 = Source branch disappeared from the reviewed graph

repo-workflows-repository_tools-act-fc328635 = Revert one saved turn

repo-workflows-repository_tools-deferred-badge-8daa08a5 = Conflict waiting: { $p }

repo-workflows-repository_tools-deferred-badge-a5eafc03 = Isolated rewrite; original branch retained until guarded acceptance

repo-workflows-repository_tools-dependency-job-57895189 = Check submodule publication dependencies

repo-workflows-repository_tools-dependency-job-80d8056a = Dependency operation completed

repo-workflows-repository_tools-dependency-job-96731680 = Parent publication stopped; inspect per-repository outcomes

repo-workflows-repository_tools-dependency-job-a7b1dc6d = Dependencies finished, but the parent repository, profile or sign-in changed. Prepare its push again.

repo-workflows-repository_tools-dependency-job-f48f9c31 = Publish dependencies before parent review

repo-workflows-repository_tools-dependency-job-f7c1dc97 = Dependency check found blockers; inspect each submodule

repo-workflows-repository_tools-force-01549c1f = Force push to the push route

repo-workflows-repository_tools-force-15d2a4c4 = The destination is currently at { $p-remote-commit-1 }. This can remove collaborators' commits. The exact URL and expected destination commit are pinned, and a lease rejects concurrent pushes.

repo-workflows-repository_tools-force-1d53000c = I reviewed the destination and accept replacing its history

repo-workflows-repository_tools-force-884b8ad2 = Replace { $p-destination-1 } on { $p-remote-2 } ({ $crate-git-redact-p-url-none-3 }) with { $p-branch-4 } at { $p-commit-5 }?

repo-workflows-repository_tools-force-b7f756d6 = Force push with explicit lease

repo-workflows-repository_tools-label-137ebba9 = { $direction } { $route-remote-1 }/{ $route-branch-2 } · { $counts }

repo-workflows-repository_tools-label-2d5fbffb = counts unknown (fetch first)

repo-workflows-repository_tools-label-87dc28eb = { $direction }: no route configured

repo-workflows-repository_tools-label-f25a8952 = { $a } ahead · { $b } behind

repo-workflows-repository_tools-object-view-14a9c8ce = Loose object: { $n } compressed bytes

repo-workflows-repository_tools-object-view-3e15593a = Object ID or file version, for example HEAD:src/main.rs. Inspection never downloads missing objects. Git 2.45+ is required for this guarantee.

repo-workflows-repository_tools-object-view-44de072f = { $o-kind-1 } · { $o-size-2 } bytes expanded · { $o-disk-3 } bytes on disk

repo-workflows-repository_tools-object-view-56daa2be = Tree entries / linked objects

repo-workflows-repository_tools-object-view-62a6da87 = Object

repo-workflows-repository_tools-object-view-7b47361a = Base

repo-workflows-repository_tools-object-view-8efd32c4 = How an object is stored

repo-workflows-repository_tools-object-view-9147a920 = { $data-format-1 } · { $data-objects-display-2 } · resolved from { $data-query-3 }

repo-workflows-repository_tools-object-view-9d4281bc = For deltas, representation size describes the delta instruction stream; expanded object size is above.

repo-workflows-repository_tools-object-view-b39d687c = Delta base: { $base }

repo-workflows-repository_tools-object-view-ddc47a7a = Git directory: { $data-git-dir-1 }
    Shared directory: { $data-common-dir-2 }

repo-workflows-repository_tools-object-view-e0723a86 = Inspect

repo-workflows-repository_tools-object-view-f4de0102 = { $if-i-0-object-else-base-1 }: { $row-oid-2 } · { $row-kind-3 } · packed { $row-disk-4 } B · representation { $row-size-5 } B · offset { $row-offset-6 } · depth { $row-depth-7 }

repo-workflows-repository_tools-patch-eef91a52 = No differences

repo-workflows-repository_tools-picker-f523255b = Known branches

repo-workflows-repository_tools-prepare-branch-review-2685a3d8 = Push preparation canceled

repo-workflows-repository_tools-prepare-branch-review-77a313b9 = Invalid selected push continuation

repo-workflows-repository_tools-prepare-branch-review-a89fdaff = The repository, profile or sign-in changed. Choose the push action again.

repo-workflows-repository_tools-publish-group-1f0d64fa = Publication incomplete: { $failed } destinations failed. Inspect per-destination results.

repo-workflows-repository_tools-publish-group-3cc9a2c6 = Publish group

repo-workflows-repository_tools-publish-group-449d962a = Every destination published the prepared commit

repo-workflows-repository_tools-refs-list-c2a1a3d6 = Copy object

repo-workflows-repository_tools-refs-list-edec79bb = Inspect object

repo-workflows-repository_tools-reftable-view-4101df87 = { $storage-git-1 } · current storage: { $storage-format-2 }

repo-workflows-repository_tools-reftable-view-87ed192f = All other writers are stopped and I have a backup

repo-workflows-repository_tools-reftable-view-891eab64 = Migration preserves refs and reflogs, refuses linked worktrees, and verifies both afterwards. Stop all other programs that write to this repository. Keep a repository backup before proceeding.

repo-workflows-repository_tools-reftable-view-c2782e54 = Ref storage and guided migration

repo-workflows-repository_tools-reftable-view-d3ba0aeb = Migrate to { $target } and verify

repo-workflows-repository_tools-reftable-view-ea4f0afa = I checked compatibility of my installed tools with the target format

repo-workflows-repository_tools-reftable-view-faca3ce1 = Reftable requires Git 2.45+ and compatible third-party tools. Some IDEs, embedded Git libraries, backup tools and older clients only support files. Check every tool that accesses this repository before migration. Files is the broadest compatibility choice.

repo-workflows-repository_tools-remote-config-preview-bba0304c = Remote preview: { $e }

repo-workflows-repository_tools-replacements-view-0a69dfc6 = Effective history

repo-workflows-repository_tools-replacements-view-1714de4d = Convert legacy grafts to replace refs

repo-workflows-repository_tools-replacements-view-33ea34c6 = Affected commit { $r-original-1 } → { $r-effective-2 }

repo-workflows-repository_tools-replacements-view-4953f897 = Remove link

repo-workflows-repository_tools-replacements-view-4cff5861 = Original history (--no-replace-objects)

repo-workflows-repository_tools-replacements-view-5fcfdd61 = Effective parents: { $r-parents-1 }

repo-workflows-repository_tools-replacements-view-7916bff6 = Create temporary graft

repo-workflows-repository_tools-replacements-view-7b8c9687 = Legacy info/grafts file (converted links remain visible/removable here):

repo-workflows-repository_tools-replacements-view-84da95ea = Commit:

repo-workflows-repository_tools-replacements-view-9c43f2c2 = I reviewed these local history changes

repo-workflows-repository_tools-replacements-view-b022884c = New parent:

repo-workflows-repository_tools-replacements-view-e51254f0 = Join imported histories: replace this commit's parents with a commit from the other history. Paste full commit IDs. Cyclic joins and existing replacements are refused.

repo-workflows-repository_tools-replacements-view-f5b874e5 = Replace refs and temporary history joins

repo-workflows-repository_tools-replacements-view-fcc3aff3 = Replace refs affect these commits and their descendants. Joins below are local and temporary; removing the replace ref restores the original history.

repo-workflows-repository_tools-repository-tool-mutation-988356ee = { $busy } is still running

repo-workflows-repository_tools-repository-tools-act-07fb4089 = Make clone independent

repo-workflows-repository_tools-repository-tools-act-38bb44e4 = Missing aggregate endpoint/session

repo-workflows-repository_tools-repository-tools-act-3c1c7a3a = Save publication group

repo-workflows-repository_tools-repository-tools-act-4e161156 = Save remote configuration

repo-workflows-repository_tools-repository-tools-act-66f98362 = Resolve submodule pointer

repo-workflows-repository_tools-repository-tools-act-69a2d9db = Remove replace ref

repo-workflows-repository_tools-repository-tools-act-82d14313 = Save pull and push routes

repo-workflows-repository_tools-repository-tools-act-888e8b45 = Migrate ref storage

repo-workflows-repository_tools-repository-tools-act-a7b0c90c = Join imported histories temporarily

repo-workflows-repository_tools-repository-tools-act-ad133eb8 = Convert legacy grafts to removable replace refs

repo-workflows-repository_tools-repository-tools-act-baa3959f = Missing branch snapshot

repo-workflows-repository_tools-repository-tools-act-e7cf34fe = Missing saved turn

repo-workflows-repository_tools-review-prepared-push-88d2d1a8 = Push selected route

repo-workflows-repository_tools-show-0256e5a0 = Run rebase in an isolated worktree

repo-workflows-repository_tools-show-03dc5c4f = Fetch refspecs: +source:destination; ^source excludes a branch/prefix

repo-workflows-repository_tools-show-04245473 = Refresh lists and file snapshots

repo-workflows-repository_tools-show-05519c89 = Reviewed identical hunks stay hidden across this session's turns. Changed hunks reappear. Marks stay local. Reverting checks the inverse against current files and retains later non-overlapping changes and staged contents.

repo-workflows-repository_tools-show-057248ff = Publish to several hosts

repo-workflows-repository_tools-show-05e1813b = Open retained candidate

repo-workflows-repository_tools-show-05e305a1 = I stopped other programs writing to this repository and have a backup

repo-workflows-repository_tools-show-05f3fddc = Mark snapshot reviewed and save last-looked boundary

repo-workflows-repository_tools-show-06b251d5 = Shared common directory: { $model-common-1 }

repo-workflows-repository_tools-show-07345f3f = Send this exact subtree split

repo-workflows-repository_tools-show-07aa5dfe = Shared and borrowed objects

repo-workflows-repository_tools-show-089eb3ef = Pull from

repo-workflows-repository_tools-show-0ae1bed9 = Save branch draft

repo-workflows-repository_tools-show-0b22ea73 = Review saved agent turn

repo-workflows-repository_tools-show-0d37e057 = New base:

repo-workflows-repository_tools-show-0e95ad58 = Submodule checkout: { $c-head-1 }

repo-workflows-repository_tools-show-107f87fc = Publish submodule dependencies before their parent

repo-workflows-repository_tools-show-111eda2a = Commit this branch only

repo-workflows-repository_tools-show-118694a9 = Publish dependencies, then review parent

repo-workflows-repository_tools-show-11d962b1 = Missing donor directory

repo-workflows-repository_tools-show-14d33bd0 = Unassigned

repo-workflows-repository_tools-show-1560b7d2 = Open submodule

repo-workflows-repository_tools-show-16312ab3 = Register source locally

repo-workflows-repository_tools-show-16d60e39 = For a plain vendored folder without subtree history: Fetch, inspect its differences, then explicitly Associate it with that fetched baseline. Association preserves every folder byte and records those differences as local edits; only future upstream changes are merged. Accept the reviewed association before subsequent updates. Existing Git-subtree imports can use Prepare update directly.

repo-workflows-repository_tools-show-16f5b799 = Named changelists

repo-workflows-repository_tools-show-17c47592 = Shelve this list only

repo-workflows-repository_tools-show-181a4047 = Session { $aggregate-session-1 } · { $aggregate-saved-turns-2 } saved turn(s) · fixed base { $review-parent-3 } → endpoint { $review-turn-4 }. Later saved turns and current unsaved files are excluded.

repo-workflows-repository_tools-show-1919101c = Replay this stack in separate worktrees

repo-workflows-repository_tools-show-19af9596 = Compare candidate of { $s-branch-1 }

repo-workflows-repository_tools-show-19cca4e2 = Check fetches heads/tags into temporary app-owned refs on each submodule. It checks the named fetch endpoint, including gitlinks in outgoing history. Nested dependencies publish first. These pushes are not atomic; failures stop their parent.

repo-workflows-repository_tools-show-1a6e5e0d = Associate existing folder with fetched baseline

repo-workflows-repository_tools-show-1b5dcef8 = Edits changed since preview; preview again before saving.

repo-workflows-repository_tools-show-1bd5bcfa = Current source: { $view-branch-1 }

repo-workflows-repository_tools-show-1c0954b2 = Lists own whole files and keep independent draft messages. Commit uses the reviewed raw working snapshot, creates an unsigned commit and bypasses hooks and filters. Other staged files remain staged. Shelve saves staged and working layers independently; unshelve refuses newer edits on its paths.

repo-workflows-repository_tools-show-1e4f897e = Git directory: { $model-git-1 }

repo-workflows-repository_tools-show-1e9112fe = Fetch registered branch

repo-workflows-repository_tools-show-1f711390 = Prepare update in own worktree

repo-workflows-repository_tools-show-1f89e0a1 = Mark visible unreviewed hunks reviewed

repo-workflows-repository_tools-show-1fad1933 = already published

repo-workflows-repository_tools-show-1fbe68fb = Last-looked boundary: { $aggregate-last-looked-as-deref-u-1 }

repo-workflows-repository_tools-show-202c8fbc = Opt in: automatically send this plain push when online, only if preflight has no findings

repo-workflows-repository_tools-show-222bd382 = Prepare selected send…

repo-workflows-repository_tools-show-2250c14b = Staged folder changes

repo-workflows-repository_tools-show-2c00ff77 = Retry failed destinations only

repo-workflows-repository_tools-show-2d0adec7 = Object store: { $crate-remote-display-model-objec-1 }

repo-workflows-repository_tools-show-2ddef7c7 = Refresh subtree registry

repo-workflows-repository_tools-show-2e1d0813 = I reviewed this exact queued commit, every endpoint and preflight finding

repo-workflows-repository_tools-show-301d9809 = Make this clone independent

repo-workflows-repository_tools-show-31a04bc4 = Required: { $oid }

repo-workflows-repository_tools-show-325b12c6 = Reload this fixed saved endpoint

repo-workflows-repository_tools-show-33463eb4 = Close proposal; retain worktrees

repo-workflows-repository_tools-show-34b10b8d = Rebase { $name }

repo-workflows-repository_tools-show-36c73d64 = Exact proposed branch patch

repo-workflows-repository_tools-show-3a7402e9 = I reviewed each original → candidate OID and the candidate diff before accepting or undoing

repo-workflows-repository_tools-show-3acc29f8 = { $c-path-1 } · base { $c-base-as-deref-unwrap-or-none-2 } · ours { $c-ours-3 } · theirs { $c-theirs-4 }

repo-workflows-repository_tools-show-3cc9a2c6 = Publish group

repo-workflows-repository_tools-show-3e04e021 = Prepare a renewed publication review

repo-workflows-repository_tools-show-3e9abe63 = Preview reverting only this turn…

repo-workflows-repository_tools-show-3f2768b0 = Checked fetch URL: { $git-redact-n-fetch-url-none-1 } · { $if-n-published-already-published-2 }

repo-workflows-repository_tools-show-41251e4b = Untracked files (not included in split publication):

repo-workflows-repository_tools-show-412ec52f = Reviewed isolated restack inputs (children use the preceding rewritten parent result):

repo-workflows-repository_tools-show-41a222be = Commit this list only

repo-workflows-repository_tools-show-43a8ef7d = Register an existing or new vendored folder

repo-workflows-repository_tools-show-440174a7 = Undo accepted layers

repo-workflows-repository_tools-show-4498585c = Result: { $oid }

repo-workflows-repository_tools-show-44e05a73 = Known commits containing both sides (you can also paste any full commit ID):

repo-workflows-repository_tools-show-454ee70f = Draft commit message for this list

repo-workflows-repository_tools-show-4601e33d = Accept requires a completely clean original checkout, including untracked and ignored files, and the exact original branch/OID. Resolve conflicts and stage their resolutions in the candidate worktree. Uncertain acceptance is reconciled without rerunning it. No candidate is cleaned up automatically.

repo-workflows-repository_tools-show-469f9427 = Undo last virtual operation

repo-workflows-repository_tools-show-46aafc00 = Open submodule to merge

repo-workflows-repository_tools-show-4734d197 = Revert only this turn

repo-workflows-repository_tools-show-4759498a = Create

repo-workflows-repository_tools-show-4829ff52 = Source branch:

repo-workflows-repository_tools-show-49346e37 = Exact source: { $n-source-1 } · push URL: { $git-redact-n-push-url-none-2 }

repo-workflows-repository_tools-show-4c8748ea = Queue this exact commit using its push route

repo-workflows-repository_tools-show-4d07d136 = Only the recorded commit is sent. Rewrites, destination changes and rejected or uncertain sends pause for review. Sending after a crash is uncertain and is never retried automatically. Queue data and protected commits are local application refs; do not mirror-push application refs.

repo-workflows-repository_tools-show-4e161156 = Save remote configuration

repo-workflows-repository_tools-show-4f480ac9 = Inverse of { $p-parent-1 } → { $p-turn-2 } against current working files; { $p-paths-len-3 } affected paths

repo-workflows-repository_tools-show-507eaa08 = original { $s-branch-1 }

repo-workflows-repository_tools-show-51768487 = Submodule conflict graph

repo-workflows-repository_tools-show-52b07a38 = Push outbox

repo-workflows-repository_tools-show-5370d2e8 = Current hunks and their owners:

repo-workflows-repository_tools-show-5471de9c = Reconcile exact acceptance receipts

repo-workflows-repository_tools-show-5673c401 = { $review-hunks-len-1 } hunks · { $review-hunks-len-unreviewed-len-2 } hidden reviewed

repo-workflows-repository_tools-show-5d3cc263 = All refs and namespace history

repo-workflows-repository_tools-show-5dc87aef = Separate pull and push routes

repo-workflows-repository_tools-show-5dcd35a3 = Each count compares this local branch to the last fetched ref for that route. Missing or excluded tracking refs show unknown. Changing a push branch affects gitcraft's ordinary branch pushes; Git itself still uses its native push.default/refspec rules.

repo-workflows-repository_tools-show-5dfdfed0 = I reviewed the exact split, all destination endpoints, selected profile/account and preflight findings

repo-workflows-repository_tools-show-5e3184e8 = Preview display truncated at 64000 characters.

repo-workflows-repository_tools-show-5f715b70 = Hunk display truncated; inspect the file diff before assigning ownership.

repo-workflows-repository_tools-show-5fa7aac5 = Ready

repo-workflows-repository_tools-show-5fb1b07d = Group { $p-group-1 } · { $p-commit-2 } → refs/heads/{ $p-branch-3 }

repo-workflows-repository_tools-show-616296e8 = Replay { $s-branch-1 } after { $s-old-base-2 } onto { $s-onto-3 }

repo-workflows-repository_tools-show-625c67be = Exact native transaction receipts distinguish committed metadata from an uncertain acknowledgement. This inspection never rolls back files, removes locks or retries publication.

repo-workflows-repository_tools-show-666eca6d = Source remote:

repo-workflows-repository_tools-show-66dfd7a6 = Review candidate patch

repo-workflows-repository_tools-show-67572031 = Remote: { $e-draft-name-1 } · all changes are repository-local

repo-workflows-repository_tools-show-6b30bed0 = Each hunk belongs to one real branch. The checked-out branch stays at its anchor commit, and Git's real index stays intact. Virtual branch commits use reviewed raw contents, create unsigned commits and bypass hooks and filters. Changed ownership and overlapping activation require explicit resolution. Working-folder changes require native macOS/Linux.

repo-workflows-repository_tools-show-6bdc8428 = candidate not yet complete

repo-workflows-repository_tools-show-6c1991e9 = Make independent copies a complete pack for refs, original ancestry, reflogs and index entries, verifies it in a separate repository, then removes the alternate link. Donors remain unchanged. The original alternate metadata is retained as a recovery backup. Staging is limited to 128 MiB per pack; larger repositories can use git repack -a before preparing again.

repo-workflows-repository_tools-show-6c892519 = Only the first 100 matching refs were examined. Paste a full commit ID to check another candidate.

repo-workflows-repository_tools-show-6f2680ab = { $b-name-1 } · { $if-b-active-active-else-inactive-2 } · { $b-owned-len-3 } owned hunks

repo-workflows-repository_tools-show-71ba13e6 = Show reviewed unchanged hunks

repo-workflows-repository_tools-show-72353618 = Preview mappings and prune deletions against fetch URL

repo-workflows-repository_tools-show-72456f4c = Display truncated at 32000 characters; open the turn comparison for more.

repo-workflows-repository_tools-show-739ffe8f = Everything since last looked · saved turns only

repo-workflows-repository_tools-show-7432d850 = Inspect local edits

repo-workflows-repository_tools-show-7790f11b = Save named group

repo-workflows-repository_tools-show-77c3b92d = No recorded state publications in this worktree.

repo-workflows-repository_tools-show-78f32476 = Send selected exact push

repo-workflows-repository_tools-show-798b0df6 = Destination branch:

repo-workflows-repository_tools-show-79af8f6d = New list:

repo-workflows-repository_tools-show-7b868cc6 = Apply this inverse; stop on overlapping or stale edits. A safety draft will preserve the current files.

repo-workflows-repository_tools-show-7bd990ec = candidate { $s-branch-1 }

repo-workflows-repository_tools-show-7c52fd3d = Refresh proposals and receipts

repo-workflows-repository_tools-show-7d86e238 = Conflicts that can wait

repo-workflows-repository_tools-show-7e381308 = Squashed import

repo-workflows-repository_tools-show-7eb9930d = Reload publication receipts

repo-workflows-repository_tools-show-7ffc5022 = Real branch tip: { $b-tip-1 }

repo-workflows-repository_tools-show-80c567ae = Old base (replay commits after):

repo-workflows-repository_tools-show-80ef5663 = No preflight findings in identity, signatures, messages, secrets, large blobs or remote history.

repo-workflows-repository_tools-show-83e94e8a = { $p-plan-commits-len-1 } outgoing commits

repo-workflows-repository_tools-show-8472b5d8 = Published: { $git-redact-output-none-1 }

repo-workflows-repository_tools-show-84e9b213 = Filter refs:

repo-workflows-repository_tools-show-864a00e5 = Lends objects to { $borrower }

repo-workflows-repository_tools-show-88db1c04 = publication required

repo-workflows-repository_tools-show-89f4921c = Fetch URLs (first is fetched; one per line)

repo-workflows-repository_tools-show-8bb0f122 = { $c-branch-1 } at { $c-old-2 }

repo-workflows-repository_tools-show-8bbd26a7 = Release stale ownership { $h-key-8-1 }

repo-workflows-repository_tools-show-8cc484e4 = Legacy entry: profile/account consent missing

repo-workflows-repository_tools-show-8daa08a5 = Conflict waiting: { $p }

repo-workflows-repository_tools-show-8dd5e5cc = History of ref / prefix (blank = all):

repo-workflows-repository_tools-show-8edc0123 = I reviewed every destination and accept partial publication

repo-workflows-repository_tools-show-8f3b56b3 = I reviewed each named remote, exact commit and target branch

repo-workflows-repository_tools-show-8f8df690 = State publication receipts

repo-workflows-repository_tools-show-8fb60b6e = Scoped sign-in: { $crate-git-redact-endpoint-none-1 } · account { $account-2 } · username { $username-3 }

repo-workflows-repository_tools-show-8fd0f155 = Anchor: { $m-anchor-1 } at { $crate-repo-short-m-base-2 }

repo-workflows-repository_tools-show-8fe4dcef = Change routes…

repo-workflows-repository_tools-show-9174163c = Isolated restack

repo-workflows-repository_tools-show-927164d1 = I reviewed this saved snapshot through the displayed endpoint

repo-workflows-repository_tools-show-92958e20 = Unshelve this list

repo-workflows-repository_tools-show-93fdd760 = Full upstream history

repo-workflows-repository_tools-show-96a8a275 = Queue consent pins the selected profile, connection account metadata and any scoped transient sign-in. A changed or missing legacy witness requires renewed approval. Prepared results expire after profile/sign-in changes.

repo-workflows-repository_tools-show-98a12458 = Continue / reconcile candidate worktree

repo-workflows-repository_tools-show-99256c25 = Selected profile { $p-consent-profile-1 } · connection account metadata pinned

repo-workflows-repository_tools-show-99d05df7 = Split committed folder history

repo-workflows-repository_tools-show-9aa26ad9 = Assign each current file to one active list (or leave it unassigned):

repo-workflows-repository_tools-show-9b1cea76 = { $p-plan-commits-len-1 } outgoing commits inspected

repo-workflows-repository_tools-show-9b9a8d05 = Read

repo-workflows-repository_tools-show-9bb7e70d = Continue / inspect resolved candidate

repo-workflows-repository_tools-show-9cbacb41 = Commit this exact owned-hunk tree as an unsigned virtual branch commit

repo-workflows-repository_tools-show-9d8f0223 = Pending: retain and inspect

repo-workflows-repository_tools-show-9e153558 = Exact split { $p-plan-head-1 } → { $p-plan-remote-2 }/{ $p-plan-destination-3 }

repo-workflows-repository_tools-show-9e8617f1 = initial parent of this session

repo-workflows-repository_tools-show-9fbeaee4 = Push:

repo-workflows-repository_tools-show-a1016f72 = Partial clone: some objects may require its promisor remote.

repo-workflows-repository_tools-show-a17b0445 = Exact commit { $p-plan-head-1 } → { $p-plan-remote-2 } / { $p-plan-destination-3 }

repo-workflows-repository_tools-show-a23601d6 = Uses installed Git's contrib subtree helper only on Prepare/Continue. Fetch and Publish are explicit network actions. Candidates, application refs and recovery directories remain local and retained; do not mirror-push application refs. Native Unix checkout only; helper absence/failure leaves an inspectable proposal.

repo-workflows-repository_tools-show-a253af39 = Saved inactive: { $h-path-1 } · { $h-key-8-2 }

repo-workflows-repository_tools-show-a2869e42 = Read outbox again

repo-workflows-repository_tools-show-a2bc21d3 = Approved profile { $consent-profile-1 } · selected account metadata pinned

repo-workflows-repository_tools-show-a39608b9 = Deactivate these hunks

repo-workflows-repository_tools-show-a503aa66 = Push to

repo-workflows-repository_tools-show-a6888267 = Last accepted import: { $oid }

repo-workflows-repository_tools-show-a6b3283e = Folder:

repo-workflows-repository_tools-show-aa785909 = In flight or interrupted: inspect its worktree and reconcile receipts explicitly

repo-workflows-repository_tools-show-aaac53fc = Only owned hunks of { $p-name-1 } will enter this branch commit. The folder, anchor and real index remain as reviewed.

repo-workflows-repository_tools-show-ab27abcb = Inspect state publication receipts

repo-workflows-repository_tools-show-ab31db17 = Stale ownership: { $h-path-1 }

repo-workflows-repository_tools-show-ad16979e = Committed source: { $v-branch-1 } at { $v-head-2 }

repo-workflows-repository_tools-show-ae033504 = Saved groups

repo-workflows-repository_tools-show-ae30be2e = Save draft message

repo-workflows-repository_tools-show-aff83496 = Group:

repo-workflows-repository_tools-show-b0a1bb3e = Reconcile exact acceptance receipt

repo-workflows-repository_tools-show-b0e84ec1 = Accept this subtree update

repo-workflows-repository_tools-show-b1249b5a = I reviewed the pull and push destinations before saving

repo-workflows-repository_tools-show-b26a038e = Pick an existing commit containing both sides, or open the submodule, merge there, then refresh this graph. Recording changes only the parent index.

repo-workflows-repository_tools-show-b2f82ce9 = { $p-branch-1 }: { $p-expected-2 } after { $p-old-base-3 } → { $p-onto-4 }

repo-workflows-repository_tools-show-b7b34c26 = Record chosen submodule commit in parent

repo-workflows-repository_tools-show-b85186ac = Parent { $p-branch-1 } at { $p-commit-2 } → { $p-parent-remote-3 }/{ $p-parent-branch-4 } ({ $git-redact-p-parent-url-none-5 })

repo-workflows-repository_tools-show-b87ce680 = Pull:

repo-workflows-repository_tools-show-b9cc89c6 = Renewed approval keeps the ORIGINAL pinned commit and accepts the CURRENT destination and source history. Verify any uncertain prior outcome on the remote first. Autosend is disabled.

repo-workflows-repository_tools-show-ba765b08 = Accept ready candidates

repo-workflows-repository_tools-show-ba8a03e2 = Refresh topology and prepare again

repo-workflows-repository_tools-show-bbcce2f9 = Refresh after merge

repo-workflows-repository_tools-show-be1e9f5f = Activate these hunks

repo-workflows-repository_tools-show-c23fbbaa = Remote URLs, refspecs and mirror policy

repo-workflows-repository_tools-show-c29574be = Preview truncated; compare this turn before accepting.

repo-workflows-repository_tools-show-c2c3785a = Save both routes

repo-workflows-repository_tools-show-c757c39b = Rebase and restack run in their own retained, detached worktrees. Originals stay unchanged until you accept. Resolve conflicts and stage the result in the proposal's worktree, then Continue. Replays are unsigned and skip repository hooks. Acceptance uses the guarded native transaction and refuses dirty/stale original worktrees; layers report partial success individually. Undo restores each accepted original tip through the same guard. No worktree is deleted automatically.

repo-workflows-repository_tools-show-c7879dfc = Review split publication

repo-workflows-repository_tools-show-cb9b7596 = Unstaged folder changes

repo-workflows-repository_tools-show-ccbcb47e = Parent: { $git-redact-s-none-1 }

repo-workflows-repository_tools-show-d05d82b6 = Push URLs (blank inherits fetch URLs; Git pushes to every listed URL)

repo-workflows-repository_tools-show-d15b2630 = These pushes are not atomic. A successful host stays published if another host fails. Each destination receives the exact commit shown; retry sends only failed destinations.

repo-workflows-repository_tools-show-d3197597 = Committed folder changes

repo-workflows-repository_tools-show-d329d33b = Preview commit of owned hunks…

repo-workflows-repository_tools-show-d5680e1f = Pruning with these mappings would delete { $p-prune-len-1 } local refs:

repo-workflows-repository_tools-show-d5714f7d = I reviewed this exact candidate patch / fetched association baseline; for a renewed send I inspected the remote after any rejected/uncertain attempt

repo-workflows-repository_tools-show-d653a1c5 = Remove entry

repo-workflows-repository_tools-show-dad5a969 = Receipt

repo-workflows-repository_tools-show-db839840 = Worktrees using this common directory:

repo-workflows-repository_tools-show-deee0d61 = Mark this hunk reviewed

repo-workflows-repository_tools-show-df2937d4 = Includes application, bisect, backup and namespace refs. Object IDs name the exact snapshot below.

repo-workflows-repository_tools-show-e01205cf = Rename list

repo-workflows-repository_tools-show-e0b832a0 = This repository's primary store

repo-workflows-repository_tools-show-e0e54b30 = Fetched source: { $oid }

repo-workflows-repository_tools-show-e27da1c1 = History (up to 200 commits)

repo-workflows-repository_tools-show-e37c5523 = Mirror pushes: force all refs and delete extra destination refs

repo-workflows-repository_tools-show-e5c0b5df = New real branch:

repo-workflows-repository_tools-show-e6eadbe7 = I reviewed these URLs, mappings and mirror consequences

repo-workflows-repository_tools-show-e8331aba = Fetch source explicitly to compare committed folder contents

repo-workflows-repository_tools-show-e949fe01 = Vendored folders and subtree publication

repo-workflows-repository_tools-show-e9e7ecb3 = Check every dependency and outgoing gitlink

repo-workflows-repository_tools-show-ea03ec13 = Choose an existing configured source remote; its fetch and push routes may differ. Registration records provenance only. A missing folder can be imported with Prepare update after an explicit Fetch.

repo-workflows-repository_tools-show-ea2af9ef = Fetch mirroring is expressed by +refs/*:refs/*. It writes directly into local namespaces, including branches. Push mirroring can overwrite or delete others' work. These previews never prune or push.

repo-workflows-repository_tools-show-ea303dfe = State: { $report-intent-state-ref-1 }
    Previous: { $report-intent-previous-2 }
    Proposed: { $report-intent-proposed-3 }
    Receipt: { $report-intent-receipt-ref-4 }
    Source branch: { $report-intent-branch-as-deref-un-5 }
    Expected HEAD: { $report-intent-expected-head-as-d-6 }
    Recovery entries: { $report-intent-evidence-7 }

repo-workflows-repository_tools-show-ea721fba = Refresh ownership and working snapshot

repo-workflows-repository_tools-show-ed38427e = Only this split commit is sent. No later original or candidate edits join it. Multiple endpoint writes are not atomic. Rejection or missing acknowledgement pauses the proposal; no automatic retry.

repo-workflows-repository_tools-show-edc5ba83 = I reviewed the selected commit and want to record this parent resolution

repo-workflows-repository_tools-show-eef651ca = Parallel branches in this folder

repo-workflows-repository_tools-show-ef0a338b = Committed: exact receipt

repo-workflows-repository_tools-show-efdfe52a = Create a new branch to start. Existing working hunks remain unassigned until you choose their owners.

repo-workflows-repository_tools-show-f279f80b = Push refspecs (blank uses push.default)

repo-workflows-repository_tools-show-f2d4cf1c = This Git cannot enumerate arbitrary root refs; established pseudorefs are included when present.

repo-workflows-repository_tools-show-f41d4ab9 = Recovery directories retain original/candidate files and full access metadata. Inspect these entries and index.lock before any manual recovery. Journals are bounded to 256 operations; archive resolved journal refs explicitly after inspecting their recovery entries.

repo-workflows-repository_tools-show-f5265a26 = Unreviewed

repo-workflows-repository_tools-show-f581c406 = Draft commit message

repo-workflows-repository_tools-show-f5b402c5 = Folder versus fetched upstream { $base }; original import baseline is unknown, so this also includes upstream differences

repo-workflows-repository_tools-show-f66ce8b9 = Prepare exact commit and destinations

repo-workflows-repository_tools-show-fad6057b = Reviewed

repo-workflows-repository_tools-show-faf582fc = Create virtual branch

repo-workflows-repository_tools-show-fb535d0d = Close proposal; retain all work

repo-workflows-repository_tools-show-fcb0aec1 = Check/publish remote:

repo-workflows-repository_tools-show-fccdc9ec = Review pinned commit and current destination…

repo-workflows-repository_tools-show-ff1cf26d = Committed local edits relative to last accepted upstream { $base }

repo-workflows-repository_tools-show-fff50056 = Leave mode; retain files and branch refs

repo-workflows-repository_tools-start-send-628a48e6 = Prepared outbox profile/sign-in changed; prepare and approve again

repo-workflows-repository_tools-start-send-994ea6b0 = Send exact queued push

repo-workflows-repository_tools-start-send-e7b0c78d = Autosend opted-in plain outbox push

repo-workflows-review_workflow-review-document-ui-7d9eb7ac = Close

repo-workflows-review_workflow-show-c3812fc4 = Remove

repo-workflows-stack-publish-dialog-19766ed6 = Cancel

repo-workflows-stack-restack-dialog-19766ed6 = Cancel

repo-workflows-stack-stack-dialogs-19766ed6 = Cancel

repo-workflows-surgery-footer-19766ed6 = Cancel

repo-workflows-test_lane-test-banner-7d9eb7ac = Close

repo-workflows-test_lane-test-output-window-e21f935f = Copy

repo-workflows-timeline-close-button-7d9eb7ac = Close

repo-workflows-timeline-commits-683768ab = and { $total-list-len-1 } more

repo-workflows-timeline-finished-19766ed6 = Cancel

repo-workflows-timeline-forget-confirm-19766ed6 = Cancel

repo-workflows-timeline-jump-controls-19766ed6 = Cancel

repo-workflows-working_drafts-history-7d9eb7ac = Close

repo-workflows-working_drafts-recovery-19766ed6 = Cancel

repo-workflows-activity-count = { $count ->
        [one] One operation{ $running }, newest first
       *[other] { $count } operations{ $running }, newest first
    }

repo-workflows-activity-running-extra = { " and the one running" }

repo-workflows-activity-entry-summary = { $outcome } · { $duration } · { $age } ago{ $background ->
        [yes] { " · in the background" }
       *[no] { "" }
    }{ $repeats ->
        [one] { "" }
       *[other] { " · " }{ $repeats } times in a row
    }

repo-workflows-agent-stage-conflicts = { $count ->
        [one] Stage this conflicted file as resolved: { $paths }. Git then forgets the conflict and its three sides.
       *[other] Stage these conflicted files as resolved: { $paths }. Git then forgets the conflicts and their three sides.
    }

repo-workflows-agent-send-confirmation = Typed { $what } into the { $terminal } terminal. Press Enter there to send the message.

repo-workflows-agent-note-location = Note for the agent · { $path }:{ $line }{ $old ->
        [yes] { " (as it was)" }
       *[no] { "" }
    }

repo-workflows-agent-session-title = { $first } · { $count ->
        [one] One turn
       *[other] { $count } turns
    } · { $when }

repo-workflows-ai-coverage = The tool saw { $whole } of { $files ->
        [one] one file
       *[other] { $files } files
    } in full; the rest only in part or by name (Settings → AI sets how much is sent).

repo-workflows-agent-hard-reset-body = Move { $target } from { $from } to { $to } (“{ $subject }”) and make the working tree match it. { $lost ->
        [0] There are no uncommitted changes to tracked files now.
        [one] The uncommitted changes to one tracked file are thrown away for good.
       *[other] The uncommitted changes to { $lost } tracked files are thrown away for good.
    } { $left ->
        [0] No commits are removed.
        [one] The commit after { $to } is no longer on { $target }.
       *[other] The { $left } commits after { $to } are no longer on { $target }.
    } Untracked files stay.

repo-workflows-agent-mixed-reset-body = Move { $target } from { $from } to { $to } (“{ $subject }”) and unstage everything. { $files ->
        [one] The staged file keeps its changes, unstaged.
       *[other] The { $files } staged files keep their changes, unstaged.
    } The working tree stays as it is.

repo-workflows-agent-drop-body = Remove { $commit } (“{ $subject }”) and its changes from { $branch }, and replay { $after ->
        [one] the following commit
       *[other] the { $after } following commits
    }. If a later commit conflicts, the rebase stops for you to finish or abort.

repo-workflows-agent-delete-body = Delete the local branch “{ $name }”, at { $commit } (“{ $subject }”). { $force ->
        [yes] Delete even if it is not merged.
       *[no] Git refuses if it is not merged.
    } Its copy on a remote stays.

repo-workflows-agent-force-lost = { $count ->
        [one] The commit on { $to } that { $branch } does not have is removed for everyone who uses it.
       *[other] The { $count } commits on { $to } that { $branch } does not have are removed for everyone who uses it.
    }

repo-workflows-agent-push-body = Send { $branch }’s { $count ->
        [one] new commit
       *[other] { $count } new commits
    } to { $to }.

repo-workflows-dco-toggle-0 = { repo-workflows-dco--5e1a6b4e }. Enabled here; this repository also asks for it: { $reason }

repo-workflows-dco-toggle-1 = { repo-workflows-dco--5e1a6b4e }. Disabled here, although this repository asks for it: { $reason }

repo-workflows-dco-toggle-2 = { repo-workflows-dco--5e1a6b4e }. Enabled for this repository

repo-workflows-dco-toggle-3 = { repo-workflows-dco--5e1a6b4e }. Disabled for this repository

repo-workflows-dco-toggle-4 = { repo-workflows-dco--5e1a6b4e }. Enabled automatically: { $reason }

repo-workflows-dco-toggle-5 = { repo-workflows-dco--5e1a6b4e }. Enabled automatically in a repository that asks for it (a DCO file or failing DCO check)

repo-workflows-dco-all-signed = { $count ->
        [one] ✔ The commit is signed off.
       *[other] ✔ All { $count } commits are signed off.
    }

repo-workflows-dco-missing-signoffs = { $missing } of { $total } commits lack a sign-off accepted by the DCO check (their author’s or committer’s):

repo-workflows-bakeoff-unassembled = { $count ->
        [one] One file picked and not assembled: close again to drop it
       *[other] { $count } files picked and not assembled: close again to drop them
    }

repo-workflows-collision-files = { $count ->
        [one] One file
       *[other] { $count } files
    }

repo-workflows-collision-conflicts = { $a } and { $b } conflict on { $count ->
        [one] one file
       *[other] { $count } files
    }:

repo-workflows-collision-check = { $count ->
        [one] Check one branch
       *[other] Check { $count } branches
    }

repo-workflows-doctor-remove-setting = Remove { $key } = { $value } from { $file }? { $scope ->
        [global] This applies to all your repositories.
        [system] This applies to every user of this machine.
       *[local] This applies here.
    } The value is not kept: to get it back, set it again.

repo-workflows-doctor-history-loaded = The graph loads { $page } commits at a time as you scroll, and at most { $max } in all. Search loads more as needed, up to that limit too. { $loaded } loaded now.{ $more ->
        [yes] { " More can be loaded." }
       *[no] { "" }
    }

repo-workflows-doctor-missing-hooks = This folder does not exist (or is not a folder): no hook runs.{ $custom ->
        [yes] { " Whatever set core.hooksPath (a hook manager?) is not installed. Run its install step or unset core.hooksPath." }
       *[no] { "" }
    }

repo-workflows-hotfix-change-summary = { $branch } · { $count ->
        [one] One change
       *[other] { $count } changes
    }{ $has-base ->
        [yes] { " since " }{ $base }
       *[no] { "" }
    }

repo-workflows-history-calendar-day = { $day }
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }

repo-workflows-lfs-checkout-summary = { $files ->
        [0] No file of the checkout is kept in LFS.
       *[other] { $missing ->
            [0] { $files ->
                [one] One file in LFS, downloaded.
               *[other] { $files } files in LFS, all downloaded.
            }
           *[other] { $files ->
                [one] One file in LFS;
               *[other] { $files } files in LFS;
            } { $missing ->
                [one] one is only a pointer here
               *[other] { $missing } are only pointers here
            } ({ $bytes } not downloaded).
        }
    }

repo-workflows-lfs-force-unlock-confirmation = { $count ->
        [one] Click again to force: its owner loses the lock.
       *[other] Click again to force: the owners lose their { $count } locks.
    }

