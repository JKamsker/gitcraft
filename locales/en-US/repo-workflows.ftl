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

repo-workflows-activity-seconds-6f29d45b = { $d-as-secs-f32-1 } s

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

repo-workflows-agents-tip-1c0ba476 = 
    Shown in this tab: its changes are in Uncommitted changes

repo-workflows-agents-tip-5c1b8dd5 = 
    Click to open it and review its changes

repo-workflows-agents-tip-7d8e5553 = 
    Task: { $row-task-prompt-1 }

repo-workflows-agents-tip-95d5fbf7 = 
    Agent: { $row-task-agent-1 }

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

