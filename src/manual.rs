//! The manual (F1, or the book in the title bar): what gitcraft does and its keyboard shortcuts.

use egui::{RichText, Ui};

use crate::keymap;
use crate::theme::{self, Tokens};

/// Ctrl, or Cmd on macOS, as the shortcuts are written.
pub(crate) fn command() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// What each part of the window does: (title, points).
const FEATURES: &[(&str, &[&str])] = &[
    (
        "Repositories and tabs",
        &[
            "Open, create or clone a repository from the start page; recent ones are listed there.",
            "One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.",
            "Right-click a tab to give it its own icon and highlight colour.",
            "Drag a tab along the strip to reorder the tabs.",
            "What you type is kept, per repository, if gitcraft crashes or is closed: the commit message, unsent pull-request comments, reviews and replies, and an interactive rebase being set up come back when the repository is opened again. Each goes once committed, sent or started, or when you empty or cancel it. After a crash, gitcraft says so when it starts again. Nothing typed into a password or sign-in prompt is kept.",
            "The main actions have keys: fetch, pull, push, stash, new branch, terminal, stage all and commit, and keys to go through the tabs (listed below, and in Settings → Keyboard shortcuts). Tooltips, the palette and menus show them. They do nothing while a text field or the terminal has the keyboard, so typing never runs one; only Ctrl+Enter in the commit message commits.",
            "A greyed-out menu entry says why under it.",
            "One window: started again while it runs (gitcraft <folder>, Open in gitcraft, the menu entry), gitcraft opens that folder in a tab of the window already open, or switches to its tab, and brings the window to the front (where the system won't allow that, its taskbar button flashes). Started without a folder, it just comes to the front.",
            "Remote on the start page opens a repository in WSL or over SSH: git runs there, this window shows it. Browse… lists that machine's folders, its repositories marked. The toolbar names the machine; the terminal opens a shell there. What ssh asks (a password, a passphrase, a new host key) is asked in this window; Remember connection keeps a password encrypted in the system's credential store, and Forget saved SSH sign-ins (Ctrl+P) removes them.",
            "Right-click a tab and choose Teleport to… (or Ctrl+P, Teleport to another machine…) to carry its work to this machine, a WSL distribution or an SSH host: the branch, the staged and unstaged changes and the untracked files arrive in a new worktree there, and a tab opens on it. It goes in a clone of the same project found there, another repository there, a clone made first or a new repository made from what is sent; nothing here is changed. No push or forge is needed.",
        ],
    ),
    (
        "Portable copy",
        &[
            "An empty gitcraft.portable file or an empty data folder next to gitcraft (next to gitcraft.app on macOS) keeps the settings, open tabs, profiles and their git config files, the avatar cache, downloaded updates and Mergiraf in that data folder, starting fresh; Settings → General → Portable mode shows where.",
            "A portable copy keeps SSH passwords in this computer's credential store only when Remember SSH sign-ins in this computer's credential store is on (Settings → General → Portable mode; it starts off): otherwise they are asked again after a restart.",
        ],
    ),
    (
        "Command palette",
        &[
            "Ctrl+P lists every action; type a few letters of one. The best matches come first: letters at the start of words or in a row, like reb for Rebase or cp for Cherry-pick. The letters typed are highlighted.",
            "With nothing typed, what can be done with the selection comes first, then what you ran lately (kept across restarts). With a commit selected: Cherry-pick, Revert, Reset main to here…, Interactive rebase onto it, a branch, tag or patch from it. With a file's diff open: stage, unstage or discard it, view it, its history or blame.",
            "Check out a branch…, Merge into…, Rebase … onto…, Compare … with… and Push another branch… pick the branch in a second step, with its own search. Esc goes back to the first step, and so does Backspace in its empty field.",
            "Typing a branch's name offers to check out the few branches that match best; Check out a branch… lists them all.",
        ],
    ),
    (
        "Graph",
        &[
            "Every branch with its pill on its latest commit: solid for the checked-out one, a laptop when it is local, a cloud when a remote has it.",
            "Double-click a pill to check that branch out (and pull it when its remote is ahead; on the checked-out branch's remote pill, just pull); hover +N to see every branch of the commit, and right-click any of them.",
            "Right-click a commit or a pill for branches, tags, merge, rebase, cherry-pick, revert, reset (soft, mixed, keep or hard) and more.",
            "Search messages, authors and hashes, or switch to Code to find the commits that added or removed some text.",
            "Search operators narrow a search and combine with each other and with text: author: and committer: (name or email), path: (a file or folder, letter case as in the repository; *.md works), after: and before: (by commit date: 2024-05-01, 2024-05, 2024, today, yesterday, or a time ago such as 12h, 3d, 2w, 6m for months, 1y), message:, contents: (git log -S), regex: (git log -G), merges: and -merges:. Quote values with spaces: author:\"Jane Doe\". Names and messages ignore the case of A–Z; changes match the case typed unless a name or message is matched too. A hash in the text still finds its commit.",
            "Typing an operator lists suggestions: authors and committers from history (as .mailmap names them), files and folders of the checked-out commit, dates. ↑ / ↓ highlight one, Tab or Enter puts it in, Esc closes the list. A search written wrong says why under the field.",
            "Drag the borders in the column header to resize; right-click it to pick the columns, make the graph column compact or reset the columns.",
            "Click GRAPH in the column header to order history topologically, by commit date or by author date, or to show First parent only (a merge without the commits it brought in). Kept per repository.",
            "While HEAD is detached a banner says so, with Create branch here and Back to the branch it was on. Checking out something else after committing there asks first, and can keep those commits on a new branch.",
            "Interactive rebase: reorder, reword, edit, squash, fix up or drop commits, then start it. Drag a row by its handle to move it; Edit stops the rebase after that commit: stage what should change, and Continue rebase amends it and goes on.",
            "Drag a branch's pill (or its row on the left) onto another branch or a commit: a menu offers merging, rebasing, an interactive rebase or a fast-forward, those that make sense there. The targets that take it light up; Esc cancels.",
            "Drag a commit onto the checked-out branch to cherry-pick it, or onto another commit of the checked-out branch to move it above or below that one, or squash or fix it up into it. Ctrl+Z undoes a drop like the menu's entries.",
        ],
    ),
    (
        "Compare",
        &[
            "Ctrl+click a second commit to list the files that differ between the two (the changes row is the working tree).",
            "On a branch's menu, “Changes x adds to main” shows what it changed since they diverged, like a pull request; “Compare main with x” shows every difference.",
            "On a merge, tick “What this merge resolved” to see only what differs from the merge git makes of its parents: how its conflicts were resolved (hunks marked “resolves a conflict”) and anything changed beyond that, by hand or by a merge made with other options (“edited beyond the merge”). A “resolved” or “edited” badge on a merge row of the graph says it has some; click it to see them. The merges shown are redone in the background for the badges, except in a repository with merge drivers of its own. Needs git 2.36 or newer; git can't redo octopus merges (more than two parents).",
        ],
    ),
    (
        "Updates",
        &[
            "A new version shows in a bar under the tabs: Update now restarts as it, with the tabs kept.",
            "An update keeps the version it replaced: Settings → General → Updates → Go back to x.y.z restarts as that version and stays on it until you click Unpin there.",
        ],
    ),
    (
        "Bisect",
        &[
            "Right-click a commit and choose Start bisect here… to find the commit that brought a change in: say whether it is good (from before the change) or bad (it has the change), and give the other end (HEAD is the bad one unless you name another). It refuses while there are uncommitted changes, conflicts, or a merge or rebase in progress.",
            "A banner over the graph then names the commit checked out for you to test and about how many steps are left; Good, Bad and Skip mark it, and right-clicking another commit marks that one. The graph tints good commits green, bad ones red and skipped ones grey.",
            "Run a command… in the banner lets git bisect run test each commit for you, on the repository's machine (here, in WSL or over SSH): exit code 0 is good, 125 skips the commit, any other code up to 127 is bad, and 128 or more stops. Its output shows under the banner as it comes; Stop run stops it, and the bisect stays where it got to.",
            "At the end the first bad commit is selected and its details open. Finish (or Stop, which asks first, before the end) runs git bisect reset, which checks out again what was checked out when it began.",
            "A bisect started in a terminal shows in the banner as soon as gitcraft sees it, and goes on from there; each worktree has its own.",
        ],
    ),
    (
        "Parallel bisect",
        &[
            "Run in parallel… in the bisect banner (or Bisect in parallel from here… in a commit's menu, or the palette) tests several commits at once, each in a worktree of its own, until the first bad commit is found: about log base (k + 1) of n rounds instead of log base 2 of n. Your checkout isn't touched, and no git bisect needs to be going on.",
            "Give the bad and the good commits, the test command (read as git bisect run reads it: exit code 0 is good, 125 skips, any other up to 127 is bad; 128 or more, a crash, stops it on an end and otherwise skips that commit with a warning), the machine (this computer, a WSL distribution or an SSH host of ~/.ssh/config), how many at once (that machine's cores by default, up to 16; at most 32) and how long one test may take (one that runs longer is stopped and counts as skipped).",
            "On another machine than the repository's, the commits are sent there first, to a clone you name or to a copy gitcraft keeps in ~/.cache/gitcraft/bisect.",
            "The two ends are tested in the first round, beside the first commits picked: when the good end already fails, or the bad end passes, it says so and stops. The banner gives the round, how many commits may still be the first bad one and how many are being tested; the graph marks those being tested, tints the tested ones green, red, grey (skipped) or amber (timed out), and draws a thin line by the commits still in question, which shrinks round after round. Hover a tested commit for the end of its output.",
            "A commit whose tree was tested before (with the same command, on the same machine, while the tab is open) isn't tested again.",
            "At the end the first bad commit is selected; when a bisect is going on between the same marks, Mark in the bisect marks it bad and its parents good there, which ends that bisect on the same commit. Cancel stops every test and removes the worktrees, as does any other end; what a crash leaves is removed when the tab opens again or the next parallel bisect starts.",
        ],
    ),
    (
        "Changes and commits",
        &[
            "Stage, unstage or discard whole files from the right pane.",
            "Path / Tree at the top of the right pane lists the files flat or in their folders; click a folder to fold it.",
            "In a file's diff, stage, unstage or discard one hunk, or click the lines you want and act on only those.",
            "A file kept in Git LFS shows as its LFS objects and their sizes; the palette has LFS: Fetch and LFS: Pull when git-lfs is installed.",
            "LFS locks: a padlock on a file's row shows who holds it (hover it for since when); Lock and Unlock are in its menu. LFS: Locks… (Ctrl+P) lists every lock, unlocks several at once, force-unlocks someone else's (admins only) and makes a pattern lockable. Changed files someone else holds, and changed lockable files nobody holds, are flagged above the changes; after a push, gitcraft offers to give back your locks on the files it carried.",
            "The arrows in a diff's header jump from change to change; the marks beside the scrollbar show where the changes are, and a click on one jumps there.",
            "Write the summary and description and commit; amend the last commit; pick the identity you commit as, or Use the profile's author to take a repository's own one out.",
            "The commit box starts from the repository's commit.template (its comment lines left out) and what its prepare-commit-msg hook adds, as git's editor would (stopped after 10 seconds; what goes wrong shows under the message and never stops a commit). An unchanged template isn't committed, as git refuses it; amending or a merge's message takes its place.",
            "Above the message: templates (commit.template, Run prepare-commit-msg, your saved ones for this repository or all, Save this message as a template; Undo brings back a message one replaced), the type / scope picker (Conventional Commits or Gitmoji), Add co-author (the repository's authors as .mailmap names them, written as a Co-authored-by: trailer) and the options (Conventional Commits, Gitmoji or no picker; whether the hook prefills; issue link patterns).",
            "Typing in the message completes the names of the changed files and, at the start of the summary, the types and scopes used before: while the list shows, Tab takes the highlighted one instead of moving to the next field, ↑ / ↓ move, Esc closes it.",
            "Issue references in a commit's message link to their page under it: #12 to the forge's issue, and any pattern you add in the commit box's options (JIRA-\\d+ → https://jira.example.com/browse/$0).",
            "Skip hooks in the commit box, or right-click Push for Push, skipping hooks: that one commit or push runs with --no-verify. A hook that fails is named in the error.",
            "Profiles (title bar, or Settings → Profile): each has its own tabs, recent repositories, theme, git author and commit signing. A repository's own git config (user.name, user.email, signing) still wins over the profile's.",
            "Themes: the palette icon in the title bar, or Settings → Appearance with a thumbnail of each (point at one to preview it). Import a VS Code theme or a Base16 scheme, follow VS Code's theme, or save a copy of one as a file to edit; set your own accent, corners and lane colours for any.",
            "Point at a setting and the panel beside it shows what it does, drawn with the values set now.",
            "Settings → Commit signing sets the format, key and program git signs with, and whether it signs commits and tags by default, in your global git config; Settings → Profile can sign just for the profile instead.",
            "The sparkles button in the commit box has an AI tool you already have (claude -p, codex exec, gh copilot, or a command of your own) write the message from the staged changes (when amending, from the last commit with them), on the repository's machine (here, in WSL or over SSH); gitcraft keeps no API key. It is off until you pick a tool (from the button, or Settings → AI, which also has the prompts and how much of the diff is sent). A message you typed is never replaced without asking; Cancel stops the tool.",
            "Double-click a commit's message in the right pane to edit it (any commit of the checked-out branch with no merges after it).",
            "Stash (toolbar or palette): everything, the picked files, only what is staged (git 2.35 or newer), or a Snapshot, which saves a stash and leaves the files and the staging as they are. You choose the message and whether untracked files go; picked untracked files always go.",
            "Ctrl+click files in the changes to pick them for stashing (or right-click one: Stash this file…); the bar over the list stashes those picked. Conflicted files and a staged rename or deletion can't be stashed by file: unstage it first, or stash what is staged.",
            "Below a commit's hash, click a section to fold it out: Signature (whether it is valid, and if not why, with the signer and git's output; Check again after importing a key), Contained in (its branches and tags, and the first tag after it), Notes (add, edit or delete a note in the namespace picked; Fetch notes and Push notes share that namespace with a remote, and stop rather than overwrite when both sides have notes the other lacks) and Backlinks (later commits that revert it, cherry-pick it, name it in a trailer such as Fixes: or just mention its hash; click one to open it).",
            "Fold staged changes into an older commit: right-click it in the graph and pick “Fold staged changes into this commit”, or use “Fold…” beside “Amend last commit”. First it shows the commit and the ones after it that get rewritten, and warns when they are already pushed. “Keep as fixup! commit for later” only commits them as “fixup! <summary>” on top. A conflict is resolved as in any rebase, and Abort goes back to before the fold; Ctrl+Z undoes it, the changes staged again.",
            "Undo and redo commits, merges, rebases, folds, resets, checkouts, branches, tags and stashes.",
            "Reflog (left pane, a branch's menu, or Ctrl+P): where HEAD and each branch have been, what moved them (commit, checkout, reset, rebase…) and from which commit to which, also for what was done in the terminal or before gitcraft was opened. Select an entry to see its commit; Create branch here (or a double-click) gets it back, or cherry-pick it, check it out or copy its hash.",
            "Find lost commits scans for the commits no branch, tag or stash has any more, even once their reflog has expired, newest first, with the same buttons; a dropped stash is applied back with Apply stash. It reads every object, so on a big repository it takes a while; Cancel stops it.",
            "Before a reset, deleting a branch (here or on a remote), dropping a commit or discarding a file, the dialog says what it will do: the commits leaving the branch (for a delete, those the checked-out branch hasn't), each marked still on another branch or only here; the files whose uncommitted changes are lost; whether the commits it rewrites were pushed, so that a force push will be needed; the git commands it runs (folded); and whether Ctrl+Z restores it.",
        ],
    ),
    (
        "Coding agents",
        &[
            "Add gitcraft to an agent as an MCP server: claude mcp add --scope user gitcraft -- gitcraft mcp (Claude Code), or codex mcp add gitcraft -- gitcraft mcp (Codex). Settings → Agents has both, ready to copy.",
            "Agents read the graph, status, diffs, blame, branches, conflicts, CI builds and pull requests, with or without a window open.",
            "What an agent changes (stage, commit, branches, checkout, stash, cherry-pick, revert, reset, moving a commit) runs in the repository's tab, as a step of its undo list: the robot on the undo button says the next Ctrl+Z takes back the agent's step.",
            "A push, deleting a branch, a hard reset, dropping a commit or staging a conflicted file waits on an Approve / Deny card naming the agent and the git commands; no answer in two minutes denies it.",
            "Settings → Agents turns agents' changes off; reading still works.",
        ],
    ),
    (
        "Branches, remotes and more",
        &[
            "Left pane: local branches with ahead / behind counts, remotes, worktrees, submodules, stashes, pull requests and tags.",
            "Submodules show whether they are modified, not initialised or out of date; right-click one to update it or open it in a tab. Settings → Git can keep them up to date after each checkout, pull, merge and rebase.",
            "Hover a branch and click its eye to hide it from the graph, or Solo it from its menu to see only the branches soloed; “Show all” at the top brings them all back.",
            "Pin to top (a branch's menu) keeps it at the top of Local; Name / Recent in the Branches header sorts the others by name or by their last commit. Both are kept per repository.",
            "A branch marked merged has its work in the default branch: merged, rebased or cherry-picked, or squash-merged by a pull request into it; stale, no commit for 90 days (Settings → Git); gone, its upstream deleted on the remote.",
            "The brush in the Branches header (or Select stale or merged branches… in the palette) lists those branches with why, only the merged ones ticked, and deletes the ticked ones here, and if asked their branches of the same name on the remote, in one go that Undo brings back. It never offers the checked-out branch, the default one, or one checked out in a worktree.",
            "Conflicts open in a three-pane editor with Ours, Theirs and Both for each conflict.",
            "Before you click, Merge, Rebase and Cherry-pick in the menus say “conflicts in 3 files”, “applies cleanly” or “may conflict”; hover one for the files (for a rebase, the commit where it would stop) and whether Mergiraf could likely solve each. A badge beside the branch in the toolbar says when it would conflict with its upstream (or the default branch). Nothing is changed to find out. Uncommitted changes aren't part of the check; git may refuse to start if they touch the same files.",
            "Open a conflict's Original to see the text both sides started from and what each inserted and deleted relative to it. When both sides added the file there is none, and the card says so.",
            "Keep both… on a conflicted file that can't be merged (an image, an archive) previews, then writes each side's version beside it, named after its branch (logo.main.png, logo.feature-x.png; ours / theirs without one, -2 when a name is taken), never over an existing file. Check them in the changes list, then Stage both, which removes the old name: references to it may need updating.",
            "Right-click a file (of the working tree, a commit or a comparison) and choose Open in diff tool to see its diff in an external tool; a conflicted file's menu, and the conflict editor's Resolve in merge tool button, resolve it in one. Settings → Git → External tools picks the tool: the one git's diff.tool / merge.tool names, or Beyond Compare, Araxis, VS Code, Meld, KDiff3, P4Merge or WinMerge. The view refreshes when the tool closes, and the file counts as resolved only when the tool reports success (git's mergetool.<tool>.trustExitCode; otherwise, when it saved the file); mergetool.keepBackup says whether a .orig copy is kept. In WSL a preset is the Windows program, handed the files through \\\\wsl.localhost; over SSH the tool runs on that machine and has to be installed there.",
            "How this conflict came about, above the conflicts of a merge, rebase, cherry-pick or revert, lists each side's commits that touched the file; pick one on each side to see what it changed or to test-merge the two. Find where it started finds the first pair whose merge already conflicts in the file (test merges with git merge-tree; your files and the index stay as they are). Cancel stops it.",
            "Right-click a file for its history, its blame or the whole file as it is at the commit.",
            "A file or hunk that makes the checks weaker gets a “weakens checks” badge: it removes assertions or tests, skips, disables or focuses tests, adds an assertion that can't fail, loosens a tolerance, rewrites a snapshot wholesale, lets CI fail or removes a CI job, or adds --no-verify. Hover the badge for the lines that made it.",
            "Switch a diff from Diff to File to read the whole file; tick View all files under a commit's files to list every file in it.",
            "Ctrl+F in a diff, a file or a blame finds in it (in the graph it searches the commits): every match is marked, Enter and Shift+Enter (or F3 and Shift+F3) go to the next and previous one. Aa matches case, ab matches whole words only, .* takes the text as a regular expression.",
            "Copy link to … and Open … on GitHub (GitLab, Forgejo, Gitea) in the menu of a commit, a branch or a file, and of a commit's diff lines (right-click a line, or select text for a range) or a file's lines at a commit (click one, Shift+click for a range, then right-click). Links to a commit, file or lines use the commit's hash; a branch's follows the branch. They go to the remote the checked-out (or right-clicked) branch tracks, else origin, else the one you pick; the status line warns when the commit or branch isn't on that remote yet.",
            "In a Markdown file's diff, the eye button renders it: its changes block by block, or with File the whole file.",
            "Sync (toolbar, or the command palette) pulls the current branch the way Pull is set to, then pushes it; if the pull fails (conflicts, or it can't fast-forward), nothing is pushed.",
            "A push the remote refuses because it has commits you don't have opens a banner: how many, and whether your branch was rewritten here (amended, rebased, undone) or both moved on, or is only behind. Pull, then push joins them; Force push (with lease)… names the commits it removes from the remote first, and pushes nothing if the remote (or your branch) moved since it was checked.",
            "A push a hook or a protected branch refuses shows git's message in full, with no force push offered. A branch on no remote yet, with several remotes, asks which one to publish it on.",
            "Review this branch in a branch's menu has the AI tool review what the branch adds to the branch pull requests go to (main, master or develop; the checked-out branch, for that one itself); its answer shows in a window, with Copy and Review again.",
            "Right-click a pull request and choose Open in worktree to work on its branch in a worktree of its own, made for it when there is none.",
            "A terminal panel with tabs, in the repository's folder.",
            "The pop-out button in the header of a diff, the terminal, a pull request or the CI view (or Pop out the view shown / Pop out the terminal in the command palette) moves it into a window of its own, say on another monitor; the main window shows what was underneath, with a reminder at the bottom that raises the window or brings the view back. It works there as before and stays open while you switch tabs; a commit selected in the main window leaves it open, and the next diff, pull request or CI view opens there. Closing that window, or its bring-back button, puts the view back in the main window; closing the view itself (its ×, or Esc outside the terminal) closes the window.",
        ],
    ),
    (
        "Keyboard and screen readers",
        &[
            "Tab and Shift+Tab move through the buttons, rows and fields; a painted button or row that has the keyboard is ringed in the accent colour, and Enter or Space clicks it.",
            "The graph is a single Tab stop, on the selected commit: ↑ and ↓ move the selection and the keyboard with it, and the next Tab leaves the graph.",
            "Screen readers hear each row by name, a commit as “commit a301da9, its summary, its author, how long ago, its branches and tags”, and files, branches, tabs and buttons likewise.",
            "Right-click menus and double-clicks (checking out a pill) still need the mouse.",
            "With animations turned off in the system when gitcraft starts (GNOME or KDE, Reduce motion on macOS, Animate windows when minimizing and maximizing on Windows), tabs and panels move into place without sliding. GITCRAFT_REDUCE_MOTION=1 (or 0) decides instead.",
        ],
    ),
    (
        "Custom commands",
        &[
            "Settings → Custom commands adds commands of your own to a commit's, a branch's or a file's menu, or to the palette (Ctrl+P).",
            "A command line is run with sh on the repository's machine (here, in WSL or over SSH), in its folder. It gets $SHA, $SHORT_SHA, $BRANCH, $REMOTE, $FILE and $REPO as shell variables, never pasted into its text: write \"$FILE\" and a name with spaces, quotes or $(…) stays one word.",
            "Prompts ask for a text or a branch first, each given to the command as its own variable ($MESSAGE…). Ask before running shows the command line before it runs.",
            "Its output goes to a toast (only its last 8 lines, red when it fails) or to a terminal tab of its own, where it can be watched and answered; Enter closes it.",
            "A repository can bring its own in .gitcraft/commands.json. They are offered only once you have read the file (Review repository commands… in a menu or the palette) and trusted it, and asked about again whenever it changes. Settings → Custom commands lists the repositories trusted, to forget them.",
        ],
    ),
];

/// Keyboard and mouse shortcuts of the views, besides the main actions' keys in
/// [`crate::keymap`]: (keys, what they do). `Cmd` stands for Ctrl outside macOS.
pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Keyboard navigation",
        &[("Tab / Shift+Tab", "Next / previous button, row or field"), ("Enter / Space", "Click the focused button or row")],
    ),
    (
        "Graph",
        &[
            ("↑ / ↓", "Previous / next commit"),
            ("Cmd+F", "Search commits"),
            ("Enter / Shift+Enter", "Next / previous search result"),
            ("↑ / ↓ then Tab or Enter", "Pick a search suggestion"),
            ("Cmd+click", "Compare with the selected commit"),
            ("Double-click a pill", "Check out that branch or tag"),
            (
                "Drag a pill or a commit",
                "Merge, rebase, fast-forward, cherry-pick, move or squash",
            ),
            ("Esc", "Close the find bar, diff, comparison or search"),
        ],
    ),
    (
        "Interactive rebase",
        &[
            (
                "P / R / E / S / F / D",
                "Pick, reword, edit, squash, fix up or drop the selected row",
            ),
            ("↑ / ↓", "Select the row above / below"),
            ("Alt+↑ / Alt+↓", "Move the selected commit up / down"),
            ("Esc", "Close the editor without rebasing"),
        ],
    ),
    (
        "Changes",
        &[
            ("Cmd+Enter", "Save a note, or commit from the message fields"),
            (
                "Click / Shift+click",
                "Pick lines in a diff, or a range of them",
            ),
            (
                "Click / Shift+click",
                "Pick lines of a file to link to (View file, or File in a commit's diff)",
            ),
            ("Cmd+click a file", "Pick it to stash (or unpick it)"),
            ("F7 / Shift+F7", "Next / previous change in a diff"),
            ("Alt+↓ / Alt+↑", "Next / previous change in a diff"),
            ("Cmd+Z", "Undo"),
            ("Cmd+Y / Cmd+Shift+Z", "Redo"),
        ],
    ),
    (
        "Find in a diff, file or blame",
        &[
            ("Cmd+F", "Find in the diff, file or blame shown"),
            (
                "Enter / Shift+Enter",
                "Next / previous match (while the find bar is open)",
            ),
            ("F3 / Shift+F3", "Next / previous match"),
            ("Esc", "Close the find bar"),
        ],
    ),
    (
        "Terminal",
        &[
            (
                "Cmd+C",
                "Copy the selection (without one, it goes to the shell)",
            ),
            ("Cmd+Shift+C", "Copy the selection"),
        ],
    ),
    (
        "Settings",
        &[
            ("Cmd+F", "Search the settings"),
            ("Esc", "Clear the search"),
            ("Alt+↑ / Alt+↓", "Previous / next page"),
        ],
    ),
];

/// Every group of shortcuts, as the manual and Settings → Keyboard shortcuts list them: the
/// window's and a repository tab's keys from [`crate::keymap`], then the views' own. Each row:
/// the keys as [`key_combo`] draws them (Ctrl or Cmd already written), what they do.
pub(crate) fn shortcut_groups() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let from_keymap = |rows: &[keymap::Row]| {
        rows.iter()
            .map(|r| (keymap::row_keys(r), r.what))
            .collect::<Vec<_>>()
    };
    let mut groups = vec![
        ("Window", from_keymap(keymap::WINDOW_ROWS)),
        ("Repository", from_keymap(keymap::REPOSITORY_ROWS)),
    ];
    groups.extend(SHORTCUTS.iter().map(|(title, keys)| {
        let keys = keys
            .iter()
            .map(|(combo, what)| (keymap::written(combo), *what))
            .collect();
        (*title, keys)
    }));
    groups
}

/// Draws the manual's contents.
pub fn ui(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let heading = |ui: &mut Ui, text: &str| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(2.0);
    };

    for (title, points) in FEATURES {
        heading(ui, title);
        for point in *points {
            // The bullet on the left, the text wrapping beside it.
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("•").color(t.text_faint));
                ui.add(
                    egui::Label::new(
                        RichText::new(point.replace("Ctrl", command())).color(t.text_dim),
                    )
                    .wrap(),
                );
            });
        }
    }

    ui.add_space(8.0);
    ui.separator();
    for (title, keys) in shortcut_groups() {
        heading(ui, &format!("{title} shortcuts"));
        egui::Grid::new(("manual_keys", title))
            .num_columns(2)
            .spacing([16.0, 6.0])
            .min_col_width(170.0)
            .show(ui, |ui| {
                for (combo, what) in &keys {
                    ui.horizontal(|ui| key_combo(ui, combo));
                    ui.label(RichText::new(*what).color(t.text_dim));
                    ui.end_row();
                }
            });
    }
}

/// Key combinations, `a / b`, each drawn like a keycap.
pub(crate) fn key_combo(ui: &mut Ui, combo: &str) {
    let t = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing.x = 4.0;
    for (n, part) in combo.split(" / ").enumerate() {
        if n > 0 {
            ui.label(RichText::new("/").color(t.text_faint));
        }
        key_chip(ui, part);
    }
}

/// A key combination drawn like a keycap.
fn key_chip(ui: &mut Ui, keys: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.field)
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).font(theme::mono(11.5)).color(t.text));
        });
}
