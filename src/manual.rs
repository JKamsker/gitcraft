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
            "Open, create or clone a repository from the start page; recent ones are listed there, each with its branch (or the commit it is detached at), a dot when it has uncommitted changes, and when it was last opened. One on another machine shows its branch only while a connection to it is open (it is never connected to just for this).",
            "When there are recent repositories, type on the start page to filter them by name or path; ↑ / ↓ pick one, Enter opens it, Esc clears the filter.",
            "Drop a folder on the window to open its repository; drop a .patch, .diff, .eml or .mbox file on a repository tab to apply it (a mail as commits, as Apply patch does). Several dropped at once are applied one after the other, each to the tab it was dropped on; if one fails, the ones after it are not applied.",
            "Scan a folder for repositories (start page) looks a few folders deep (skipping hidden ones, node_modules, target and the like) and lists the repositories found, to open one or add those not listed yet to the recent list (as many as fit: it is never emptied for them). Stop keeps what was found so far.",
            "Workspaces (+ beside WORKSPACES on the start page): named groups of repositories, here or on other machines. Each row shows the branch, uncommitted files, ahead / behind and when it last fetched; click it to open that repository, hover it to see why a look or fetch failed. One on another machine is looked at unasked only while gitcraft is connected there.",
            "On a workspace: + adds an open or recent repository, or another folder; ↻ looks again (connecting where needed); the cloud fetches them all, asking for no password (one that needs a sign-in says so); the folder opens them all in tabs; the gear renames or deletes it. ✕ on a row (on hover) takes it out.",
            "Where's my work? (start page, beside WORKSPACES, or the palette): every repository gitcraft knows (recent ones of every profile, open tabs, workspaces, and those found on a machine) is looked at for commits not pushed, commits in a repository with no remote, stashes, uncommitted changes (here or in another worktree) and branches whose remote branch is gone (listed while they have commits on no remote). Push sends a branch to its upstream of the same name, or to origin and tracks it there, asking for no password; a branch tracking one of another name, or a branch of the repository, is pushed from the repository itself. Open opens the repository. A machine gitcraft isn't connected to is only looked at with Connect and look (and pushed to with Connect and push); Find more repositories searches a machine's home folder (here, a WSL distribution or a host of ~/.ssh/config).",
            "Pull requests for you (start page): across every repository you opened, on GitHub, GitLab, Forgejo and Gitea, the pull requests whose review is asked of you, and yours whose checks (a GitLab pipeline) fail or that have comments by others you haven't seen. Forgejo and Gitea are searched once per host, so a pull request from your fork to its upstream is found too. Click one to open it in its repository's tab; a GitLab merge request, or one in a repository not opened here, opens in the browser. What it said so far then counts as seen, as does what one of yours had when the inbox first met it. It asks again every five minutes while the start page shows, shows the first eight (Show more for the rest), and says which forges couldn't be asked on hover; one whose tool isn't installed or that has no login is left until ↻, which asks now, also of repositories on machines gitcraft isn't connected to. It uses gh and glab's logins, and for Forgejo and Gitea one from GITEA_SERVER_URL / GITEA_SERVER_TOKEN, tea's, or one gitcraft already used in a tab: asking unasked, it never runs git's credential helper (which may ask to unlock a keyring).",
            "One tab per repository. Changes made elsewhere (editor, terminal) show up by themselves.",
            "Right-click a tab to give it its own icon and highlight colour.",
            "Copy diagnostics (at the top of this manual, or Help: Copy diagnostics in the palette) puts what a bug report needs on the clipboard: gitcraft's version and channel, the system, the graphics adapter, git, gh, glab and tea (with a login or not, never a token) and the git settings in effect, without your name, email, signing key, paths or home folder. Check it before posting.",
            "Drag a tab along the strip to reorder the tabs.",
            "What you type is kept, per repository, if gitcraft crashes or is closed: the commit message, unsent pull-request comments, reviews and replies, and an interactive rebase being set up come back when the repository is opened again. Each goes once committed, sent or started, or when you empty or cancel it. After a crash, gitcraft says so when it starts again. Nothing typed into a password or sign-in prompt is kept.",
            "The main actions have keys: fetch, pull, push, stash, new branch, terminal, stage all and commit, and keys to go through the tabs (listed below, and in Settings → Keyboard shortcuts). Tooltips, the palette and menus show them. They do nothing while a text field or the terminal has the keyboard, so typing never runs one; only Ctrl+Enter in the commit message commits.",
            "A greyed-out menu entry says why under it.",
            "One window: started again while it runs (gitcraft <folder>, Open in gitcraft, the menu entry), gitcraft opens that folder in a tab of the window already open, or switches to its tab, and brings the window to the front (where the system won't allow that, its taskbar button flashes). Started without a folder, it just comes to the front.",
            "A repository with no commits shows its first steps in the graph: make the first commit, add a remote, publish.",
            "A “Did you know” tip shows under the graph, a different one each day; a hint shows once, the first time you do something with a shortcut nearby.",
            "The eye beside a tip turns tips and hints off; Settings → General → Tips turns them back on and can show the hints seen again.",
            "Tabs restored on start are read only when you first open them. A tab left in the background for 30 minutes with nothing going on in it (no operation, running terminal, typed message, open editor, review, pull request or builds) stops watching its folder, fetching and following builds, and drops its graph and diff: opening it reads it again, with the same commit and diff.",
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
        "From the terminal and the file manager",
        &[
            "gitcraft . opens the repository the current folder is in, from any of its subfolders or worktrees; gitcraft <folder> does the same for that folder.",
            "gitcraft log <file> opens the file's history, gitcraft blame <file>:<line> its blame scrolled to that line, gitcraft show <rev> a commit and gitcraft compare a..b (or a...b, since they diverged) the files that differ. gitcraft --help lists them. While gitcraft is running, each goes to its window.",
            "Links from the browser: a gitcraft://open?url=<link> link (or the link itself pasted into the palette, the start page or given to gitcraft) opens a pull request, commit or file#L40 of GitHub, GitLab, Gitea / Forgejo or Bitbucket in the clone you have of that repository, among the open tabs and recent repositories; with none, it offers to clone it.",
            "Files have History in gitcraft and Blame in gitcraft in their right-click menu: in Explorer when the installer adds them, in Dolphin from the .deb and .rpm packages, and in Finder (Quick Actions) and Files (Scripts) once Settings → General adds them there.",
        ],
    ),
    (
        "Gerrit",
        &[
            "For Gerrit, set git config gitcraft.forge gerrit, then reload and fetch. Hosts named gerrit.… and *.googlesource.com are recognised automatically.",
            "Push for review picks the target branch and optional topic, checks every commit above the fetched target for a unique Change-Id, then pushes the exact previewed commit to refs/for/<branch>. Install your server’s commit-msg hook and amend commits with missing IDs. Fetch and rebase divergent or merged series first.",
            "The arrow on a graph row, or Open Gerrit change in its menu, opens the change. Amend last commit keeps its original Change-Id, even with a new description; push for review again to upload another patch set.",
            "For another web address or a reverse-proxy prefix, set git config gitcraft.gerritUrl https://review.example.org/gerrit. Git handles transport and sign-in.",
        ],
    ),
    (
        "Graph",
        &[
            "Every branch with its pill on its latest commit: solid for the checked-out one, a laptop when it is local, a cloud when a remote has it.",
            "Double-click a pill to check that branch out (and pull it when its remote is ahead; on the checked-out branch's remote pill, just pull); hover +N to see every branch of the commit, and right-click any of them.",
            "Right-click a commit or a pill for branches, tags, merge, rebase, cherry-pick, revert, reset (soft, mixed, keep or hard) and more.",
            "A robot on a commit's row: an agent wrote it (amber: with a person), as the commit records it (git-ai notes, Agent Trace, an agent's account or Co-authored-by trailer). Agent-written in the search bar steps through them; blame and diffs show a stripe per line; a branch's menu says how much agents wrote.",
            "Search messages, authors and hashes, or switch to Code to find the commits that added or removed some text.",
            "Search operators narrow a search and combine with each other and with text: author: and committer: (name or email), path: (a file or folder, letter case as in the repository; *.md works), after: and before: (by commit date: 2024-05-01, 2024-05, 2024, today, yesterday, or a time ago such as 12h, 3d, 2w, 6m for months, 1y), message:, contents: (git log -S), regex: (git log -G), merges: and -merges:. Quote values with spaces: author:\"Jane Doe\". Names and messages ignore the case of A–Z; changes match the case typed unless a name or message is matched too. A hash in the text still finds its commit.",
            "Typing an operator lists suggestions: authors and committers from history (as .mailmap names them), files and folders of the checked-out commit, dates. ↑ / ↓ highlight one, Tab or Enter puts it in, Esc closes the list. A search written wrong says why under the field.",
            "Drag the borders in the column header to resize; right-click it to pick the columns, make the graph column compact or reset the columns.",
            "Split commit… (the menu of a commit of the checked-out branch): select one of the new commits on the left (or press its number), then click files, hunks or lines on the right to put them in it; each new commit has its own message. Check split shows each one's patch, Split commit replaces the old commit with them and replays the commits after it. Ctrl+Z undoes it.",
            "Distribute staged fixes… (above the staged files, or the palette): each staged change goes into the earlier commit of the branch that last changed its lines; clear matches are taken, the rest suggested or left staged. Check, then absorb. Ctrl+Z undoes it; an installed git-absorb can do it instead.",
            "Move changes to another commit… (same menu): click what to take out of the commit (click again to keep it), pick another commit of the branch or the working changes (staged), check what stays and what moves, then move it; a commit left with nothing is dropped. Ctrl+Z undoes it.",
            "Click GRAPH in the column header to order history topologically, by commit date or by author date, or to show First parent only (a merge without the commits it brought in). Kept per repository.",
            "While HEAD is detached a banner says so, with Create branch here and Back to the branch it was on. Checking out something else after committing there asks first, and can keep those commits on a new branch.",
            "Interactive rebase: reorder, reword, edit, squash, fix up or drop commits, then start it. Drag a row by its handle to move it; Edit stops the rebase after that commit: stage what should change, and Continue rebase amends it and goes on.",
            "Drag a branch's pill (or its row on the left) onto another branch or a commit: a menu offers merging, rebasing, an interactive rebase or a fast-forward, those that make sense there. The targets that take it light up; Esc cancels.",
            "Drag a commit onto the checked-out branch to cherry-pick it, or onto another commit of the checked-out branch to move it above or below that one, or squash or fix it up into it. Ctrl+Z undoes a drop like the menu's entries.",
            "Ghost run: rest the pointer on a Rebase, Hard reset, Pull, Force push, Delete or Drop commit entry, in a menu or on the toolbar (hold Alt to skip the wait), and the graph shows the result: the new commits faint with dashed lanes, the pills where they would point outlined, the commits that would be lost crossed out. A strip over the foot of the graph lists those commits with their authors, the files of the working tree that would change and where it would stop on a conflict. Looking changes nothing: only choosing the entry does.",
            "Back and forward (Alt+← / Alt+→, the mouse's back and forward buttons, or the arrows at the start of the toolbar) go through the tab's recent views: the graph with its selected commit, diffs, a file's history, blame or contents, pull requests and builds. Clicking through commits is one step; following a link (a parent, a branch, a tag) is a step of its own.",
            "The panel button in a diff's header (or Settings → Graph → Diffs) shows the diff below the graph instead of in its place: the graph stays usable, the diff follows the file opened next, and the border between them drags to resize. Esc still closes the diff.",
            "After a rebase, an amend or any other rewrite gitcraft makes, Compare before / after (beside the message saying it's done, or Compare before / after the last rewrite in the branch's menu) shows the old commits and the new ones paired up: unchanged ones faded, changed, added, dropped and moved ones marked. Click a pair to see how its patch changed. The old commits are kept for 30 days.",
            "Start hotfix from v2.3.1… in a version tag's menu makes hotfix/2.3.2 there; Finish hotfix… (its menu, or the palette) merges it into develop (if used) and main, tags main's merge, deletes it and pushes it all, shown first and undone with Ctrl+Z (also when it stops part-way).",
            "Cut release… in a commit's menu (or the palette) suggests the next version from the Conventional Commits since the last tag and drafts the notes; it can move the version in Cargo.toml / package.json and CHANGELOG.md, tag (annotated or signed), push and make the release on the forge with files, showing each step first. A step that fails can be resumed; Ctrl+Z undoes what was done here.",
            "Description takes the flexible width; Branch / Tag fits visible pills until you drag its border. Right-click the header to restore Fit visible branch / tag pills, pick columns, make the graph compact or reset widths. Faint lane labels identify offscreen tips; hover a description for the full message.",
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
            "A branch that has diverged from its upstream (rebased, not pushed yet) has “Compare commits with origin/x” in its menu: the commits of each, paired up as git range-diff does.",
        ],
    ),
    (
        "Code owners",
        &[
            "With a CODEOWNERS file (GitHub's, GitLab's with its sections, or Gitea's and Forgejo's regular expressions, each read where and how that forge reads it), each file row shows its owners, and a line above a commit's, a comparison's, a pull request's or the staged files says who they need: “Needs payments and infra”.",
            "Create pull request suggests the owners of what the branch changes as reviewers (from the base's CODEOWNERS; none for a draft); untick those you don't want asked.",
            "Check CODEOWNERS (Ctrl+P) lists the lines the forge reads differently than they look, the patterns that match no file and the files without an owner.",
        ],
    ),
    (
        "Tests over history",
        &[
            "Right-click a commit and choose Test commits up to here… (or Test commits… in the palette) to run a command, such as cargo test, at each commit from the base branch to it. It runs on the repository's machine (here, in WSL or over SSH), in scratch worktrees in that machine's temporary folder, several commits at once (at most 4 on another machine), in the background: you can keep working. Exit code 0 passes; any other fails.",
            "The graph then has a test lane: a small square by each commit, green with a tick when it passed, red with a cross when it failed, outlined while it waits or runs, amber with a dot when it couldn't be tested. Hover it for what it means; click it to see the last lines the test printed, with Copy (for tests run since gitcraft started: a result from before, or from git test, has no output to show). Right-click the column header (or Settings → Columns) to show or hide the lane; the flask over it opens Test commits….",
            "A banner over the graph shows how far the run got, with Stop (tests that finished keep their result), then how it went and the oldest commit that failed (a click selects it).",
            "Results are kept by tree in git notes, as git-test keeps them (refs/notes/tests/default, the command in test.default.command), so git test on the command line and gitcraft share them. A rebase that keeps a commit's files keeps its square, and a tree tested once is not tested again unless you tick Test again commits that have a result. Changing the command can forget the old one's results.",
        ],
    ),
    (
        "Rehearse",
        &[
            "Right-click a branch (its pill, or in the left pane) and choose Rehearse merge or rebase… (or use the palette for the base branch) to try a rebase of the checked-out branch onto it, or a merge of it, before doing it. It is played through in a scratch worktree on the repository's machine; your branches and files stay as they are.",
            "The report says where it stops (for a rebase, at which commit, say commit 3 of 5) and which files conflict, or that it goes through; with a test command it also says whether the result passes (a result already stored for its files is taken instead of running it again).",
            "Tick Rehearse the merge too (or the rebase) to see both side by side, with whether they give the same files. Rebase now or Merge now does the one you pick for real, after checking that neither branch moved since; it goes into the undo history (Ctrl+Z), and conflicts open in the conflict editor as usual.",
        ],
    ),
    (
        "Find the interaction",
        &[
            "When two branches each pass their tests but fail once together, right-click one of them (its pill, or in the left pane) and choose Find the interaction… (or type Find the interaction in the palette): it looks for the commits of the checked-out branch and that one that fail together.",
            "It tests subsets of their commits, each applied on the commit the two diverged from, in a scratch worktree on the repository's machine, in the background (delta debugging), until it can say, for instance, these two commits fail together; either alone passes. Click a commit to select it in the graph.",
            "It first checks that the two fail together and that each passes alone, and says so when they don't (or when their commits don't apply together). It runs at most the number of tests you set; out of tests, it shows the smallest failing set it got to. A failure is run again (once by default) before it is believed: one that doesn't repeat is set aside as flaky. Stop ends it.",
            "The two branches may have up to 200 commits together since they diverged, and no merges; the search's checkouts run no hooks. Commits of one branch that fail without the other's (one taken without the later commit that fixes it) don't count as an interaction. With the repository's test command, results are shared with the test lane (kept by tree).",
        ],
    ),
    (
        "Changes and commits",
        &[
            "Stage, unstage or discard whole files from the right pane.",
            "Path / Tree at the top of the right pane lists the files flat or in their folders; click a folder to fold it.",
            "In a file's diff, stage, unstage or discard one hunk, or click the lines you want and act on only those.",
            "Undo part of a past commit: right-click a file in its list for Revert changes to this file, or in its diff use Revert file, Revert hunk, or click lines and Revert N lines. The change goes into the working tree to look over before committing (Ctrl+Z undoes it when the file hadn't changed since). When the file changed since, a 3-way merge puts it in (staged), and conflicts are marked in the file. Binary files can't be reverted this way.",
            "A file kept in Git LFS shows as its LFS objects and their sizes; the palette has LFS: Fetch and LFS: Pull when git-lfs is installed.",
            "LFS locks: a padlock on a file's row shows who holds it (hover it for since when); Lock and Unlock are in its menu. LFS: Locks… (Ctrl+P) lists every lock, unlocks several at once, force-unlocks someone else's (admins only) and makes a pattern lockable. Changed files someone else holds, and changed lockable files nobody holds, are flagged above the changes; after a push, gitcraft offers to give back your locks on the files it carried.",
            "LFS: Storage… (Ctrl+P, in a repository that uses LFS) shows the disk the LFS objects take, the files that are only pointers with Download, which folders git-lfs downloads, and Prune. In View all files of the checked-out commit a cloud marks those files; their menu has LFS: Download. A file's menu also has LFS: Fetch only its folder, and Track *.ext in LFS for a type not yet in LFS.",
            "Unity projects: Unity: Set-up… (Ctrl+P) checks Force Text, visible .meta files, .gitattributes and .gitignore, adds what is missing, and registers UnityYAMLMerge; when it is found, a conflicted scene or prefab has Solve with UnityYAMLMerge. An asset staged without its .meta (or the other way round) is flagged above the changes, with the fix.",
            "Other binary files show their kind, sizes and details before and after, and Text (Word, PowerPoint slides in show order, OpenDocument, Excel cells with their formulas, PDFs where their text is plain), Members (ZIP-based files) or Bytes (hex) of both sides. A note says when a side's text is incomplete or couldn't be read, or its members aren't compared. A file diffed through its textconv says so and is staged or unstaged whole.",
            "The arrows in a diff's header jump from change to change; the marks beside the scrollbar show where the changes are, and a click on one jumps there.",
            "A hunk's header says how many unchanged lines are folded above it (“42 unchanged lines”) and its buttons open them: ↑ those right above the hunk, ↓ those right below the one before, Show all, or Function (the function the hunk is in, as far as it goes). Lines opened next to a hunk stay open when the diff is read again, a line commented on in a pull request opens by itself, and hunks and lines stage as before.",
            "Code moved within a file shows in a muted colour of its own, with a bar beside both places. Moved to line… / Moved from line… on the block's first line opens a panel with both places, the lines edited on the way marked, and a button to go to each; Esc closes it.",
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
        "Timeline and undo tree",
        &[
            "Ctrl+Shift+Z (or the clock in the toolbar) shows the timeline: each operation with the branches it moved and the files it changed, kept after a restart. Redo is Ctrl+Y.",
            "Branch moves made elsewhere (the terminal, another app, while gitcraft was closed) show up too, marked elsewhere, named from the reflog; Ctrl+Z undoes them as well.",
            "Undo a few steps and do something new: the steps undone stay, as a side path of the tree. Pick any entry and Go here (or double-click it) to put the branches back as it left them; changes in files are kept (staged ones end up unstaged).",
            "An entry's old branch tips come back with Put … back, or as a new branch with New branch there, even after git gc: they are kept under refs/gitcraft/undo/.",
            "Forget… drops older entries, after asking; past the newest 300 they go by themselves.",
            "After a rebase, squash, reorder or amend, a receipt says whether the final files are identical, the authors kept, how many messages changed and what became of the signatures; in the timeline, Follow each commit pairs each old commit with its replacement (as git range-diff does).",
            "Remove this, keep later work: on an entry that only made commits on a branch (commits, a merge, cherry-picks), plays the commits made on it since again without it, on a new branch in a worktree beside the repository; the branch itself stays. A commit that no longer applies stops it there, its conflicted files listed: resolve them in that worktree, or discard the attempt.",
        ],
    ),
    (
        "Repository doctor",
        &[
            "The wrench in the toolbar (or Doctor: … in the palette) opens the repository doctor in the big area, its sections on the left; Esc closes it.",
            "Ignore rules and attributes: type a path, or pick Ignore rules and attributes in a working-tree file's menu, to see the rule that ignores it (or takes it back with !), whether it is tracked, its line endings in the repository and the working tree, and what its attributes make of it: text or binary, LFS, merge and diff drivers, and drivers named but not set up here. The ignored files are listed; edit the repository's .gitignore files, .git/info/exclude or your own excludes file, preview the files the change would ignore or stop ignoring (by that file's rules alone), then save.",
            "Hooks: the folder git runs hooks from (core.hooksPath or .git/hooks), each hook with the operations that run it, and what keeps one from running: not executable, CRLF line endings, an interpreter that isn't installed, a folder with a hook's name. Make executable and Convert to LF fix it in one click. husky, lefthook and pre-commit are recognised, with where to change their hooks. What git and its hooks printed during the last operations is kept in memory, until gitcraft closes, to read after.",
            "Health and maintenance: commits, packed and loose objects and their size, how long git status and the tab's last reloads took, and what would make git faster here (the commit-graph, scheduled maintenance, the file system monitor where git has one, the untracked cache, packing loose objects), each turned on with one click. It also says what the graph's 100 000-commit limit means for this repository.",
            "Which configuration wins: each setting (pull, signing, line endings, …) with the value in effect and its scope and file; open one for every value git read, the one that wins on top. Remove a value from its file so the others apply again, set it for this repository, this worktree (with extensions.worktreeConfig), your global config or everyone on the machine (those asked first), and compare it across worktrees. Nothing credential-related is shown.",
            "Hidden changes: tracked files marked assume-unchanged or skip-worktree, which never show under Changes, what each flag means, and clearing those set by mistake, all or the ticked ones (asked first). Files sparse-checkout leaves out are listed apart: they are not flags anyone set.",
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
            "Tick Remember resolutions (above the conflict editor's result, or in the palette) and a conflict you resolved before is resolved the same way when it comes back (git rerere). The editor says so and shows the reused text under each conflict; nothing is staged until you save. Forget this resolution brings the conflict back.",
            "Note why… under a conflict keeps a reason and a test command with how it is resolved, in git. When the same conflict comes back, it is resolved the same way with the reason shown and how its test last went; if the code around it changed since, it is only offered, as stale. Test after saving runs the tests you wrote once the file is saved (Keep its note and test makes a decision's own yours). The palette gets or sends the decisions through the remote; the newer one wins.",
            "Right-click a file for its history, its blame or the whole file as it is at the commit.",
            "A file or hunk that makes the checks weaker gets a “weakens checks” badge: it removes assertions or tests, skips, disables or focuses tests, adds an assertion that can't fail, loosens a tolerance, rewrites a snapshot wholesale, lets CI fail or removes a CI job, or adds --no-verify. Hover the badge for the lines that made it.",
            "History of these lines: select lines in a diff (drag over them, or click changed lines) or in a file (click, Shift+click or drag), then right-click (Ctrl+C copies them). It lists every commit up to that version that touched them, followed back through earlier edits and renames; click one to see only what it did to them.",
            "Switch a diff from Diff to File to read the whole file; tick View all files under a commit's files to list every file in it.",
            "Ctrl+F in a diff, a file or a blame finds in it (in the graph it searches the commits): every match is marked, Enter and Shift+Enter (or F3 and Shift+F3) go to the next and previous one. Aa matches case, ab matches whole words only, .* takes the text as a regular expression.",
            "Copy link to … and Open … on GitHub (GitLab, Forgejo, Gitea) in the menu of a commit, a branch or a file, and of a commit's diff lines (right-click a line, or select text for a range) or a file's lines at a commit (click one, Shift+click for a range, then right-click). Links to a commit, file or lines use the commit's hash; a branch's follows the branch. They go to the remote the checked-out (or right-clicked) branch tracks, else origin, else the one you pick; the status line warns when the commit or branch isn't on that remote yet.",
            "Some files can be read in a way of their own, with a button in the diff's header (click it again for the lines; staging stays there). A click on a part goes back to the lines, at its line (or the nearest one shown), and a file that can't be read that way shows the plain diff with a note saying why.",
            "Prose (the book button, for .md, .adoc, .rst and .txt): paragraphs wrapped to the pane; in an edited one the removed words are struck through on red and the added ones on green, and a paragraph only wrapped differently says re-wrapped. Long runs of unchanged paragraphs fold; click to show them.",
            "By key (the list button, for JSON, YAML and TOML): each key path that changed, such as server.timeout: 30 → 60, keys added and removed, values that changed kind, and keys that only moved listed apart; a click goes to the key's line.",
            "As a table (for CSV and TSV): rows added, removed and changed in a grid, changed cells as old → new, renamed columns detected; .dvc files show the data they track, size and hash, before and after.",
            "Dependency changes (the package button, for Cargo.lock, package-lock.json, npm-shrinkwrap.json, pnpm-lock.yaml, poetry.lock, uv.lock, composer.lock and go.sum): packages upgraded, downgraded, added and removed, with links to the registry and to what changed, and changes of licence where the lockfile has them.",
            "API contract changes (the server button, for OpenAPI and Swagger files, or from the view by key): the breaking changes, such as endpoints removed, parameters newly required and responses narrowed, each with its endpoint; a click opens its place in the file.",
            "In a Markdown file's diff, the eye button renders it: its changes block by block, or with File the whole file.",
            "Sync (toolbar, or the command palette) pulls the current branch the way Pull is set to, then pushes it; if the pull fails (conflicts, or it can't fast-forward), nothing is pushed.",
            "A push the remote refuses because it has commits you don't have opens a banner: how many, and whether your branch was rewritten here (amended, rebased, undone) or both moved on, or is only behind. Pull, then push joins them; Force push (with lease)… names the commits it removes from the remote first, and pushes nothing if the remote (or your branch) moved since it was checked.",
            "A push a hook or a protected branch refuses shows git's message in full, with no force push offered. A branch on no remote yet, with several remotes, asks which one to publish it on.",
            "Review this branch in a branch's menu has the AI tool review what the branch adds to the branch pull requests go to (main, master or develop; the checked-out branch, for that one itself); its answer shows in a window, with Copy and Review again.",
            "Right-click a worktree and choose Collision matrix… to see which of the worktrees' branches would conflict when merged (merged in memory, nothing changed), and the order to land them in with the fewest branches to rework.",
            "Right-click a worktree and choose Agent fence… to list the paths agents must stay out of; they are written into each worktree's Claude Code deny rules (and, if ticked, .cursorignore and AGENTS.md). A linked worktree that touched one anyway (committed, changed, untracked or ignored files), or can't be checked, turns red; merging or rebasing onto a branch that touched one asks first.",
            "Right-click a linked worktree and choose Scope drift… to set what it changed against its task (or linked issue): folders the task doesn't mention, new dependencies, public signatures changed, the share that is formatting only. Click an item to open its files.",
            "When a diff pairs lines that have nothing to do with each other, pick another algorithm (Myers, Minimal, Patience, Histogram) in its header: the line at the top stays in view, the choice is kept for that file type, and the repository's config is left alone.",
            "Generated files (linguist-generated or -diff in .gitattributes), lockfiles and minified files start folded: at the end of file lists, and in their diff (reached from their history, say) behind a line such as “+1,204 −880, lockfile”. One click opens them; Settings → Git → Diffs adds patterns or turns folding off.",
            "Stacks: This branch is based on… in a branch's menu records the branch it is a layer on, the one history suggests picked first. STACKS in the left pane shows each stack, main → schema → api → ui, with the commits each layer adds, its pull request, and a warning on a layer whose parent moved on and needs restacking; “Changes x adds to its parent” in a layer's menu shows just its own changes, and a pill's tooltip in the graph names its stack. Renaming a branch takes the layers on it along.",
            "Restack: when a lower branch changes, STACKS says how many dependent branches need updating; Restack… previews each one's new base, then replays their own commits onto their parents' tips, parents first (a branch checked out in another worktree is rebased there). A conflict stops it: resolve, then Continue restack, or Abort restack to put every branch back. One undo puts every branch back too. A layer's menu has Restack the branches on it…, and the interactive rebase has Update dependent branches (--update-refs) for rebasing a stack from its top.",
            "Stacked pull requests: right-click a stack's title in STACKS (or Publish its stack's pull requests… in a layer's menu) to preview each one's base and patch, then push the branches and open them bottom-up, each into the branch below; run it again later to update them all (push, fix their bases). When a lower one merges, the ones above go into what it went into; one closed without merging is opened anew. Each layer's pull request is asked of the forge by its branch, so a publish that stopped halfway never opens one twice, and a branch is never pushed over a teammate's commits it doesn't have. GitHub, GitLab, Forgejo and Gitea.",
            "A picture's diff compares in 2-up, Swipe, Onion skin or Difference, the wheel zooming and a drag panning all of them alike (double-click to fit); the strip below is its history, a click opens a commit's change.",
            "A notebook's diff is cell by cell, outputs (pictures too) before and after; JSON shows its lines. Strip outputs when committing sets a clean filter in this clone only (nbstripout, else a Python 3 script): staged notebooks lose outputs and counts, the file on disk keeps them. Cells are never run.",
            "Syntax in a diff compares syntax with Difftastic (difft), installed where the repository is. Changed tokens stand out through reindentation and wrapping. Turn off Syntax to stage or see formatting edits; unsupported languages, parse errors and tool errors show the patch with a note.",
            "Right-click a pull request and choose Open in worktree to work on its branch in a worktree of its own, made for it when there is none.",
            "Start work from an issue (Ctrl+P, or the button on the pull requests' header) lists the open issues assigned to you on GitHub, GitLab, Forgejo or Gitea; pick one for a branch named after it (42-fix-login-crash) from the remote's default branch, here or in a new worktree. While it is checked out, a card above the graph keeps the issue's acceptance criteria (or its task list, or its description) in view as of the day it was read, and its pull request (merge request, on GitLab) starts with the issue's title and Closes #42.",
            "gitcraft remembers which version of a pull request you looked at. After the author pushes again (a force push too), its page says what changed, like “Rewritten since you last looked: 2 commits changed, 1 added”; click it to compare the two versions. In the list, “changed since you last looked” marks it, and its menu has What changed since you last looked and Mark as seen. A pull request's page opened for the first time, or its comparison once shown, counts as seen.",
            "Pull requests from forks show awaiting approval (Approve and run), can edit and first-time. Check out to push to its author adds their fork as a remote and checks the branch out so that Push goes back to it; Rebase onto main and push to them rebases it and force-pushes with a lease. Remove unused fork remotes (Ctrl+P) tidies up after.",
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
    (
        "When git fails",
        &[
            "A failure gitcraft knows shows a card under the toolbar (or on the start page) saying what happened and what to do; What git said holds git's own words.",
            "Its button fixes what can be fixed: Trust this folder (a repository owned by another user: safe.directory, that folder only), Remove stale lock (after saying how old the lock is and whether git is running), Enable long paths and Use Windows' certificates (Git for Windows).",
            "A fix first says exactly what it changes and in which config file, and runs only once confirmed; a config change has an Undo on the card. Try again runs what failed again, when it can.",
            "Line endings (core.autocrlf against .gitattributes), a broken transfer or a request too large (HTTP 413), unrelated histories, a push from a shallow clone and an SSH key refused are explained.",
        ],
    ),
    (
        "Maintainers",
        &[
            "Sign off in the commit box adds Signed-off-by with your name and email (DCO). It is on by itself in a repository with a DCO file or .github/dco.yml, and while a pull request whose DCO check fails is open; turning it the other way is kept for the repository.",
            "Where sign-offs are asked for, a commit without its author's says so under its author, and a pull request's page lists those the DCO check refuses under Sign-off (DCO). With its branch checked out, Sign off the branch's commits… adds your sign-off to each (rebase --signoff) and force-pushes with a lease, after showing what it will do; Ctrl+Z puts the old commits back, on the remote and here.",
            "The taskbar button shows how far a clone, fetch, pull or push is, and turns red (on Linux: asks for attention) when one fails while the window is in the background. Its menu (right-click it) lists the recent repositories, Clone… and Open….",
            "Fetch, pull, push and clone show how far git is (“Receiving objects: 45%”) and a thin bar under the toolbar; Cancel stops git and what it started, and leaves undo as it was. A cancelled clone removes what it had cloned.",
            "Activity (the list button in the toolbar) lists each operation with its git commands, how long each took, how it ended and all it printed; copy any of it. Click a message in the toolbar to open its operation there.",
            "Fetch, Pull and Push can be clicked while another operation runs: they start once it is done (up to three wait, in order; Cancel or a failure drops them). The other buttons say what they wait for.",
            "Offline mode (Settings → Git, or Go offline in the palette) stops gitcraft going on the network by itself: no background fetch, avatars, update checks or build and pull request polling, and Fetch, Pull, Push and the other commands that reach a remote say why they don't run. On a metered connection, or without one, only the background work pauses. The toolbar says so, with when the repository was last fetched.",
        ],
    ),
    (
        "Coding agents",
        &[
            "Start agent task… (the + of AGENTS, or the palette) makes a branch and a worktree, opens it and starts a terminal there running the agent you pick (Claude Code, Codex, Gemini CLI, Cursor Agent, Aider or a command of your own), with the task as its first message when it takes one.",
            "AGENTS on the left lists the tasks with their worktrees' uncommitted changes. Click one to open it and review its changes with the normal diff; its menu starts the agent again or opens a terminal there.",
            "Agent turns as review steps: Agent turns: save each turn in this worktree (palette), or Save each turn to review on its own when starting a task, sets Claude Code's and Codex's hooks to run gitcraft checkpoint after each turn (from the agent's next start). Uncommitted changes then lists AGENT TURNS named by their prompts: click one for what it changed; its menu compares it with the files now. Earlier sessions fold away; a session's menu deletes its turns.",
            "Hand it to the agent: click a line's number in a diff (Alt+click in uncommitted changes, where a click picks the line) to write a note for the agent; notes stay in this worktree until sent. They collect under the big area, and Send to agent types them into the agent's terminal (named after the agent, or the one picked) as one message, for you to send with Enter. Send to agent is also on a conflict (its three sides), a failing CI run's log and a pull request's thread.",
            "Bake-off: compare attempts… (palette, or Compare them under AGENTS) puts 2–4 worktrees or branches that tried the same task side by side: the files each touched, its size, the dependencies it adds and whether it builds (your command, kept for the repository, in a terminal of each worktree); a worktree's uncommitted work counts. Click a file to see it across them; take it whole (click again to take it back) or tick some of its hunks from any of them, and Assemble makes a new branch from the base with that, checking nothing out. Hunks of two attempts on the same lines are flagged and refused.",
            "Worktree setup… (palette) is a recipe that runs in every worktree gitcraft makes for the repository, for an agent or not: it copies or links files from the main worktree (.env.local, node_modules), sets variables (PORT=$((3000 + GITCRAFT_WORKTREE_INDEX)): the main worktree is 0, each other keeps the number it was given) and runs a setup command, in a terminal where its output shows. It runs with sh on the repository's machine, WSL and SSH included. A step that fails says so, and then neither the setup command nor the agent runs.",
        ],
    ),
    (
        "Packages",
        &[
            "What a package will publish (from the palette, Ctrl+P) lists the files cargo package --list or npm pack --dry-run --ignore-scripts would ship for each crate of the workspace and the top package.json, what looks shipped by accident (.env, keys, logs, build output), what a package usually wants and leaves out (its README, LICENSE, main file), and the files new or gone since the version on crates.io or npm.",
        ],
    ),
];

/// The "Did you know" tips under the graph: features above, each said in one short line of its
/// own (`Ctrl` as written in them). Keep them in step with [`FEATURES`].
const TIPS: &[&str] = &[
    "Double-click a branch pill to check that branch out.",
    "Ctrl+click a second commit to list the files that differ between the two.",
    "A branch's menu has “Changes x adds to main”: what it changed, like a pull request.",
    "Hover a branch and click its eye to hide it from the graph; Solo shows only that one.",
    "Right-click a commit or a pill for merge, rebase, cherry-pick, revert, reset and more.",
    "Search can look in the code: switch it to Code to find the commits that added some text.",
    "Right-click the graph's column header to pick its columns or make the graph compact.",
    "In a diff, click the lines you want to stage, unstage or discard only those.",
    "F7 and Alt+↓ go to the next change of a diff; Shift+F7 and Alt+↑ to the previous one.",
    "Double-click a commit's message in the right pane to edit it.",
    "Ctrl+Z undoes commits, merges, rebases, resets, checkouts, branches, tags and stashes.",
    "Ctrl+P opens the command palette: every action, and every branch to check out.",
    "Right-click a file for its history, its blame or the whole file at that commit.",
    "Switch a diff from Diff to File to read the whole file.",
    "Conflicts open in a three-pane editor with Ours, Theirs and Both for each one.",
    "Profiles each have their own tabs, theme, git author and commit signing.",
    "Right-click a tab to give it its own icon and highlight colour.",
    "Ctrl+Shift+T reopens the tab closed last.",
    "Remote on the start page opens a repository in WSL or over SSH.",
    "A pull request's menu has Open in worktree, to work on it in a folder of its own.",
    "Interactive rebase reorders, rewords, squashes or drops commits.",
];

/// The "Did you know" tips, as written (with `Ctrl`).
pub(crate) fn tips() -> &'static [&'static str] {
    TIPS
}

/// Keyboard and mouse shortcuts of the views, besides the main actions' keys in
/// [`crate::keymap`]: (keys, what they do). `Cmd` stands for Ctrl outside macOS.
pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Keyboard navigation",
        &[
            ("Alt+← / Alt+→", "Back / forward through recent views"),
            (
                "Mouse back / forward",
                "Back / forward through recent views",
            ),
            ("Tab / Shift+Tab", "Next / previous button, row or field"),
            ("Enter / Space", "Click the focused button or row"),
        ],
    ),
    (
        "Start page",
        &[
            ("Type", "Filter the recent repositories"),
            ("↑ / ↓", "Pick a recent repository"),
            ("Enter", "Open the one picked"),
            ("Esc", "Clear the filter"),
        ],
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
                "Alt over Rebase, Reset hard, Pull…",
                "Preview the result in the graph (ghost run)",
            ),
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
            (
                "Click / Shift+click",
                "Pick lines in a diff or a file, or a range of them",
            ),
            (
                "Click / Shift+click",
                "Pick lines of a file to link to (View file, or File in a commit's diff)",
            ),
            ("Cmd+click a file", "Pick it to stash (or unpick it)"),
            ("F7 / Shift+F7", "Next / previous change in a diff"),
            ("Alt+↓ / Alt+↑", "Next / previous change in a diff"),
            ("Cmd+Z", "Undo"),
            ("Cmd+Y", "Redo"),
            ("Cmd+Shift+Z", "Timeline and undo tree"),
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
