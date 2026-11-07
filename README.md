# gitcraft

A visual git client: a simplified yet powerful take on the usual graph-based git GUIs.
Rust + [egui/eframe](https://github.com/emilk/egui), the same technical stack of the ArtCraft suite, so it builds on Windows, Linux and macOS and stays light and fast. It drives the installed `git` command line,
so your config, credentials and hooks apply as usual.

![gitcraft: branches, remotes and worktrees on the left, the graph with branch pills and relative dates, uncommitted changes on the right and the integrated terminal below](docs/screenshot.png)

## Features

**Repositories and tabs**

- Start page: checks that `git` is installed; open, create (`git init`) or clone a repository;
  recent repositories, each with its branch, a dot for uncommitted changes and when it was last
  opened (read in the background). Type to filter them, ↑ / ↓ and Enter to open one.
- Drop a folder on the window to open it, or a `.patch` / `.diff` / `.eml` / `.mbox` on a
  repository tab to apply it.
- _Scan a folder for repositories_ finds the repositories a few folders down and lists them to
  open or add to the recent list.
- Workspaces on the start page: named groups of repositories (here, in WSL or over SSH). Each
  shows every repository's branch, uncommitted files, ahead / behind and last fetch, looked at in
  the background a few at a time; _Fetch all_ and _Open all_ work on the whole group.
- _Where's my work?_ (start page or palette): one panel over every repository gitcraft knows, on
  every machine, listing commits not pushed, repositories with no remote, stashes, uncommitted
  changes (in any worktree) and branches whose remote branch is gone. Each repository has
  _Open_; a branch has _Push_ when it goes to a remote branch of its own name. One round trip per
  repository, a few at a time; it can also search a machine's home folder (here, WSL
  distributions, `~/.ssh/config` hosts) for repositories gitcraft doesn't know yet.
- A pull-request inbox on the start page: across every repository opened and every forge (GitHub,
  GitLab, Forgejo, Gitea), the pull requests waiting for your review, and yours with failing checks
  or comments you haven't seen. A click opens one in its repository's tab (a GitLab merge request
  in the browser). It uses the logins gh and glab have; for Forgejo and Gitea, one from the
  environment, tea's, or one gitcraft already used: it never runs git's credential helper unasked.
- Clone options: shallow history with a chosen depth, blobless partial clones, and selected
  folders (one path per line). With blobless and sparse combined, only checked-out file
  contents download initially. In an open
  repository, **Sparse checkout…** in the palette offers a folder tree, or a return to a full
  checkout. Root files and files immediately inside parent folders are always included. It
  refuses uncommitted, untracked or ignored files and asks before replacing custom non-cone
  patterns. Move ignored files outside the working tree first; Git can delete them during
  sparse changes, even when an ordinary status looks clean.
  Partial clones show a download notice; opening a historical file or diff whose contents
  are missing asks before Git downloads them. The server must support partial clones.
  These options run on the repository's machine, including WSL and SSH.
- Browser-like tabs, one per repository (Ctrl+T, Ctrl+W, middle-click; Ctrl+Shift+T reopens a
  closed one; Ctrl+Tab / Ctrl+Shift+Tab go round them, also on macOS; Ctrl+1 … 8 and Ctrl+9 for
  the last pick one, Cmd on macOS). Open tabs, recents, theme and settings are remembered.
- Keys for the main actions of a repository tab: Fetch (Ctrl+Shift+F), Pull (Ctrl+Shift+L), Push
  (Ctrl+Shift+K), Stash (Ctrl+Shift+S), New branch (Ctrl+B), Terminal (Ctrl+J), Stage all
  (Ctrl+Shift+A) and Commit (Ctrl+Enter). Cmd on macOS. None of them fires while a text field or
  the terminal has the keyboard (Ctrl+Enter in the commit message still commits). The toolbar's tooltips, the palette and the context menus show
  each key, and a greyed-out menu entry says why under it.
- Profiles, as in GitKraken: switch between them from the title bar (or the palette). Each has
  its own open tabs, recent repositories, theme, git author and commit signing (Settings →
  Profile); the other settings pages are shared. Switching closes this profile's tabs and opens the
  other one's (refused while a tab has unfinished work).
- One window: `gitcraft <folder>` or _Open in gitcraft_ while gitcraft is running opens that
  folder in a tab of the running window (or switches to its tab) and brings it to the front; a
  bare `gitcraft` just brings it to the front. Where the system doesn't let a window come to
  the front by itself (Windows, Wayland), its taskbar button flashes instead. Two windows no
  longer save their tabs over each other's. The handoff goes over 127.0.0.1 with a random
  token kept in a file of the data folder only you can read. Every build shares that folder, so
  `cargo run` while an installed gitcraft runs goes to that window: close it first.
- Changes made elsewhere (editor, terminal) show up by themselves: the folder is watched.
- What you typed survives a crash or a restart: the commit message, unsent pull-request
  comments, reviews and replies, and an interactive-rebase list being set up are saved per
  repository every few seconds and when the window loses the focus, and come back when the
  repository is opened again (Amend is not ticked again: the last commit may be another one by
  then). Each goes once it is used (committed, sent, the rebase started or cancelled) or
  emptied. After a crash, the next start says _gitcraft closed unexpectedly: your drafts and
  tabs were restored_. They are kept in gitcraft's data folder (never in the repository), each
  file written whole or not at all; a box keeps up to 256 KB, and drafts untouched for three
  months are let go of. Nothing typed into a password or sign-in prompt is saved.
- A repository with no commits shows its first steps in the graph: _Make the first commit_
  (the changes open at the commit summary, or an empty first commit, undoable, when there is
  nothing to commit), _Add a remote_ and _Publish_ (push and track the branch), those done
  ticked off. They stay under the first commit until it is published or a second commit is
  made; _Hide these steps_ puts them away for the tab.
- Tips: a "Did you know" line under the graph, a different tip each day (the arrow shows the
  next one; the eye turns tips and hints off, Settings → General → Tips turns them back on), and
  a one-line hint, once ever, the first time you do something with a shortcut nearby (selecting
  a commit: Ctrl+click another to compare; clicking a pill: double-click checks it out; hiding a
  branch: Solo; opening a diff: F7 / Alt+↓).
- Lazy tabs: tabs restored on start read the repository and watch its folder only once they are
  first shown, so many saved tabs don't mean as many history loads and folder watchers at
  startup (nor background fetches until then). A tab left in the background for 30 minutes with
  nothing going on (no operation, running terminal, typed message, open editor, unsent review,
  pull request or builds view) drops its graph, diff and commit details and stops watching,
  fetching and following builds; shown again, it reads afresh (the opening screen meanwhile),
  then selects the same commit and opens the same diff if they are still there.
- Repositories on another machine, in a WSL distribution or over SSH: the window stays here and
  git runs there (see below).

**From the terminal and the file manager**

- `gitcraft .` (or `gitcraft <folder>`) opens the repository that folder is in, from any
  subfolder or linked worktree, also a `\\wsl.localhost\…` folder.
- `gitcraft log <file>` opens a file's history (also of a file deleted since), `gitcraft blame
  <file>:<line>` its blame scrolled to and marking that line (`gitcraft blame -- <file>` takes a
  name ending in `:12` as it is), `gitcraft show <rev>` selects a commit of the current folder's
  repository, and `gitcraft compare a..b` lists what differs between two commits (`a...b`: what
  `b` changed since they diverged). `gitcraft --help` lists them; a revision that starts with `-`
  is refused.
- _History in gitcraft_ and _Blame in gitcraft_ on files: the Windows installer adds them to
  Explorer's right-click menu (an option, like _Open in gitcraft_ on folders); the .deb and .rpm
  packages add a Dolphin service menu (`packaging/linux/gitcraft-servicemenu.desktop`, also in
  the `.tar.gz`); Settings → General adds them to Files (GNOME) as scripts and to Finder as
  Quick Actions (they may show after logging in again). Each starts gitcraft, which hands the
  file to the window already open. No overlay icons.
- Links from the browser: `gitcraft://open?url=<forge link>` opens that pull request, commit or
  file (`#L40` scrolls the blame to the line) in your clone of the repository, found by its
  remotes' URLs among the open tabs and the recent repositories; with none, the clone dialog
  opens with the repository's URL (clone, then open the link again). A link to a pull request opens in a clone whose `origin` is that repository (a fork's `upstream` isn't where its pull requests come from); a file link opens at the linked commit when the clone has it, else as the clone has the file, and a branch with `/` in its name is told from the folders by what the clone has. The link itself pasted into the palette (Ctrl+P) or the start
  page's field, or given as `gitcraft <link>`, does the same. GitHub, GitLab (groups too),
  Gitea / Forgejo (Codeberg) and Bitbucket links are read by their paths; every link is checked
  (https or http only, a host name with a dot such as `git.example.com`, so not `localhost`, no `..`, a revision that can't be an option) and
  anything else is refused with why. The Windows installer registers the `gitcraft://` scheme
  (an option), the .deb / .rpm install `gitcraft-link.desktop` (`x-scheme-handler/gitcraft`),
  and the AppImage's menu entry adds one. macOS hands a link to an app as an Apple Event, which
  the windowing library doesn't pass on, so there paste the link instead.
- Without a terminal (a menu, a file manager, Windows), what the command line says (`--help`,
  a mistake) shows in a dialog.

**Remote repositories (WSL, SSH)**

- Start page → _Remote_ (or the palette's _Open remote repository_): pick a WSL distribution
  or an SSH host (the hosts of `~/.ssh/config` are offered), and the repository's folder there
  (`/home/me/repo` or `~/repo`), typed or found with _Browse…_, which lists that machine's
  folders and marks its repositories. A `\\wsl.localhost\…` folder chosen with _Open_ is a WSL
  repository too, and `gitcraft ssh://host/path/to/repo` opens one from the command line.
- The window never waits for that machine: everything asked of it is asked in the background,
  and a slow or unreachable machine only shows as work still going on (anything asked of it
  from the window's thread is refused rather than run, and a test drives the window against a
  WSL repository to keep it so).
- git, and every file read or written in the repository (diffs of the working tree, staging
  lines, the conflict editor, `.gitignore`, rebase todo lists), runs on that machine, with its
  own git, config, credentials and hooks. Nothing has to be installed there but git and a POSIX
  `sh` with the tools any Linux, BSD or macOS has: `mktemp -d` (each shell's temporary
  folder), `base64 -d` (what a command reads, a patch or a file's new contents), `cat` and
  `wc` (passing output back), `stat` (GNU or BSD: noticing changes), and `ps` with `awk`
  (stopping a command that hung, and all it started).
- A few shells per machine are kept open (`ssh -T host sh`, `wsl.exe -d distro --exec sh`), so a
  command costs one round trip, not a new connection; history is read page by page in them
  too. SSH signs in as it does in a terminal (keys, an agent, `~/.ssh/config`;
  `GITCRAFT_SSH_CONFIG` names another config): what it asks — a password, a key's
  passphrase, whether to trust a host seen for the first time — is asked in the window (ssh
  runs gitcraft as its `SSH_ASKPASS`). A password or passphrase typed is kept for that machine
  while gitcraft runs, so the other shells don't ask again, and forgotten as soon as ssh says
  it was wrong. Tick **Remember connection** to keep it after a restart too: once ssh has
  signed in with it (a wrong one is never saved), it is saved encrypted in the system's
  credential store (Windows Credential Manager, the macOS Keychain, the Secret Service on
  Linux), never in gitcraft's own files, and forgotten when it stops working or with
  **Forget saved SSH sign-ins** in the command palette (or Settings → Git).
- Names with a `\` in them (Linux allows one) work like any other.
- Network operations (fetch, pull, push, clone) run in processes of their own, so the reads
  never wait behind them, and show git's progress as they go. A command that hangs is stopped
  there, with what it started, after 5 minutes (30 for network operations); _Cancel_ stops one
  at once the same way.
- Changes made there show up by themselves: the repository is looked at every few seconds
  (every 15 while the window isn't focused).
- The terminal opens a shell on that machine, in the repository, whatever the login shell
  (bash, zsh, fish, csh…). Pull requests and builds use `gh` / `glab` of this machine, told
  which repository, and only for github.com, gitlab.com or a host they're signed in to.
  Forgejo and Gitea are asked from this machine too, with this machine's login for the host.
- Over SSH, programs of this machine can't open the files (reveal in the file manager, open in
  editor, Mergiraf). In WSL they can, through `\\wsl.localhost`.
- Settings that write this machine's global git config (commit signing, Mergiraf) don't apply
  there, nor do a profile's author and signing: that machine's own git config does (the commit
  box says `· global (SSH · host)`).
- **Teleport** (a tab's right-click menu, or the palette's _Teleport to another machine…_):
  carries the tab's work to another machine — this one, a WSL distribution, an SSH host — where
  it arrives in a new worktree, a tab opening on it: the branch with its commits, the staged
  changes still staged, the unstaged ones still unstaged, and the untracked files (not the
  ignored ones). No push, forge or cloud is involved, and nothing of the source is changed: a
  snapshot is taken through a temporary index (one tree for the index, one for the working
  tree), sent as a `git bundle` of only the commits the other machine lacks, through the shells
  already open to it, checked there (`git bundle verify`, and every object as it is unpacked)
  and then checked out. The work goes in a clone of the same project found there (by its
  remotes' URLs, among the recent and open repositories), another repository named, a clone
  made first (the work then goes in its own working tree), or a new repository made from the
  bundle. A bundle over 100 MB asks first. Submodules, uncommitted files kept in Git LFS, files
  marked assume-unchanged, a shallow clone, a branch checked out there already (or there with
  commits this one lacks) and a merge or rebase in progress are refused, with the reason. What
  it made (temporary refs, index and bundle, a half-made worktree, a clone made for it) is
  removed again, also when it fails or is cancelled.

**Graph**

- Every branch, coloured lanes, ↑/↓ to move. History loads 1500 commits at a time as you scroll.
- Branch and tag pills in their own column left of the graph, on each branch's tip only, joined
  to their commit: white text, a laptop icon when the branch is local, a cloud when a remote has
  it at that commit (`x` and `origin/x` make one pill), a pull-request icon when one is open,
  `+N` when more refs share the commit, the author's avatar and how long ago it was ("2 hours
  ago"). Author, date and hash columns are optional (Settings).
- Drag the column borders in the header to resize. Right-click the header to pick the columns,
  make the graph column compact (lanes closer together), or _Reset columns_ to their first
  widths.
- Click _GRAPH_ in the header to order history topologically, by commit date (the default) or
  by author date, and to show _First parent only_: a merge without the commits it brought in,
  so main reads one merge per pull request. Kept per repository; the header names the order
  when it isn't the default.
- While HEAD is detached, a banner says so in plain words, with _Create branch here_ and _Back
  to main_ (the branch it was on). Checking out something else from the app after committing
  there stops first, lists the commits that would be left on no branch, and offers to keep them on a new
  branch.
- The checked-out branch's pill is solid, the others a darker shade. Double-click a pill to
  check that branch (or tag) out; hover `+N` to list every pill of the commit and double-click
  the one you want.
- Each commit's summary, followed by the rest of its message, faded and cut to fit.
- An _Uncommitted changes_ row on top while the working tree is dirty.
- AI authorship, as agents record it, never guessed from the code: git-ai's line attribution
  (`refs/notes/ai`), Agent Trace records (`refs/notes/agent-trace`), an agent's own account as
  the author, or an agent's own identity in a `Co-authored-by:` trailer (its address, or its
  name with a no-reply or bot address: a person called Claude is a person). A robot marks
  agent-written commits on their row (amber when co-written), with where that comes from on
  hover; _Agent-written_ in the search bar steps through them. Blame and a commit's diff get a
  thin stripe per line: blue for an agent, amber for both, grey for a person, none where nothing
  is recorded. A branch's menu → _How much agents wrote_ says, of the lines it added since it
  left the default branch, e.g. _71% agent-written (412 of 580 lines added in 9 commits), 9% of
  that since edited in commits no agent is recorded for_ (files moved don't count as edits).
- Search (Ctrl+F) through messages, authors and hashes of all history, or switch to _Code_ to
  find the commits that added or removed a piece of text in a file (`git log -S`); Enter /
  Shift+Enter jump between results, loading older history when needed.
- Search operators, combined with each other and with free text: `author:`, `committer:`,
  `path:`, `after:` / `before:` (`2024-05-01`, `2024-05`, `today`, `yesterday`, `12h`, `3d`,
  `2w`, `6m` for months, `1y`), `message:`,
  `contents:` (`git log -S`), `regex:` (`git log -G`) and `merges:` / `-merges:`; quote values
  with spaces (`author:"Jane Doe"`). Typing one suggests authors (as `.mailmap` names them),
  files and folders, or dates (↑ / ↓, then Tab or Enter); a mistake is explained under the
  field.
- Right-click a commit: create a branch, a tag or a worktree on a new branch there, check it
  out, interactive rebase onto it,
  cherry-pick, revert, reset the current branch to it (soft / mixed / keep / hard, confirmed with what it changes;
  _Keep_ drops the commits after it but keeps uncommitted changes, and stops with git's message
  if a file with changes would be touched), the menu
  of each branch on it, copy the hash, message or branch name. Right-click a branch's pill to get
  check out, fast-forward, merge, rebase and interactive rebase for that branch on top.
- Compare: Ctrl+click a second commit (or _Compare with…_ in the commit menu, the changes row
  standing for the working tree) to list the files that differ between the two and open each
  diff. On a branch, _Changes x adds to main_ shows only what it changed since the two diverged,
  as a pull request shows it, and _Compare main with x_ every difference; a checkbox switches
  between the two. The right pane says how many commits each side has that the other hasn't.
- What a merge resolved: on a merge commit, _What this merge resolved_ lists only what differs
  between the merge git would have made of its parents and the one recorded
  (`git show --remerge-diff`): the conflicts resolved, then anything changed beyond that, by
  hand (an "evil" merge) or by a merge made with other options or an older git. Each opens in
  the diff view, git's conflict message on top and every hunk marked _resolves a conflict_ or
  _edited beyond the merge_; a clean, unedited merge says so. Merge rows of the graph that
  resolved something carry a _resolved_ / _edited_ badge, read in the background for the rows
  on screen only, a few at a time (not in a repository with merge drivers of its own, which
  redoing a merge would run); a click shows what it resolved. Needs git 2.36 or newer where
  the repository is (otherwise the box is greyed out and says why); git doesn't redo octopus
  merges, which the box says too. Redoing a merge writes temporary objects in `.git/objects`,
  so it can't be done in a repository that is read-only for you.
- Patches: _Create patch from commit…_ saves a commit as a file (`git format-patch`). _Apply
  patch…_ (the latest commit's menu, the changes row's menu or the palette) turns one from
  `git format-patch` into commits (`git am`) and puts any other diff on the working tree
  (`git apply`). Ctrl+Z undoes either.
- Interactive rebase: the commits to rewrite, newest on top; pick, reword (edit the message in
  place), edit (stop there to change the commit; _Continue rebase_ amends it with what is staged
  and goes on), squash, fix up
  or drop each one, then _Start rebase_. Drag a row by its handle to move it, or click a row and
  use the keys: P, R, E, S, F, D set its action, ↑ / ↓ select another, Alt+↑ / Alt+↓ move it.
  Ctrl+Z undoes it.
- Bisect: _Start bisect here…_ in a commit's menu (or the palette) marks it good (HEAD is then
  the bad end) or bad, and checks out commits between the two for you to test. A banner over
  the graph names the commit to test and about how many steps are left, with _Good_, _Bad_,
  _Skip_ and _Stop_; the graph tints good, bad and skipped commits. _Run a command…_ has `git
  bisect run` test each commit on the repository's machine, its output streamed under the
  banner and stoppable. At the end the first bad commit is selected and _Finish_ (or _Stop_,
  which asks first) checks out again what was checked out when it began. A bisect started in a
  terminal is picked up and goes on from the banner. Refused while there are uncommitted
  changes or conflicts, or a merge, rebase, cherry-pick or revert is in progress.
- Parallel bisect: _Run in parallel…_ in the bisect banner (or _Bisect in parallel from
  here…_ in a commit's menu) tests k commits at once, each in its own worktree on the machine
  chosen (this one, a WSL distribution or an SSH host), so it takes about log₍ₖ₊₁₎(n) rounds
  instead of log₂(n). The test command's exit code is read as `git bisect run` reads it; k
  defaults to that machine's cores (at most 16). The ends are tested in the first round (_the
  good end already fails_), the graph tints results as they come and the range still in
  question visibly shrinks; a tree tested before isn't tested again, and a test past its time
  limit is stopped and skipped. The checkout isn't touched; Cancel stops every test and removes
  the worktrees.
- _Split commit…_ (a commit's menu, for commits of the checked-out branch): hand its files,
  hunks or lines out to two or more new commits (up to 9), each with its own message. _Check
  split_ makes the new commits without touching the working tree and shows each one's patch;
  _Split commit_ puts them in place of the old one (together they are exactly the old commit) and
  replays the commits after it. Ctrl+Z undoes it. A merge, merges after the commit, and the
  first commit are greyed out with why.
- _Move changes to another commit…_ (same menu): click the files, hunks or lines to take out of
  the commit, then pick where they go: another commit of the branch (newer or older), or the
  working changes, staged. _Check move_ shows what stays in the commit and what moves;
  _Move changes_ rewrites the branch from the older of the two (a commit left with nothing is
  dropped). Ctrl+Z undoes it; a conflict stops the rebase as usual.
- Test lane: _Test commits up to here…_ in a commit's menu (or _Test commits…_ in the palette)
  runs a command such as `cargo test` at each commit from the base branch, on the repository's
  machine, in scratch worktrees in its temporary folder, several at once and in the background.
  A pass / fail square by each commit (with a tick or a cross) fills in as results come; click
  one for what the test printed. Results are kept by tree in git notes in git-test's format
  (`refs/notes/tests/default`, `test.default.command`), so `git test` and gitcraft share them
  and a rebase that keeps a commit's files keeps its result.
- Rehearse: _Rehearse merge or rebase…_ on a branch plays a rebase onto it, or a merge of it,
  through in a scratch worktree and reports where it stops (_stops at commit 3 of 5, 1
  conflict_), which files conflict and whether the test command passes on the result. The
  merge and the rebase can be rehearsed side by side; _Rebase now_ / _Merge now_ checks that
  neither branch moved meanwhile, then does it as usual, undoable with Ctrl+Z.
- _Distribute staged fixes…_ (above the staged files, or the palette): each staged change goes
  into the earlier commit of the branch that last changed those lines, as git-absorb does. A
  change whose lines one commit last changed is matched; one several commits changed is only
  suggested (the newest); one no commit of the branch changed (a new file, an older line) stays
  staged. Change any of them, _Check_ the fixups and what stays staged (a fix a later commit
  would conflict with is refused there), then absorb: the commits are made again with their
  fixes (signed when your config signs), and the index and working tree stay exactly as they
  are. Ctrl+Z undoes it. With
  `git-absorb` installed (on the repository's machine for WSL / SSH), _Run git-absorb instead_
  hands it over.
- Find the interaction: two branches pass alone but fail together. _Find the interaction…_ on a
  branch tests subsets of its commits and the checked-out branch's, applied where the two
  diverged, in a scratch worktree (delta debugging, `ddmin`), until it can say _these two
  commits fail together; either alone passes_. It has a test budget, and re-runs a failure
  before believing it, setting flaky results aside.
- Quick edits from a commit's menu, for commits of the checked-out branch: _Edit commit
  message_, _Drop commit_ (which asks first), _Move commit up_ / _down_, each a rebase without
  the editor and undone with Ctrl+Z. What can't be done (a merge, merges after the commit) is greyed out with why.
- Drag and drop: drag a branch's pill (or its row in the left pane) onto another pill, a commit
  or a branch in the left pane, and a small menu offers what makes sense there: _Merge x into
  main_, _Rebase main onto x_, _Interactive rebase main onto x…_, or _Fast-forward_ when one
  branch is only behind the other, which also works for a branch that isn't checked out. Drag a
  commit onto the checked-out branch to cherry-pick it, and a commit of the checked-out branch
  onto another commit of that branch to move it right above or below that one, or squash or fix
  it up into it. The targets that take what is dragged light up, the graph scrolls near its
  edges, Esc cancels, and each entry runs the same operation as the right-click menu, undone
  with Ctrl+Z.
- Ghost run: rest the pointer on a rebase entry (_Rebase main onto x_), the hard reset of
  _Reset main to here_, _Pull_ (the toolbar button or a mode in its menu), _Force push to
  origin/x (with lease)…_, _Delete…_ / _Delete locally and on origin…_ or _Drop commit_ (hold
  Alt to skip the wait), and the graph draws the result before anything is done: the commits it
  would make on top, faint with dashed lanes, the pills where they would point outlined, the
  commits it would leave on no branch crossed out with their lanes faded. A strip over the foot
  of the graph lists those commits with their authors, the files of the working tree that would
  change, and the step that would conflict. It is worked out in the background without touching
  refs, index, working tree or reflog: `git replay` (git 2.44+) or `git merge-tree` (2.40+;
  merges from 2.38) in memory, or a scratch clone in the temporary folder on an older git. A
  hover never goes to the network: a pull is shown as of the last fetch. _Force push to origin/x (with
  lease)…_ is in a local branch's menu when its upstream has commits it hasn't.
- Compare versions (`git range-diff`): two versions of a series of commits side by side, the
  old one on the left and the new on the right, paired up. Unchanged commits fade; changed,
  added, dropped and moved ones stand out, and a commit whose patch became empty says so. Click
  a pair to see how its patch changed (the diff of the two diffs, coloured like a diff), or the
  whole patch of a commit only one side has.
- Before / after: every rewrite gitcraft makes (rebase, interactive rebase, quick edits, amend,
  pull with rebase, a reset; a rebase continued after conflicts too) keeps the branch's old
  commits under `refs/gitcraft/rewrites/`, so `git gc` can't take them and the graph doesn't
  list them. _Compare before / after_ beside the message saying it's done, _Compare before /
  after the last rewrite_ in the branch's menu, or the palette compares the two. A branch that
  has diverged from its upstream has _Compare commits with origin/x_. Rewrites are kept for 30
  days, the newest 20 of each branch.

**Branches, remotes, worktrees, stashes, tags** (left)

- Local branches with ahead/behind; right-click: check out (or in a new worktree), fast-forward,
  merge into / (interactively) rebase onto the current branch, push, create a pull request,
  rename, delete (here, or here and on the remote in one go)…
- _Pin to top_ (a branch's menu) keeps a branch at the top of _Local_; _Name_ / _Recent_ in the
  panel's header sorts the others by name or by their last commit, newest first. Both are kept
  per repository.
- _merged_ and _stale_ marks on the branches whose work is in the default branch (merged,
  rebased or cherry-picked, or squash-merged by a pull request into it) or that had no commit
  for 90 days (Settings → Git), next to _gone_ for an upstream deleted on the remote. The brush
  in the panel's header (or _Select stale or merged branches…_ in the palette) lists them, and
  those gone, with why; only the merged ones start ticked. It deletes the ticked ones here, and
  if asked their branches of the same name on the remote, in one go that Undo brings back, after
  checking that nothing would be refused. It never offers the checked-out branch, the default
  one, or one checked out in a worktree.
- Hide a branch from the graph with the eye on its row (on hover), or _Solo_ some from their
  menu to see only those; the graph then has just the commits of the branches it shows and the
  checked-out one. _Viewing 12 of 55_ at the top says how many it shows, with _Show all_. Kept
  per repository.
- Stacked branches: _This branch is based on…_ in a branch's menu records the branch it is a
  layer on (`branch.<name>.gitcraft-parent` in the repository's git config, with the parent's
  commit it was based on as `gitcraft-parent-base`), the one history suggests picked first: the
  local branch it forks from last. _STACKS_ then shows each stack as `main → schema → api → ui`,
  every layer with the commits it adds, its pull request and a warning when its parent moved on
  (amended, rebased, new commits) and it needs restacking; a branch pill's tooltip in the graph
  names its stack. _Changes api adds to schema_ in a layer's menu shows only its own changes.
  Undoable; _Remove from its stack_ takes a branch out, and renaming a branch takes the layers on
  it along.
- Restacking: after a lower branch changes (amended, rebased, new commits), STACKS says _3
  dependent branches need updating_; _Restack…_ (or _Restack the branches on it…_ in a branch's
  menu) previews each one's new base and how many commits move, then replays them in order with
  `git rebase --onto <parent's tip> <its old base>`, the old base being the parent's commit it was
  recorded on. A branch checked out in another worktree is rebased in that worktree (which git's
  `--update-refs` would skip). The others are rebased here, which needs this worktree clean;
  what you had checked out is checked out again at the end. A conflict stops it, kept in the repository's config
  (`gitcraft-restack.*`), so _Continue restack_ (after resolving) or _Abort restack_ (every
  branch back, with its recorded base) works after a restart too. One Ctrl+Z puts every branch
  it moved back. The interactive rebase has _Update dependent branches_ (`--update-refs`, git
  2.38+) for rebasing a stack from its top in one go.
- Stacked pull requests, on GitHub, GitLab, Forgejo and Gitea alike: right-click a stack's
  title in STACKS (or _Publish its stack's pull requests…_ in a layer's menu). Each layer's own
  pull request is asked of the forge by its branch, whatever its state (never a fork's branch of
  the same name). The preview shows each one's base (the layer below; the bottom one goes into
  the branch the stack is on), its title (editable), its commits and `--shortstat` with a button
  for its changes in full, and whether it will be opened, retargeted or just pushed. _Publish_
  pushes each branch bottom-up (`--force-with-lease --force-if-includes`, git 2.30+: a restacked
  branch goes over its old commits, never over a teammate's fetched but not taken in) and opens
  the missing ones, each description naming the one it depends on, optionally as drafts. Publish
  again later to update them all: pushed again, and any whose base is wrong pointed at the right
  one. When a lower one is merged, it is left alone and the ones above go into what it went
  into, also as recorded in git's config (restack them afterwards); one closed without merging
  is opened anew. A layer with nothing of its own is pushed, with no pull request. Each one's
  number is recorded on its branch (`branch.<name>.gitcraft-pr`) as soon as it is known, and a
  publish that failed halfway is run again without opening any twice.
- Remotes: add, edit (name and URL), fetch one, remove; their branches below each.
- Worktrees: a yellow dot in place of the folder icon marks the others with uncommitted changes,
  here and in the switcher. Add (existing or new branch), open in the same tab (the tab then shows the branch, a searchable switcher and a button to close the worktree, and what you left keeps running; can be turned off in Settings) or a new one, open a terminal there, lock, remove,
  prune.
- Collision matrix (a worktree's menu → _Collision matrix…_): for branches worked on side by side,
  by agents in worktrees or otherwise, every pair is merged in memory (`git merge-tree`, git 2.38
  or later: nothing is checked out, no branch moves) and each with the branch they land on. The
  branches of the linked worktrees are ticked; tick others, or pick another branch to land on.
  Each cell says how many files the pair conflicts on (hover for them, click to compare the
  two; `?` when a pair couldn't be merged in memory, and why). Below it is the order to land
  them in with the fewest branches to rework: as many as merge cleanly with each other and the
  target first, then the rest, fewest files to resolve first. It is a prediction pair by pair;
  Esc puts the window away and the menu brings it back as it was.
- Agent fence (a worktree's menu → _Agent fence…_): one list per repository of the paths agents
  must stay out of (secrets, `.env`, CI workflows, migrations, lockfiles…), in `.gitignore`'s
  syntax without `!` exceptions, any case, kept in the repository's git config
  (`gitcraft.fence`, shared by its worktrees). It is written into every worktree's Claude Code
  deny rules (`.claude/settings.local.json`, kept out of git through `info/exclude`), also into
  worktrees added later, and, when ticked, into a block of `.cursorignore` and a section of
  `AGENTS.md` for Codex (files a repository usually tracks, so they show as changed); only
  gitcraft's own entries are replaced. Rules can leak (a script the agent runs reads the file
  anyway), so gitcraft checks the result: a linked worktree whose commits (since its merge-base
  with the main worktree's commit, or the default branch in a bare layout), uncommitted
  changes, untracked files or ignored files touch the fence turns red, as does one that can't
  be checked; its hover lists the paths. A merge, fast-forward or rebase onto any branch is
  first checked in the background (what its commits bring in, and its worktree if it has one)
  and asks for a confirmation when they touched the fence. Cherry-picks and merges on the forge
  aren't checked. A copied `.env` in a worktree counts too: the fence says where agents work.
- Scope drift (a linked worktree's menu → _Scope drift…_): what the worktree changed since it
  left the main branch (committed, uncommitted and new files), set against its task: the text
  written in the window (kept in the worktree's own git folder, never committed) or, without
  one, the issue its branch names (`fix/123-login`, `issue-77`, not dates) or the task does
  (`fixes #12`), read through `gh` / `glab`. Exact
  figures first: _14 files changed, 9 files in 5 folders the task doesn't mention, added 2
  dependencies (Cargo, npm, Python, Go, Ruby, Composer), changed 3 public signatures, 40%
  formatting only_ (a signature is its declaration's first line; the line figures are of tracked
  files). Each item opens the worktree's changes and the item's first file (a change only that
  worktree's working tree has opens the worktree itself). _Ask a
  coding agent_ optionally runs `claude -p` (or `codex exec`) here with the task and the paths,
  no file contents, for a second opinion.
- Submodules (shown when the repository has any): each with its commit and whether it is
  modified, not initialised or out of date. Update one or all (`git submodule update --init
  --recursive`), open one in a new tab (or double-click it), show it in its folder. _Keep
  submodules up to date_ in Settings → Git updates them after every checkout, pull, merge and
  rebase.
- Stashes: stash (with a message, untracked files included or not), see what a stash holds,
  apply, pop, drop. Stash only some files (`git stash push -- <paths>`: Ctrl+click files in the
  changes to pick them, or _Stash this file…_ on a file's menu), only what is staged (`git stash
  push --staged`, git 2.35+), or save a _Snapshot_: a stash of every change that leaves the
  working tree and the staging exactly as they are. All of them are undone with Ctrl+Z.
- Pull / merge requests of GitHub (through `gh`), GitLab (through `glab`), Forgejo and Gitea
  (through their API; Codeberg is Forgejo): list, open in the browser, check out, create from a
  branch. On GitHub, Forgejo and Gitea a click opens one in the big area: description, checks,
  commits, conversation and changed files; comment, review (with comments on lines, pending until
  sent), reply, apply suggested changes, dismiss reviews and merge. Forgejo's and Gitea's API can't
  resolve conversations, so that stays on their web page. _Open in worktree_ (in a request's menu,
  and above the changed files of an open pull request) opens the worktree its branch is checked
  out in, or makes one beside the repository first: on the local branch, on a new one tracking
  the remote branch, or fetched from the forge for a fork that isn't a remote here.
- Start work from an issue: _Start work from an issue…_ (the palette, or the button on the pull
  requests' header) lists the open issues of the repository assigned to you (`gh issue list
  --assignee @me`, `glab issue list --assignee=@me`, Forgejo's and Gitea's API with your login
  for the host; the first 100). Pick one, and gitcraft makes a branch named after it
  (`42-fix-login-crash`) from the remote's default branch (where its `HEAD` points, else main,
  master or develop), fetched first so that it starts where the remote is now, checked out here
  or in a new worktree beside the repository. A branch of that name already there is used as it
  is, and one only the remote has is tracked. The picker says beforehand which of these it does,
  and when changes in the working tree would go along. While that branch is checked out, a card
  above the graph keeps the issue in view: its _Acceptance criteria_ section, else its task
  list, else its description, as of the day it was read (read it again with a click), folded
  away with another. Which issue a branch is for is kept in the repository's git config
  (`branch.<name>.gitcraft-issue`): every worktree of the clone sees it, over WSL and SSH too,
  and git renames or deletes it with the branch; it isn't pushed. _Create pull request…_ (merge
  request, on GitLab) on that branch starts with the issue's title and `Closes #42`, into the
  branch it started from.
- What changed since you last looked: gitcraft remembers which version of each pull request you
  looked at (its page opened for the first time, or its comparison shown), in refs of the repository (`refs/gitcraft/reviewed/…`, which also keep its commits
  here once the forge no longer has them). After the author pushes again, the page says
  _Rewritten since you last looked: 2 commits changed, 1 added, 3 identical_ (or _New commits_
  when they were only added to); click it to compare the two versions, or ✕ to take the new one
  as seen. In the list (every forge, GitLab too) a request that changed says _changed since you
  last looked_, and its menu has _What changed since you last looked_ and _Mark as seen_. A
  request that only moved onto a newer base, every commit the same, says so quietly.
- Pull requests from forks (GitHub, GitLab, Forgejo, Gitea): the list marks those with workflow
  runs awaiting a maintainer's approval (GitHub; _Approve and run_ in the menu, chosen twice, as
  it runs the contributor's code on your CI), those whose author lets maintainers push to their
  branch (_can edit_) and first-time contributors (GitHub, GitLab). _Check out to push to ann_
  (in the menu, and on the pull request's page) adds the fork as a remote named after its owner
  (or uses the remote that already is it, by its URL), reached like `origin` (SSH or HTTPS),
  fetches the branch, checks it out tracking it and sets `branch.<name>.pushRemote`, so that
  Push sends your fixes back to the contributor's branch. When the branch's name is taken here
  it is checked out as `ann/branch`, and Push sends it to theirs by name. A remote added this
  way is marked `remote.<name>.gitcraftFork`. Ctrl+Z goes back, deleting the branch and the
  remote it added (refused once the branch has commits of yours). _Rebase onto main and push to
  ann_ fetches the base and their branch, refuses if they pushed commits you haven't, rebases,
  then force-pushes to their fork with a lease, after showing what it will do; if the rebase
  stops on conflicts, the push is offered once it is continued. Ctrl+Z puts the branch back on
  their fork and here. _Remove unused fork remotes_ (in the palette when there are some) takes
  out the marked remotes no branch uses any more, keeping those whose branches hold commits no
  branch here has (Ctrl+Z adds them back).
- Gerrit: _Push for review_ picks a fetched target branch and optional topic, validates every
  commit's `Change-Id`, and submits to `refs/for/<branch>`. Graph rows link to the change.
  Amending preserves its `Change-Id`; submit again for a new patch set.
- Tags: create (light or annotated, optionally pushed), push, delete locally or on the remote.
- CODEOWNERS: the file at HEAD, found where the forge of `origin` looks for it (GitHub:
  `.github/`, the root, `docs/`; GitLab: the root, `docs/`, `.gitlab/`; Gitea: the root, `docs/`,
  `.gitea/`, and on Forgejo `.forgejo/` after those) and read its way: on GitHub,
  gitignore-like patterns where the last match wins; on GitLab, patterns anchored only by a
  leading `/` and matching files only (a trailing `/` for a folder's contents), in sections
  (`[Docs]`, `^[Optional]`, `[Approvals][2] @owner`, those of one name merged) whose owners add
  up, where an exclusion (`!pattern`) that matches drops the section; on Gitea and Forgejo,
  regular expressions on the whole path (`^pattern$`), where every match adds its owners and
  `!regex` stands for the other files. Each file row (changes, a commit, a comparison, a pull
  request) shows its owners; a line above the files of a commit, comparison or pull request,
  and above the staged files, says who they need ("Needs payments and infra · 2 files without
  an owner"), worked out in the background. _Create pull request_ suggests, ticked, the owners
  of what the branch changes since its base, read from the base's CODEOWNERS as the forge reads
  it (none for a draft). Once it is open they are asked one at a time (GitHub: accounts and
  teams; GitLab: accounts; Forgejo and Gitea: both), so one the forge refuses — the author, an
  account that is gone — is said and doesn't keep the others from being asked. _Check
  CODEOWNERS_ in the palette lists the lines the forge would skip or read differently, the
  patterns that match no file and the files without an owner.
- _Cut release…_ (a commit's menu, or the palette for HEAD) suggests the next version from the
  Conventional Commits since the last version tag (`v1.2.3` or `1.2.3`; other tags are passed
  over): `feat` moves the minor version, `!` or `BREAKING CHANGE` the major one (the minor one
  before 1.0), anything else the patch; a release candidate becomes its release; the tag's `v`
  (or its absence) is kept. It drafts the notes, grouped by type (breaking changes, features, bug
  fixes, performance, documentation, other), with links to the pull requests their summaries
  name (`(#12)`), to edit. It can move the version in `Cargo.toml` (and the package's entry in
  `Cargo.lock`) and `package.json` (and `package-lock.json`), changing nothing else in them, and
  add the notes on top of `CHANGELOG.md`, in one commit, `chore(release): v1.3.0` (from the tip
  of the branch checked out, with those files unchanged); make an annotated or signed tag whose
  message is the notes (headings kept); push that commit to the branch's upstream and the tag,
  both or neither (`--atomic`); and make the release on the forge (`gh release create`,
  `glab release create`, Forgejo's and Gitea's API; pre-releases except on GitLab) with files
  of this computer attached. The dialog says what each step will do before anything runs, then
  how each went. A step that failed (a push refused, the forge unreachable) can be resumed from
  there — a release or file the forge has already is kept, not made twice — or skipped; a push
  that needs signing in asks. Ctrl+Z takes the version commit and the tag back off here (the
  tag put back as it was on Ctrl+Y); what was pushed and published stays.
- Hotfixes, the git-flow way: _Start hotfix from v2.3.1…_ in a version tag's menu offers a branch
  `hotfix/2.3.2` there (git-flow's prefix when it is set up), checked out. _Finish hotfix…_ (the
  branch's menu, or the palette while it is checked out) shows the tag it will make (`v2.3.2`,
  with notes from its commits to edit) and what it will do, then, as one step: merges it into the
  development branch (`develop` or git-flow's, if there is one) and the main one (`main`,
  `master` or git-flow's) with `--no-ff`, tags main's merge (annotated, or signed when ticked;
  git-flow's tag prefix when it is set up), deletes it here, and pushes those branches (to their
  upstreams) and the tag at once (`--atomic`). A merge that would stop on conflicts is said
  beforehand (git 2.38 or later) and refused, and so is a branch checked out in another
  worktree. Ctrl+Z puts the hotfix branch back (checked out, with its upstream), the branches
  where they were and takes the tag off here; what was pushed stays, and Ctrl+Y brings back the
  same merges and tag. Should it stop part-way (a checkout or a merge refused), Ctrl+Z puts back
  what it did.
- Conflicted / unstaged / staged lists with stage, unstage, discard and mark-resolved buttons,
  _Stage all_ / _Unstage all_; right-click a file to open it, open it in your editor, show it in
  its folder, see its history or blame, or ignore it (the file, its extension or its folder go
  into `.gitignore`; a tracked file stops being tracked).
- _Path_ / _Tree_ at the top of the pane lists the files of the changes, a commit, a comparison
  or a pull request flat with their paths, or as a tree of their folders; a folder folds with a
  click and says how many files it holds. The choice is remembered.
- Part of a file: _Stage hunk_, _Discard hunk_ and _Unstage hunk_ on each hunk of its diff, or
  click the changed lines you want (Shift+click for a range) and stage, unstage or discard only
  those. Discarding can be undone with Ctrl+Z.
- Commit box (Ctrl+Enter, from anywhere in the tab: with no summary yet it goes to the summary;
  with nothing staged it stages everything), _Amend last commit_
  (warns when the commit is already pushed), the identity you commit as (and whether it is the
  repository's own, the profile's or the global one), and saved identities or profile authors
  to switch a repository to; _Use the profile's author_ takes the repository's own out again.
- Undo part of a past commit: _Revert changes to this file_ in a commit's file list, _Revert
  file_, _Revert hunk_ and _Revert N lines_ (after clicking lines) in its diff. The piece is
  reverse-applied to the working tree (`git apply -R`), to look over before committing; when it
  applies as it is, Ctrl+Z takes it out again. When the file has changed since, a 3-way merge
  puts it in (staged); overlapping changes are left as conflicts to settle. Binary files and
  submodules are refused.
- _Sign off_ in the commit box adds `Signed-off-by` with your name and email (DCO). It is on
  by itself in a repository with a `DCO` file (`DCO.md`, `DCO.txt`…) or `.github/dco.yml`
  committed, and while a pull request whose DCO check fails is open. Turning it the other way is
  kept for the repository (`gitcraft.signOff` in its git config); turning it back to what the
  repository asks takes that out again. In a repository that asks for sign-offs, a commit
  without its author's says so in the commit pane, and a pull request's page lists the commits
  the DCO check refuses under _Sign-off (DCO)_ (and marks them among its commits); elsewhere
  nothing is flagged. With the pull request's branch checked out at its latest commit, _Sign
  off the branch's commits…_ shows what it will do, then adds your sign-off to every commit of
  the branch (`rebase --signoff`; a branch with merges is refused) and force-pushes it with a
  lease, so that a push made by someone else since isn't overwritten (one already fetched must
  be pulled first). Ctrl+Z puts the commits as they were back, on the remote and here; if the
  push fails, the rewrite stays undoable here.
- _Skip hooks_ in the commit box, or _Push, skipping hooks_ (right-click Push, or the palette),
  runs that one commit or push with `--no-verify`. When a hook makes an operation fail, the
  error names it (`the pre-commit hook stopped it (exit code 1)`), then says what it printed;
  hooks whose exit git ignores (`post-commit`…) are never blamed.
- AI commit messages through a command-line tool you already have: the sparkles button in the
  commit box sends the staged diff to `claude -p`, `codex exec`, `gh copilot` or a command of
  your own (Settings → AI) and fills in the message; a message already typed is only replaced
  if you choose to. _Review this branch_ in a branch's menu does the same with what the branch
  adds to the branch pull requests go to (main, master or develop) and shows the answer in a
  window. When amending, the message is written from the last commit with what is staged. The tool runs on the repository's machine (here,
  in WSL or over SSH) with its own sign-in, the prompt on its input: gitcraft keeps no API key.
  Off until you pick a tool; the tools installed there are found for you. The diff sent is cut
  to a size you set (64 KB at first), each file getting a fair share, lock files and what didn't
  fit named with their line counts; the prompt follows the repository's style (Conventional
  Commits when its history uses them) and can be edited. Cancel stops the tool, and one that
  takes over 5 minutes is stopped.
- The commit box starts from the repository's `commit.template` (`~` and relative paths as git
  reads them; its comment lines are left out, because git keeps a message it is given as it
  is) and then runs its `prepare-commit-msg` hook with the arguments git passes, as git's own
  editor would (stopped after 10 seconds; an error shows under the message and never stops a
  commit; the options can leave the hook out, since git runs it again when committing). An
  unchanged template isn't committed, as git refuses it; amending, or a merge's message, takes
  its place. Above the message: templates (`commit.template`, _Run prepare-commit-msg_, saved
  ones for this repository or all), a type / scope picker (Conventional Commits, Gitmoji or
  none), _Add co-author_ (the repository's authors as `.mailmap` names them, written as a
  `Co-authored-by:` trailer by `git interpret-trailers`) and the options. A template that
  replaces what was typed can be undone. Typing completes the names of changed files and the
  types and scopes used before: while the list shows, Tab takes the suggestion instead of
  moving to the next field.
- Issue references in a commit's message link to their page: `#12` to the forge's issue, and
  patterns of your own (`JIRA-\d+` → `https://jira.example.com/browse/$0`).
- _Fold staged changes into this commit_ (a commit's menu in the graph, or _Fold…_ beside
  _Amend last commit_): the staged changes go into an older commit of the checked-out branch. The dialog
  first shows that commit and the commits after it that get rewritten, and warns when any of
  them is already pushed (a force push will be needed). The changes are committed as
  `fixup! <summary>` (with your hooks and signing) and melded in by a rebase without the
  editor; unstaged and untracked changes stay as they are. _Keep as fixup! commit for later_
  leaves the `fixup!` commit on top instead, for a later `git rebase -i --autosquash`. A
  conflict stops it like any rebase: _Continue rebase_ finishes it, _Abort_ goes back to
  before the fold. Ctrl+Z undoes the whole of it, the changes staged again. It is greyed out
  with why when nothing is staged, HEAD is detached, the commit is a merge or has merges after
  it, or another operation (rebase, merge, cherry-pick…) is in progress.
- Undo / redo (Ctrl+Z / Ctrl+Y) of commits, amends, merges, rebases, folds, pulls, resets,
  cherry-picks, reverts, checkouts, branch create / rename / delete, tags and stashes.
- _What will this do_: before a reset, deleting a branch (here, on the remote or both),
  dropping a commit or discarding a file, the dialog lists the commits leaving the branch (for a
  delete, those the checked-out branch hasn't), each marked _still on x_ or _only here_; the
  files whose uncommitted changes will be lost; that a force push will be needed when the
  commits rewritten are already on the upstream; the git commands it runs, folded; and
  _Ctrl+Z restores this_ when undo covers it. It is read in the background (also for
  repositories in WSL or over SSH); long lists end with _and 340 more_.

**Reflog and lost commits** (above the graph)

- What undo can't reach: what was done in the terminal, in another client or before gitcraft
  was opened. _Reflog_ (the left pane's _Reflog_ section, _Show reflog_ in a branch's menu, or
  the palette) lists where HEAD or a local branch has been, newest first and read page by page:
  how long ago and when, what moved it (commit, amend, checkout, reset, rebase, merge, pull,
  cherry-pick…), from which commit to which, and the commit's summary.
- Select an entry to see its commit in the right pane, and in the graph when it is there.
  _Create branch here_ (or a double-click) gets it back without touching anything else; or
  cherry-pick it, check it out detached, copy its hash. Resetting the checked-out branch to it is
  in its menu, behind the usual confirmation.
- _Find lost commits_ scans for the commits no branch, tag or stash reaches any more, even once
  their reflog has expired, newest first, with the same buttons; a dropped stash comes back with
  _Apply stash_ (`git fsck --unreachable --no-reflogs`, in the background: it reads every
  object, so it can be cancelled). Its tab only says what it does: the scan waits to be asked.
  A commit taken back (a branch made there) drops off the list.
- **Timeline and undo tree** (Ctrl+Shift+Z, or the clock in the toolbar; redo is Ctrl+Y):
  gitcraft's operations, with the branches before and after each and the files it changed,
  kept across restarts (open yesterday's rebase and get the old branch tip back as a branch, or
  put the branch back). Branch moves made elsewhere (the terminal, another client, while
  gitcraft was closed) show up too, named from the reflog. Undo three steps and do something
  new: the undone steps stay as a side path of the tree, and _Go here_ jumps to any entry,
  changes in files kept. The commits it needs stay under `refs/gitcraft/undo/*`, so `git gc`
  doesn't prune them. Entries past the newest 300 are dropped; _Forget…_ drops them sooner.
- **Rewrite receipt**: after a rebase, squash, reorder or amend (in gitcraft or elsewhere), the
  status line and the timeline entry say _final files identical, authors kept, 2 messages
  changed, signatures replaced_. _Follow each commit_ pairs each old commit with its
  replacement, as `git range-diff` does, with whether its files, message, author and signature
  changed. The receipt is kept with the entry.
- **Remove this, keep later work** (in the timeline): pick an older operation that only made
  commits on a branch (yesterday's merge, a commit, cherry-picks) and gitcraft plays the
  commits made on that branch since again without it, in a worktree beside the repository, as
  a new branch; the branch itself doesn't move. If a later commit no longer applies, it stops
  there and names the conflicted files: resolve them in that worktree, or discard the attempt.
  Undo deletes the new branch.
- _What a package will publish_ (from the palette, Ctrl+P) lists the files
  `cargo package --list` or `npm pack --dry-run --ignore-scripts` would ship for each crate of
  the repository's Cargo workspace and its top `package.json`, run where the repository is
  (here, in WSL or over SSH). It flags what looks shipped by accident (`.env`, keys, logs, build
  output, editor folders, a crate's files git doesn't track, big files), what is left out that a package
  usually wants (the README and licence, a crate's `src/` files, the files the manifest names:
  `readme`, `main`, `bin`…), and, asked of the registry once the files are listed, the files new
  or gone since the version on crates.io or npm (not compared for a package published to
  another registry). Refresh reads it again after a change.

**Diffs and conflicts** (centre)

- Unified or side by side (a new file takes the whole width, with no empty old side), syntax
  colours, the changed part of each line marked, whitespace changes optional, images shown
  before / after. Esc closes.
- Pictures compare four ways: _2-up_ side by side, _Swipe_ (drag the line, or the slider),
  _Onion skin_ (the new one over the old, as see-through as the slider says) and _Difference_
  (the pixels that changed, bright over a faint old picture). Zoom (the wheel, at the pointer)
  and pan (drag) stay the same across the modes and both sides of 2-up, the two pictures laid
  out on one canvas so that pixels line up; _Fit_, _100%_ and a double-click reset them. A
  checkerboard shows through transparent parts, and the header says how the dimensions and
  size changed and how many pixels did. Pictures past 4096 pixels are shown scaled (both by the
  same factor); a picture that doesn't decode (an `.ico`) shows before / after as before.
  Below, the picture's history is a strip of thumbnails, newest first; click one to see that
  commit's change.
- Jupyter notebooks (`.ipynb`) diff cell by cell: each changed cell's source line by line, its
  outputs (text, errors, pictures; HTML and SVG as their source, long ones cut) before and
  after, a changed metadata shown line by line, a new execution count only noted, unchanged
  cells folded into a line; cells matched by their ids, or by their sources in order, and moved
  ones marked. Nothing in a notebook is run. _JSON_ switches back to the file's own lines (to
  stage a part of them). _Strip outputs when committing_ sets a clean filter in this clone only
  (`.git/config`, not shared with others), using nbstripout when it is installed, else a small
  Python 3 script: what is staged has no outputs, execution counts or widget state while the
  file on disk keeps them, so a notebook whose outputs alone changed no longer shows as
  modified. A filter set by another tool is shown and left alone.
- A file kept in Git LFS shows as its LFS objects before and after, with their sizes, rather
  than the text of the pointer git stores. With git-lfs installed, the palette (Ctrl+P) of a
  repository that uses LFS has _LFS: Fetch_ and _LFS: Pull_.
- LFS file locks: a padlock on a file's row shows who holds its lock (hover it for since when);
  _Lock_ and _Unlock_ are in the menu of a file of the working tree or the checked-out commit.
  _LFS: Locks…_ in the palette lists every lock, filters them by path or owner, unlocks several
  at once, force-unlocks someone else's (admins only) and makes a pattern lockable
  (`git lfs track --lockable`; commit `.gitattributes` to share it). Above the changes, a
  changed file someone else holds is flagged (the push would be refused), as are changed
  lockable files nobody holds, with _Lock all_. After a push of the checked-out branch,
  gitcraft offers to give back your locks on the files it carried that have no changes left.
  Locks are read when the tab opens and every few minutes while background fetching is on
  (Settings), and whenever the panel opens.
- LFS storage: _LFS: Storage…_ in the palette of a repository that uses LFS says how much disk
  the local LFS objects take, lists the files of the checkout that are only their pointer on
  disk (never downloaded, or fetched but not checked out) with _Download_ for each or all, sets
  which folders git-lfs downloads on fetch, pull and checkout (`lfs.fetchinclude` /
  `lfs.fetchexclude`, for this repository only; what a committed `.lfsconfig` sets is shown),
  and prunes old objects after showing what that frees, asking twice. In _View all files_ of
  the checked-out commit, a cloud marks the files that are only pointers (not on files that
  commit changed, whose stats take that place). The menu of a file of the checkout has _LFS:
  Download_ for such a file, _LFS: Fetch only <folder>_ for a file in LFS, and _Track \*.ext in
  LFS_ for one whose type isn't (which writes `.gitattributes`).
- Unity projects (at the root or one folder down) are recognised, before the first commit too.
  _Unity: Set-up…_ in the palette checks that assets are saved as text (Force Text) and `.meta`
  files are visible, that `.gitattributes` merges Unity's YAML with Smart Merge (and, in a
  repository that uses LFS, keeps its binaries there), that `Library`, `Temp`, `Logs` and the
  other generated folders are ignored and not committed, and adds the missing lines or stops
  tracking those folders on request. A hint above the changes names the first thing worth
  fixing (_Don't show again_ turns it off for the repository). It finds UnityYAMLMerge in Unity
  Hub's editors (or one you choose) and registers it as the repository's merge driver; when it
  is found, a conflicted scene or prefab has _Solve with UnityYAMLMerge_ in the conflict editor,
  which shows the result before saving. Above the changes, an asset or folder staged without
  its `.meta` (or a `.meta` without its asset, or one deleted without the other) is flagged,
  with a button to stage the missing half, delete the leftover `.meta` too, or keep the `.meta`.
- Other binary files show what they are instead of "Binary files differ": their kind (and a
  change of kind), what changed in size, and a before / after table of what can be told about
  them (a PDF's version, pages, title and author; an archive's members and unpacked size;
  the dimensions of a picture the image view doesn't take). Then _Text_, _Members_ or _Bytes_:
  the text of Word and OpenDocument files, the slides of PowerPoint files in the order they
  are shown, the cells of Excel sheets (a formula next to its value), and the text of PDFs
  where it is plainly in them, diffed line by line; the members of a ZIP-based file added,
  removed and changed; and the first 64 KB of both sides in hex, bytes that differ at the same
  offset marked. Files over 64 MB aren't read. A note says when a side's text is incomplete (a
  slide or sheet too big to read), when the texts are too unlike to match line by line, and
  when a side couldn't be read or its member list is damaged (its members then aren't
  compared). A file with a `textconv` driver shows git's text of it, said so above the diff,
  and is staged or unstaged as a whole.
- The arrows in a diff's header (F7 / Shift+F7, or Alt+↓ / Alt+↑) jump to the next and previous
  change. The strip beside the scrollbar marks where the changes are, removals red and additions
  green; click a mark to jump to its change, or anywhere else on the strip to scroll there.
- The panel button in a diff's header (or Settings → Graph → Diffs) puts the diff below the graph
  instead of in its place (remembered): the graph stays usable, selecting another commit keeps
  the diff, which follows the next file opened, and the border between them drags to resize.
  Diffs of a file's history, of a pull request, over the builds, and the conflict editor keep
  their own place.
- Context that opens where you need it: each hunk's header says how many unchanged lines are
  folded above it (_42 unchanged lines_), and a row after the last hunk those below it. _↑ 20_
  shows the lines right above the hunk, _↓ 20_ those right below the hunk before, _Show all_
  every one, and _Function_ the lines of the function git names for the hunk (when it encloses
  the hunk), up to its first line and down to its end. In a narrow pane the least needed
  buttons give way first. The lines opened join their hunk as context, so staging, unstaging
  and discarding hunks or picked lines work as before; those next to a hunk stay open when the
  diff is read again (after staging, or a change on disk), and a line a pull request's review
  commented on opens by itself. Folds are only offered when the file read matches what git
  diffed: not after a clean filter (`core.autocrlf`, Git LFS) or textconv, nor for files that
  aren't UTF-8. Close and reopen the diff to fold everything again.
- Moved code: a block removed in one place of the file and added in another (the whitespace
  around its lines aside, so re-indenting doesn't hide it) shows in a muted colour of its own
  instead of red and green, with a bar beside both places. _Moved to line…_ / _Moved from
  line…_ on its first line opens a panel with the two places together: the lines kept, those
  edited on the way (the changed part of each marked), and buttons to go to either place; the
  button reads _…, edited_ when lines changed on the way. Esc or its × closes the panel.
  Staging hunks and lines works as before (a click on the button opens the panel rather than
  picking that line).
- The _Diff_ / _File_ switch shows the whole file as it is after the change instead (before it,
  for a deleted file), coloured and with line numbers.
- Ctrl+F in a diff (unified or side by side), a file or a blame finds in it rather than in the
  commits: every match is marked and _Enter_ / _Shift+Enter_ (or F3 / Shift+F3) step through
  them in the order of the rows; the bar shows which match of how many (e.g. 3 of 12). Match
  case, whole words and regular expressions are toggles; Esc closes the bar. A very long file
  is searched a few lines per frame, so the window stays responsive.
- The algorithm in a diff's header (_Default_, _Myers_, _Minimal_, _Patience_, _Histogram_)
  pairs the lines another way when they line up badly: the diff is read again with
  `--diff-algorithm`, the line at the top of the view stays at the top, and the choice is kept
  for that file type (`*.rs`, `makefile`…; Settings → Git → Diffs lists and forgets them). The
  repository's config is never written; _Default_ is what `diff.algorithm` says.
- Generated files and lockfiles start folded: files marked `linguist-generated` or `-diff` in
  `.gitattributes` (read with `git check-attr`, so every attributes file counts, and
  `linguist-generated=false` keeps a file open), lockfiles (`Cargo.lock`, `package-lock.json`,
  `yarn.lock`, `pnpm-lock.yaml`, `poetry.lock`, `go.sum`, `Gemfile.lock`, `composer.lock`…) and
  minified files (`*.min.js`, `*.min.css`). File lists gather them at their end under one
  closed row (_2 lockfiles +1,204 −880_; the working tree's lists have no counts), and a file
  clicked there opens. Reached another way (its history, a pull request's timeline), its diff
  shows only a line such as _+1,204 −880, lockfile_ until _Show the changes_. Settings → Git → Diffs adds patterns of your
  own (`*.snap`, `dist/**`, `vendor/`) or turns folding off.
- Readings that understand the file: some files get a button in the diff's header that shows
  their change in a way of their own instead of as lines (click it again for the lines, where
  staging stays). They read both sides of the file in the background (on the repository's
  machine, for WSL and SSH, under its old name before a rename); a click on a part goes back
  to the lines, at its line or the nearest one the diff shows, and a file they can't read (not UTF-8, over 8 MB, malformed) shows the plain diff with a note
  saying why.
  - _Prose_ (the book, for `.md`, `.adoc`, `.rst` and `.txt`): paragraphs wrapped to the
    pane. In an edited paragraph the removed words are struck through on red and the added
    ones on green, compared word by word (a punctuation mark counts as a word of its own;
    the text keeps its own spacing), so a paragraph only re-wrapped says _re-wrapped_ and
    marks nothing. Removed and added paragraphs show whole; long runs of unchanged ones fold
    (click to open).
  - _By key_ (the list, for JSON, YAML and TOML): one row per key path that changed, such as
    `server.timeout: 30 → 60`, keys added and removed, a value that became another kind
    (`boolean → string`), and keys that only moved within their map listed apart from
    changed values (_moved, same value_). List items are matched by content, so an item
    inserted at the top doesn't make every one after it changed (in lists of a few thousand
    items that differ; longer ones are compared item by item in place). Comments and
    formatting don't count (quoting does in YAML, where `'5'` is a string and `5` a number);
    a YAML alias is compared as what its anchor holds; JSON may have the comments and
    trailing commas of JSON-with-comments files. A file that doesn't parse says on which side
    and line.
  - _As a table_ (for `.csv` and `.tsv`): a grid of the rows added, removed and changed, the
    changed cells as `old → new`, the unchanged rows between them folded (click to open) and
    the header kept on top. Columns are matched by name; one renamed (most of its values the
    same) shows `old → new name`, and columns added or removed are marked. Rows are matched
    by their first column when its values are all there and all different (a key), else by
    content. Quoted cells may hold commas, quotes and line breaks; a quote that never ends
    falls back to the plain diff with the line.
  - _Data tracked by DVC_ (for `.dvc` files): each path it tracks before and after, with its
    size, hash and, for a folder, how many files it holds, as files in Git LFS show.
  - _Dependency changes_ (the package, for `Cargo.lock`, `package-lock.json`,
    `npm-shrinkwrap.json`, `pnpm-lock.yaml`, `poetry.lock`, `uv.lock`, `composer.lock` and
    `go.sum`): the packages upgraded, downgraded, added and removed, each with its versions,
    a link to its registry page (crates.io, npm, PyPI, Packagist, pkg.go.dev) and one to what
    changed between the two versions (diff.rs for crates, npmdiff.dev for npm packages, the
    release history elsewhere); changes of licence are shown where the lockfile records them
    (npm, Composer). A version installed in several places counts once; a package kept at
    several versions is listed per version added or removed; a git dependency or a Composer
    branch shows its commit, so moving it is seen; `go.sum` is read as the newest version of
    each module (it keeps lines of older ones). The project's own packages (workspace members,
    path dependencies) get no registry links. A click on a package's name goes to its line. The links open in the browser only
    when clicked: reading the lockfile asks nothing of any server.
  - _API contract changes_ (the server, for an OpenAPI or Swagger file: offered on files named
    like one, and from the view by key of any JSON or YAML file that is one): what breaks the
    API's clients, each with its endpoint — endpoints removed, parameters that became required
    or are new and required, a request body or request field now required (read-only fields
    aside), a request media type dropped, request values narrowed (an enum value taken away,
    no more other fields), types that no longer take what they took (`integer` → `number` is
    wider, not breaking), success responses removed, response fields that were always there
    removed or no longer always returned, and responses that can now return an enum value
    clients don't know — then the endpoints added. A path parameter renamed is the same
    endpoint; header names are compared in any case. Local `$ref`s are followed, each pair
    once per endpoint (schemas that refer to each other end). A click opens the change's
    place in the file.
- A Markdown file's diff has an eye button that renders it: the changes block by block (removed
  ones red, added ones green, the rest as they are) or, with _File_, the whole file. Images of
  the repository show as they are on each side of the change; any other image is a link, so a
  file can't make gitcraft contact a server.
- _weakens checks_: a file of a commit, a comparison or the changes (staged, unstaged and untracked
  apart), and each hunk of its diff that does it, gets this badge when it removes assertions or
  tests (not ones moved to another file), deletes a test file, skips, disables or focuses tests
  (`#[ignore]`, `@pytest.mark.skip`, `it.skip`, `xit`, `.only`, `@Disabled`, `[Ignore]`,
  `t.Skip()`…), adds an assertion that can't fail (`assert True`), loosens a tolerance in a test
  (`rtol=1e-6` → `1e-2`), rewrites a snapshot wholesale, lets CI fail (`continue-on-error`,
  `allow_failure`, `|| true`), removes a GitHub Actions / Forgejo / Gitea or GitLab job or
  trigger, or adds `--no-verify`. Fixed rules on the diff's lines and the files' names, the same
  way every time (prose files and text in strings left out); the badge's hover lists the lines
  that made it, `old line` for removed ones.
- Code takes the colours of the theme VS Code (or Insiders, VSCodium) is using, read from its
  settings and extensions, when it is dark or light like gitcraft's; a setting turns it off.
- Conflicts open in a three-pane editor: each conflict's two sides with _Ours_, _Theirs_ and
  _Both_ buttons, and the result below (editable). _Use mine_ / _Use theirs_ for a whole file.
- Each conflict's _Original_ section shows the text both sides started from and what each
  inserted and deleted relative to it ("one side deleted the validation, the other changed
  it"): from diff3 markers in the file, or rebuilt from the index. Without one (both sides
  added the file) the card says so.
- _Keep both…_ on a file that can't be merged (an image, an archive) previews the two files it
  would write, named after each side's branch (`logo.main.png`, `logo.feature-x.png`; _ours_ /
  _theirs_ without one, `-2` when a name is taken), with each version's origin and size; it
  never writes over an existing file, warns that references to the old name may need updating,
  and stages nothing until you have checked them.
- With [Mergiraf](https://mergiraf.org) installed, _Solve with Mergiraf_ lets it solve the
  conflicts it can from the syntax of the code; the rest stay to choose, and nothing is saved
  before you check the result. Settings → Git installs it in one click and can have git merge
  with it everywhere ([docs/mergiraf.md](docs/mergiraf.md)). gitcraft only runs the `mergiraf`
  program: it neither bundles nor links Mergiraf (GPL-3.0), so gitcraft's licence is unchanged.
- Conflict warnings before you act: in the menus of branches and pills, _Merge_ and _Rebase_,
  and in a commit's menu _Cherry-pick_ (not for merge commits), say _conflicts in 3 files_,
  _applies cleanly_ or _may conflict_ before they are clicked. Hover one to see which files would conflict (for a rebase, at which
  of its commits) and whether Mergiraf could likely solve each one. A badge beside the branch in
  the toolbar says when merging its upstream (or the default branch, without one) would
  conflict, or rebasing onto its upstream when Pull is set to rebase. It is worked out with `git merge-tree --write-tree` in the background, a rebase or
  cherry-pick commit by commit, and never touches the working tree, the index or a ref, so it
  works in WSL and over SSH too; results are remembered until the commits change. Merges need git
  2.38, rebases and cherry-picks 2.40; with an older git the entries say so on hover instead.
  A partial clone isn't checked (git would download the files it lacks); F5 checks again.
  Uncommitted changes aren't part of the check; git may refuse to start if they touch the same
  files.
- External diff and merge tools: _Open in diff tool_ in a file's right-click menu (working tree,
  staged, a commit's file, a comparison) shows its diff in one; _Resolve in merge tool_ on a
  conflicted file (or the same button in the conflict editor) resolves it in one. Settings → Git →
  External tools picks each: the tool git's `diff.tool` / `merge.tool` names
  (`git difftool --tool=… -y`, `git mergetool --tool=… -y`), or Beyond Compare, Araxis, VS
  Code, Meld, KDiff3, P4Merge or WinMerge, with their program when it isn't found by itself
  (a program of this machine; over SSH git there finds its own).
  The view refreshes when the tool closes. The file counts as resolved only when the tool
  reports success, as git decides it: from its exit code with `mergetool.<tool>.trustExitCode`,
  otherwise from whether the file was saved; `mergetool.keepBackup` says whether the `.orig`
  copy is kept. In WSL a preset is the Windows program, given the files through their
  `\\wsl.localhost` paths; over SSH the tool runs on that machine and has to be installed
  there. Tools that need a terminal (vimdiff) are left to the terminal panel.
- Above the conflicts of a merge, rebase, cherry-pick or revert, _How this conflict came
  about_ lists each side's commits that touched the file (a rebase's, cherry-pick's or
  revert's other side is the commit being applied). Pick one on each side to see what it
  changed or test-merge the two. _Find where it started_ finds the first pair whose merge
  already conflicts in the file: a few test merges with `git merge-tree` (git 2.38, or 2.40
  for a rebase, cherry-pick or revert), using the base git uses, with your files and the index
  left as they are, in the background and cancelable.
- _Remember resolutions_ (above the conflict editor's result, or _Remember conflict resolutions_
  in the palette) turns on `git rerere` for the repository, with `rerere.autoUpdate` off there:
  when a conflict you resolved before comes back, git reuses your resolution and the editor
  says so, showing each conflict with the text reused for it. It stays unstaged until you save;
  _Forget this resolution_ runs `git rerere forget` and brings the conflict markers back.
- _Note why…_ under a conflict keeps a reason ("keep the new validation, keep the old wire
  format") and a test command with how it is resolved, in git
  (`refs/gitcraft/conflict-decisions/*`). When the same conflict comes back, it is resolved the
  same way with the reason shown, and how its test last went; if the code around it changed
  since, it is only offered, marked stale. _Test after saving_ runs the tests you wrote in the
  repository's folder once the file is saved; a decision's own test runs only once you take it
  as yours (_Keep its note and test_). The palette's _Get conflict decisions_ and _Send
  conflict decisions_ share them through a remote; for the same conflict, the newer one wins.
- A merge, rebase, cherry-pick, revert or patch stopped on conflicts shows a banner with
  _Abort_; the commit box finishes it (_Continue rebase_ during a rebase, _Continue patch_ while
  `git am` applies one).
- File history (every commit that changed the file, following renames; click one for its diff
  or the file as it was, beside the list), the history of a few lines (select them in a diff or
  a file, right-click → _History of these lines_: every commit up to that version that touched
  them, `git log -L`, followed back through earlier edits and renames, each showing only what it
  did to them), blame (who last changed each line, newer changes
  marked brighter; click to go to the commit) and the whole file at a commit (_View file_ in a
  file's menu).
- _View all files_ under a commit's files lists every file of the commit, not only those it
  changed; click one to read it as it was in that commit.
- Links to the forge: _Copy link to …_ and _Open … on GitHub_ (GitLab, Forgejo, Gitea;
  self-hosted ones too, under a sub-path, or on another port for an HTTPS remote) in the menu of
  a commit, a branch (its pill or its row in the sidebar), a file, and the lines of a commit's
  diff (right-click a line, or the text selected for a range) or of a file at a commit (click a
  line, Shift+click to make a range, then right-click; also with _File_ in a commit's diff). A
  link to a commit, a file or lines names the commit by its hash, so it keeps showing the same
  thing when the branch moves on; a branch's link follows the branch. Links go to the remote
  the checked-out branch tracks (from a branch's own menu or pill, that branch's), else
  `origin`; with neither, the menu lists the remotes to choose from. The link is made in the
  background and copied (or opened); when the commit or branch isn't on that remote yet (as of
  the last fetch), the status line says so: the page won't be found until it's pushed. Stashes
  and uncommitted changes have no links.
- A commit's details fold out four more sections, read in the background once the selection
  rests on a commit: its **Signature** (whether it is valid, and if not why: untrusted key,
  expired, revoked, invalid, unknown key, unsigned or not checked, for GPG, SSH and X.509, with
  the signer, the key and git's own output; history is read with signature checks off, so only
  the commit selected is checked, and _Check again_ re-checks it after you import a key), where
  it is **Contained in** (the branches and tags that have it and the first tag after it,
  `describe --contains`), its **Notes** (add, edit and delete git notes in any namespace; fetch
  a namespace from a remote or push it to one. Neither overwrites: when both sides have notes
  the other lacks, it stops, and `git notes merge` on the command line combines them) and its
  **Backlinks** (later commits that mention its hash: reverts, cherry-picks and trailers such
  as `Fixes:` apart from plain mentions; click one to open it).

**Coding agents (MCP)**

- `gitcraft mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server over
  stdio, for Claude Code, Codex and other agents. Add it once:

  ```sh
  claude mcp add --scope user gitcraft -- gitcraft mcp
  codex mcp add gitcraft -- gitcraft mcp
  ```

  (or, in Codex's `~/.codex/config.toml`, `[mcp_servers.gitcraft]` with `command = "gitcraft"`
  and `args = ["mcp"]`). Settings → Agents shows the same commands with this copy's own path when
  `gitcraft` isn't on the PATH. The agent starts it in the repository it works in; `--repo
  <path>` names another one (also one in WSL or over SSH, as gitcraft names it:
  `ssh://host/path`).
- Reading tools work on their own, with no window open: `log` (the graph: commits with their
  branches and tags, paged, or one file's history), `show_commit`, `status`, `diff` (working
  tree, index or a commit, paged by bytes), `blame` (a range of lines), `branches` (with
  remotes, tags, stashes and worktrees), `conflicts`, `ci_status` (runs, jobs and the failed log
  through `gh` / `glab`) and `pull_requests`. Paths must stay inside the repository, revisions
  can't be taken for options, and a remote's URL never carries a token out.
- Changing tools run in the gitcraft window that has the repository open: `stage`, `unstage`,
  `commit`, `create_branch`, `rename_branch`, `checkout`, `stash`, `cherry_pick`, `revert`,
  `reset` (soft, mixed) and `move_commit`. Each is a step of that tab's undo list, marked with a
  robot on the undo button and named in its tooltip, so Ctrl+Z also takes back what the agent
  did. With no window for the repository, they fail and say to open it.
- What can't be undone, or is easy to regret, waits on an _Approve / Deny_ card in the window
  that names the agent, says what will happen and shows the git commands: `push` (forced or
  not), `delete_branch`, a `hard` reset, `drop_commit`, and staging a conflicted file (which ends
  the conflict). The call waits for the answer; after two minutes it is denied automatically.
- An approval can take up to two minutes, longer than some agents wait for a tool by default:
  give the server more time there (Codex: `tool_timeout_sec = 180` under
  `[mcp_servers.gitcraft]`). A call the agent cancels, or an agent that quits, takes its card
  away, and what hadn't started doesn't run.
- `gitcraft mcp` has no window of its own to ask ssh's questions in: for a repository over SSH
  it signs in with keys (an agent or a key without a passphrase) only. An agent running inside
  WSL starts the Linux gitcraft, which can't reach a window on Windows: start it from Windows
  with `--repo \\wsl.localhost\<distro>\path` instead.
- Settings → Agents turns agents' changes off; the reading tools still work. `gitcraft mcp`
  reaches the window the way a second start of gitcraft does (see one window, above): over
  127.0.0.1, with the running window's random token from a file only you can read, believed only
  while that window holds its lock. An agent's change is a request of its own kind there, and
  its connection stays open until the window answers.

**Repository doctor** (the wrench in the toolbar, or _Doctor: …_ in the palette)

- A page in the big area, its sections listed on the left as in Settings; Esc closes it.
  Everything is read in the background, and changes run as the tab's operations.
- _Ignore rules and attributes_ (also in the menu of a working-tree file): for a path, typed
  from the repository's top or absolute inside it, the rule responsible (`git check-ignore -v`),
  shown as ignoring it, the last `!` rule that matches it, or not applying because the file is
  tracked (with what `git rm --cached` would change); a folder, and a path with no file, are
  said to be so. Its line endings in the repository and the working tree (`git ls-files
  --eol`), with a warning when CRLF is stored in the repository and a note when no attribute
  pins them (where line-ending churn comes from); its attributes (`git check-attr -a`) in words
  (text or binary, LFS, merge and diff drivers, git's own diff patterns), and a warning for a
  merge, diff or filter driver an attribute names that isn't set up here (git falls back to its
  own, or refuses the file for a required filter; LFS not installed is named as such). The
  ignored files are listed (a folder ignored whole once). Each `.gitignore`, `.git/info/exclude`
  and your own excludes file (`core.excludesFile`, or git's default `~/.config/git/ignore`) can
  be edited, with a preview of the files the change would start or stop ignoring (by that
  file's rules alone, and the tracked files it matches, which stay tracked). Saving keeps the
  file's line endings, refuses a file changed meanwhile (_Read it again_ keeps your text) or a
  symbolic link, and asks once more for a file outside the repository; closing the doctor
  with unsaved edits asks first.
- _Hooks_: the folder git runs hooks from (`git rev-parse --git-path hooks`, so
  `core.hooksPath` too; set to nothing, hooks are off and said so), every hook in it with the
  operations that run it, and what keeps one from running: not executable (git skips it with a
  hint), CRLF line endings (a `#!/bin/sh\r` line makes git fail with "cannot exec … No such
  file or directory"), an interpreter its `#!` line names that isn't installed where git runs
  (looked up on the PATH git gets), a folder with a hook's name (git fails on it), no `#!` line
  (Git for Windows only). Files git never runs (`pre-commit.old`) are listed apart. _Make
  executable_ and _Convert to LF_ fix those in one click (never through a symbolic link).
  husky, lefthook and pre-commit are recognised, with where to change their hooks; with husky
  the scripts in `.husky/` its stubs run are checked too. What git and its hooks printed during
  the last dozen operations gitcraft ran in the repository (commit, merge, push, checkout,
  rebase…, also when they succeeded) is kept in memory, colours taken out and passwords in URLs
  hidden. On WSL and SSH the folder is read in a few round trips there.
- _Health and maintenance_: commits (`git rev-list --count --all`), packed and loose objects and
  their size (`git count-objects -v`, with leftover garbage pointed out), how long `git status`
  takes and how long the tab's last reloads took; then what would make git faster here, each
  with the commands it runs and _Turn on_: the commit-graph (`git commit-graph write
  --reachable --changed-paths`, and `fetch.writeCommitGraph` to keep it up to date), scheduled
  maintenance (`git maintenance start`, asked first: it registers the repository in your own
  global config, never a profile's, and schedules git with the system), git's file system
  monitor (`core.fsmonitor`; Windows and macOS only, said so elsewhere), the untracked cache,
  and packing loose objects (`git gc`) when there are many. It also says what the graph's
  100 000-commit limit means for this repository.
- _Which configuration wins_: the settings that explain why git behaves differently here (pull
  strategy, push, signing, line endings, file handling, performance…), each with the value in
  effect and its scope and file (`git config --list --show-origin --show-scope --includes`).
  Opened, every value git read, the one in effect on top and those it overrides below.
  _Remove_ takes every value of the setting out of that file (asked first) so the others apply
  again; _Set it for_ writes a value for this repository, this worktree (when
  `extensions.worktreeConfig` is on), your global git config (usually `~/.gitconfig`) or
  git's system file (those two asked first); _Compare with the other worktrees_ shows what it
  is in each. A value from a gitcraft profile's file is marked as such and changed in
  Settings → Profile. Only an allowlist of settings is read: nothing credential-related is
  ever shown.
- _Hidden changes_: tracked files marked `assume-unchanged` or `skip-worktree`
  (`git ls-files -v`), whose changes never show under Changes, what each flag does (and that
  git supports neither as a way to ignore changes to tracked files), and clearing them (all,
  or those ticked; asked first). Files sparse-checkout leaves out carry the same mark but are
  listed apart, with how sparse-checkout is set up, and are never cleared; with pattern-mode
  sparse-checkout every skip-worktree file counts as its own.

**Coding agents**

- _Start agent task…_ (the + of AGENTS on the left, or the palette) makes a branch and a
  worktree beside the repository, opens it and starts a terminal there running the agent you
  pick: Claude Code, Codex, Gemini CLI, Cursor Agent, Aider or a command of your own, the task
  given as its first message when the agent takes one. If the agent isn't installed there, the
  terminal says so. AGENTS lists the tasks with their uncommitted changes; click one to
  open its worktree and review what the agent did with the normal diff.
- _Worktree setup…_ (palette): a recipe per repository that runs in every worktree gitcraft
  makes (the Add worktree dialog, a pull request's _Open in worktree_, agent tasks), in a
  terminal with its output: it copies or links ignored files from the main worktree
  (`.env.local`, `node_modules`), sets variables per worktree
  (`PORT=$((3000 + GITCRAFT_WORKTREE_INDEX))`, a `CARGO_TARGET_DIR` of its own; the main
  worktree is number 0, the others keep the number they were given) and runs a setup command
  (`npm ci`). A step that fails says so; the setup command and the agent are then not run. It runs with `sh` on the repository's machine (Git's own `sh` on Windows),
  WSL and SSH included; the script it makes can be read in the editor, and tried on a worktree
  before saving. Kept in `.git/gitcraft/`, never committed.
- Agent turns as review steps: _Agent turns: save each turn in this worktree_ (palette), or
  _Save each turn to review on its own_ when starting a task, installs Claude Code's
  `UserPromptSubmit` / `Stop` hooks (`.claude/settings.local.json`) and Codex's `notify`
  (`.codex/config.toml`, read for trusted projects), both kept out of git through
  `info/exclude`. After each turn they run `gitcraft checkpoint`, which saves the worktree as
  it is (uncommitted and untracked files, its own index untouched) under
  `refs/gitcraft/agent/<session>/<n>`, a turn that changed nothing skipped. _Uncommitted
  changes_ then shows AGENT TURNS, each named by its prompt: click one to see what that turn
  changed; its menu compares it with the files as they are now. The latest session shows,
  earlier ones fold away (named by their first prompt); a session's menu deletes its turns.
  Hooks take effect the next time the agent starts in the worktree. On a WSL or SSH machine, where gitcraft isn't,
  the hooks run a `sh` script gitcraft writes into the repository's git folder. The turns are
  not history: the graph leaves them out.
- Hand it to the agent: click a line's number in a diff (Alt+click in a diff of uncommitted
  changes, where a click picks the line to stage it) to write a note for the agent on it. Notes
  are kept per worktree until they are sent, never published, and collect in NOTES FOR THE
  AGENT under the big area; _Send to agent_ types them into the worktree's agent terminal (a
  terminal named after the agent, or the one picked beside the button) as one message, pasted
  for you to send with Enter, and clears them. The same _Send to agent_ is on a conflict (each
  conflict's three sides), on a failing CI run's log (its end) and on a pull request's thread
  (with its file and line). Nothing is typed into a program that hasn't asked for pastes.
- _Bake-off: compare attempts…_ (palette, or _Compare them_ under AGENTS): pick 2–4 worktrees or
  branches that tried the same task (the agent tasks come picked when there are 2–4 of them).
  A grid compares them with the commit they all start from (their merge base; one that started
  from a newer commit is named):
  the files each touched, its size, the dependencies it adds (Cargo, npm, Go, Python, Ruby,
  Composer manifests) and whether it builds (a command you type, kept for the repository, run
  in a terminal in each worktree). A worktree's uncommitted work is compared too, read through
  an index of gitcraft's own. Click a file to see it side by side across the attempts that
  changed it; take it whole from one (click again to take it back), or tick hunks of several
  (a binary file only goes whole), and _Assemble_ makes a new branch from the base with what
  you picked, without checking anything out; the commit message lists where each file came
  from, and _Show in graph_ goes to it. Hunks of two attempts that change the same lines are
  flagged, and Assemble refuses them.

**Toolbar, terminal, app**

- Undo, redo, the timeline, Fetch, Pull, Push (with incoming / outgoing counts) and Sync, new branch, stash, terminal,
  search, open the folder (Explorer / Finder, your editor), refresh (F5).
- Back and forward (the arrows at the start of the toolbar, Alt+← / Alt+→, or the mouse's back
  and forward buttons) go through the tab's recent views: the graph with its selected commit,
  diffs, a file's history, blame or contents, pull requests and builds. Clicking through commits
  in the graph is one step; following a link (a parent, a branch, a tag) is a step of its own.
  If its commit or change has gone since, the status bar says so.
- Pull's arrow (or a right-click on it) sets what it does: fast-forward, merging when the branch
  has diverged; fast-forward only; or rebase. Also in Settings, and the menu has _Fetch all_.
- _Sync_ pulls (the way Pull is set to), then pushes; if the pull fails (conflicts, or it can't
  fast-forward), nothing is pushed.
- A refused push gets a banner instead of "failed to push some refs": _origin/dev has 3 commits
  you don't have_, saying whether the branch was rewritten here or both moved on, with _Pull,
  then push_ and _Force push (with lease)…_. The force push names the commits it removes from the
  remote, and runs `git push --force-with-lease=<ref>:<commit checked>` (and
  `--force-if-includes` where git has it), so a remote that moved since pushes nothing. A hook's
  or a protected branch's refusal shows git's message in full, with no force push offered. Read
  from `git push --porcelain`, not from git's sentences.
- Fetch, pull, push and clone show how far git is ("Receiving objects: 45%", from git's own
  `--progress`) beside the spinner and as a thin bar under the toolbar, with _Cancel_: it
  stops git and everything git started (`git-remote-https`, `ssh`; on another machine, the git
  command there is stopped the same way as one that hangs), runs none of the later steps, and
  leaves undo as it was. A cancelled clone removes what it had cloned; a lock file a stopped
  git couldn't remove (`index.lock`…) is named in the toolbar's message, and a pull stopped
  while merging or rebasing leaves that in progress, with _Abort_. Fetch, Pull and Push stay
  clickable while something runs and start once it is done (up to three wait, in order; the
  toolbar says which, and Cancel or a failure drops them, saying so). Anything else asked for
  meanwhile is not started and says why, and the toolbar's other buttons say what they wait
  for.
- Activity (the list button in the toolbar, or _Activity_ in the palette): every operation of
  the tab, newest first, with how it ended and how long it took; unfold one for each git
  command it ran (as you would type it; tokens in URLs, `Authorization` headers and a password
  typed into the tab are hidden), its exit code, its time and
  all it printed, selectable, with a button that copies the lot. The operation running shows
  its commands so far. A message in the toolbar (an error, cut to its first line there) opens
  the operation's entry when clicked. The last 100 operations are kept, each command's output
  up to 64 KiB (its start and its end); a background fetch only when it fails, and the same
  failure again and again as one entry, counted.
- The taskbar button shows how far a clone, fetch, pull or push is (the least advanced, when
  several run). When one fails while the window is in the background it turns red on Windows
  (on Linux the launcher asks for attention, as each dock shows it), until the window is looked
  at or another operation starts. On Windows that is the taskbar's own progress; on Linux the
  launcher's (the `LauncherEntry` signal that Ubuntu's dock, Dash to Dock, KDE's task manager
  and others show).
- The taskbar button's menu (Windows: its jump list; Linux: the launcher's actions) lists the
  recent repositories, then _Clone…_ and _Open…_ (`gitcraft --clone`, `gitcraft --open`). On
  Linux they go into the AppImage's menu entry, or, for the .deb and .rpm, into a copy of the
  package's entry in `~/.local/share/applications` (one you wrote yourself is left alone); the
  package's own entry has _Clone…_ and _Open…_. The OS calls this takes on Windows are in a small
  crate of their own, `crates/gitcraft-os`, so that gitcraft itself keeps forbidding unsafe code.
- Background fetch every few minutes (Settings; 0 turns it off).
- Offline mode (Settings → Git, or _Go offline_ in the palette): nothing goes on the network by
  itself (the background fetch, avatars, update checks and downloads, the polling of builds and
  pull requests), and in a repository's tab Fetch, Pull, Push and the other git commands that
  reach a remote (`ls-remote`, `remote update`, LFS transfers…) say why they don't run instead of
  waiting for a network that isn't there. Commands that only change the config (`remote add`,
  `set-url`…) still run. Cloning, the forge's pages (pull requests you open, reviews) and
  _Check for updates_ are not refused: they are asked for by name. A repository in WSL or over
  SSH is still looked at every few seconds, as it needs to be to work. The toolbar says
  _Offline · last fetched 3 h ago_ (from `FETCH_HEAD`); a click on it goes online. Without the
  switch, gitcraft asks the system every minute whether there is a connection and whether it is
  metered (Windows: a metered or roaming network, or one at its data limit, through
  `NetworkInformation`; Linux: NetworkManager), and the background work waits for its first
  answer (a few seconds at most): without a connection, or on a metered one (unless Settings →
  Git says otherwise), only that background work pauses, and the toolbar says so; what you ask
  for still runs, and a repository over SSH still fetches in the background (on that machine's
  network). Windows' "local access only" and "limited" don't count as no connection, as a
  proxy or VPN can make it say so. Once the network is back, pull requests, builds and avatars
  are asked for again; avatars already downloaded still show meanwhile.
- HTTPS sign-in: when a remote wants a username and password (or token) and no credential helper
  provides them, a dialog asks and the operation runs again. They stay in memory for the tab;
  git's own credential helper, if there is one, stores them. Cloning asks the same way.
- Terminal panel with tabs: PowerShell, cmd, Git Bash, WSL… on Windows, your shells elsewhere, in a
  real pseudo-terminal (ConPTY on Windows) with colours, scrollback, mouse selection and copy
  (Ctrl+C with a selection, Ctrl+Shift+C or right-click).
- Pop out: the diff, the terminal, a pull request or the CI view moves into a window of its own
  (the button in its header, or the command palette), for a second monitor. It keeps working
  there (staging lines, comments, typing in the shell, undo) and stays open across tab switches
  and commits selected in the main window, which shows what was underneath and a reminder that
  raises the window. Closing the window or its bring-back button docks the view again; closing
  the view itself (its ×, or Esc outside the terminal) closes the window.
- Command palette (Ctrl+P): every action, ranked by how well it matches what you type (word
  starts, letters in a row, the start of the entry: `reb` finds _Rebase main onto…_ first, `cp`
  _Cherry-pick_), the letters you typed highlighted. With nothing typed, what can be done with
  the selected commit (_Cherry-pick a301da9c_, _Revert_, _Reset main to here…_) or the
  open file (_Stage_, _Blame_, _File history_) comes first, then what you ran lately (remembered
  across restarts). Verbs that need a branch (_Merge into main…_, _Rebase main onto…_, _Check out
  a branch…_, _Compare main with…_, _Push another branch…_) list the branches in a second step
  with their own search, so thousands of branches don't flood the list; Esc goes back, and so
  does Backspace in its empty field. Typing a branch's name in the first step also offers to
  check out the few that match best.
- Custom commands (Settings → Custom commands): your own command lines in a commit's, a branch's
  or a file's menu, or the palette. Each runs with `sh` on the repository's machine (here, in
  WSL or over SSH) in its folder, with `$SHA`, `$SHORT_SHA`, `$BRANCH`, `$REMOTE`, `$FILE` and
  `$REPO` (and the answers to its prompts: a text or a branch, asked first) as shell variables,
  never pasted into the command line. It can ask before running, and its output goes to a toast
  or to a terminal tab of its own. A repository can bring its own in `.gitcraft/commands.json`
  (`{"commands": [{"name": …, "where": "commit" | "branch" | "file" | "palette", "command": …,
  "prompts": [{"variable": "NAME", "label": …, "kind": "text" | "branch", "default": …}],
  "confirm": true, "output": "toast" | "terminal"}]}`); they are offered only once you have read
  the file and trusted it, and asked about again whenever it changes. `confirm` is `true` and
  `output` `"toast"` when left out; a field the format doesn't know makes the file unusable (the
  review says why). A toast shows only the last 8 lines of what a command printed.
- Manual (F1, or the book in the title bar): what each part does and every keyboard shortcut.
- _Copy diagnostics_ (at the top of the manual, or _Help: Copy diagnostics_ in the palette):
  what a bug report needs, as Markdown on the clipboard. gitcraft's version and channel, the
  system, the graphics adapter and backend wgpu draws with, the versions of git, `gh`, `glab`
  and `tea` and whether each has a login (`gh auth token`'s exit code, offline too; `glab auth
  status`, which asks its server; `tea`'s logins counted; never a token or an account), and the
  settings of the doctor's allowlist in effect in the repository shown, with their scope and
  file name. Never your name, email or signing key; a setting whose value is a path only says it
  is set, and any user's folder (`/home/x`, `C:\Users\x`, `/mnt/c/Users/x`) is written `~`.
  The tools are asked side by side; check the text before posting it.
- Avatars: on a GitHub repository, the authors' GitHub accounts (through `gh` when installed, else
  GitHub's API); on Forgejo and Gitea, the accounts the host ties the commits to; on GitLab, the
  host's avatar API; elsewhere Gravatar (a hash of the email).
  Cached on disk; can be turned off.
- Settings (Ctrl+, or the gear in the title bar), in pages listed on the left: the profile in
  use and what is its own, zoom, branch column width, graph columns, avatars, notifications,
  update check (with a Check for updates button), background fetch, keeping submodules up to
  date, editor command, external diff and merge tools, default shell and terminal font size, saved identities, custom commands,
  SSH sign-ins, agents, the AI tool and its prompts, and every keyboard shortcut. Ctrl+F searches all pages at once (Esc clears it); Alt+↑ /
  Alt+↓ go through the pages.
- Tabs in the title bar (Windows, Settings → Appearance): the tab strip takes the place of the
  system's title bar, as in a browser. Drag the space next to the tabs to move the window (it
  snaps to the screen's edges as usual), double-click it to maximize; minimize, maximize and
  close sit at the end of the strip, drawn as Windows 11 draws them, and the window's edges
  resize it.
- The settings, command palette, theme and manual buttons sit in the title bar, or in the footer
  of a repository's left or right panel, in any order: drag one there (it snaps into the opening
  made for it), right-click it, or pick in Settings → Appearance. On a folded panel they stack up
  its strip. Tabs that don't fit scroll sideways under the end of the strip (wheel or touchpad),
  with `+` and a list of those out of view next to a grip that moves the window; when the title
  bar gets too narrow its buttons go into the profile menu.
- Previews beside the settings: point at a setting and a panel on the right (above the page in
  a narrow window) draws what it does with the values set now, as the app draws it. The branch
  column measured on a small graph with long branch names cut to fit, the columns at their
  widths, standard and compact lanes side by side, the lane colours, each way of pulling as a
  before-and-after graph, the terminal with the shell's prompt at its font size, unified and
  side-by-side diffs, context or the whole file, files in folders or in one list, code colours,
  avatars, and the whole window in a theme.
- Commit signing in Settings → Commit signing: the format (OpenPGP, SSH or X.509), signing key
  and program, and whether commits and tags are signed by default. These are your global git
  config's `gpg.format`, `user.signingkey`, `gpg.program` (`gpg.ssh.program`, `gpg.x509.program`),
  `commit.gpgSign` and `tag.gpgSign`, shown as they are set and written back there; git does
  the signing as usual. A profile can sign its own way instead (Settings → Profile).
- How a profile's git author and signing reach git: gitcraft writes them to a config file of the
  profile's (in its data folder, `profiles/<id>.gitconfig`) that includes your own global config
  first and then sets the profile's values, and runs git with `GIT_CONFIG_GLOBAL` pointing at it
  (git 2.32 or later). So the profile wins over `~/.gitconfig`, and a repository's own config
  (`git config --local`, `--worktree`) wins over the profile, as it does over `~/.gitconfig`.
  `git config --global` from gitcraft still reads and writes your own file. The built-in
  terminal keeps your own config.
- Looks on GitHub for a newer release on every start, of the main channel (releases) or the dev
  channel (a prerelease of every change to `dev`). It downloads it in the background and parks it
  on disk; "Update now" in the banner then swaps it in place of the running version and restarts.
  A copy gitcraft can't replace (a .deb or .rpm, an all-users install) offers the release page
  instead. Looking and downloading can each be turned off.
- Going back after an update: the update keeps the version it replaced next to the program
  (`<name>.previous`, such as `gitcraft.exe.previous` or `gitcraft.app.previous`, its version in
  `<name>.previous-version`; delete both to free the space). Settings → General → Updates →
  _Go back to 0.3.3_ swaps the two and restarts, then stays on that version (no update is
  offered, and Check for updates is off) until _Unpin_. The button then reads _Go forward to
  0.3.4_: that swaps back to the newer one, unpinned. A version from
  before pins existed doesn't know them: it skips the version you went back from instead, and
  offers later ones.
- Toasts, and a desktop notification when a long operation ends while the window is in the
  background.
- Themes (Settings → Appearance, the palette icon in the title bar, or _Theme:_ in the command
  palette), each profile its own:
  - 40 built in: gitcraft's Dark, Medium Gray and Light (PhotoCraft's Pro palettes), High
    Contrast Dark and Light, and the published palettes of Catppuccin (Mocha, Macchiato, Frappé,
    Latte), Tokyo Night (Night, Storm, Day), Rosé Pine (main, Moon, Dawn), Kanagawa (Wave,
    Dragon, Lotus), Everforest (Dark, Light), Dracula, Nord, Gruvbox (Dark, Light), Solarized
    (Dark, Light), One Dark and One Light, GitHub (Dark, Light), Ayu (Dark, Mirage, Light),
    Monokai, Monokai Pro, Night Owl, Material Palenight, Tomorrow Night, Horizon and
    SynthWave '84. A theme colours everything: panels, text, the graph's lanes and pills, diffs
    and the terminal's 16 colours. A test keeps every one's text readable (WCAG contrast).
  - A gallery of thumbnails; pointing at one shows the whole window in it before choosing.
  - _Follow VS Code_: the interface takes the workbench colours of the theme VS Code (or
    Insiders, VSCodium) is using, and changes with it.
  - Themes from the internet: _Import theme file…_ takes a VS Code colour theme (`.json`, from an
    extension's `themes` folder or sites such as vscodethemes.com), a Base16 or Base24 scheme
    (`.yaml`, e.g. the tinted-theming collection) or a theme saved by gitcraft, and copies it
    into gitcraft's `themes` folder.
  - Your own: _Save a copy to edit_ writes the theme in use, every colour by name, as a file in
    that folder (`extends` names a theme to start from, so a file can set only an accent). Edit
    it in any editor and the window follows as you save; a file that doesn't read says why
    under the buttons.
  - For any theme: your own accent colour, rounder or square corners, and the graph's lanes in
    the theme's hues, gitcraft's classic ones or colour-blind-safe ones (Okabe–Ito).
- Keyboard and screen readers: Tab / Shift+Tab move through the toolbar, the sidebar's rows, the
  file lists, the graph, the tabs and the start page's cards; a painted button or row with the
  keyboard is ringed in the accent colour (egui's own buttons and fields show their pressed
  look), and Enter or Space clicks it. The graph is one Tab stop on the selected commit, ↑ / ↓
  moving the keyboard along. Each painted widget has a name for screen readers (AccessKit): a
  graph row reads "commit a301da9, Merge…, Jonas, 26 minutes ago, branch dev". Right-click
  menus and double-clicks (checking out a pill) still need the mouse.
- Reduced motion: when gitcraft starts it asks the system whether to animate (GNOME's
  `enable-animations`, KDE's animation speed, macOS' Reduce motion, Windows' "Animate windows
  when minimizing and maximizing"); with animations off, tabs and panels move into place without
  sliding. `GITCRAFT_REDUCE_MOTION=1` (or `0`) decides instead.
- When git fails in a way gitcraft knows, a card under the toolbar (or on the start page, for a
  repository that won't open) says what happened, with git's own words below. Where there is a
  fix, a button makes it, after saying exactly what it changes and in which config file; a
  config change can be undone from the card (until it is closed), and _Try again_ runs what
  failed again once, when it is the tab's last operation:
  - a repository owned by another user (_dubious ownership_): **Trust this folder** adds that
    folder, and only it (never `*`), to `safe.directory` in your global config;
  - a `.lock` file left behind (_index.lock exists_): how old it is and which git processes are
    running on that machine (on Windows and on another machine, gitcraft's own included), then
    **Remove stale lock** (only a `….lock` file in the
    repository's git folder, and only if nothing wrote it since);
  - _Filename too long_ in an open repository (a checkout, a pull): **Enable long paths** sets
    `core.longpaths` for the repository (Git for Windows; a failed clone isn't explained yet);
  - _SSL certificate problem_: **Use Windows' certificates** sets `http.sslBackend=schannel` in
    your global config (Git for Windows);
  - explained only: line-ending warnings (what `core.autocrlf` does, as it is set here, against
    `.gitattributes`, as it is for the file), _RPC failed_ / _early EOF_ / HTTP 413,
    _unrelated histories_, _shallow update not allowed_ and _Permission denied (publickey)_.
  A refused push is the push banner's. Everything it reads or changes runs in the background, on
  the repository's machine (WSL and SSH included).
- Themes: Dark, Medium Gray and Light, from PhotoCraft's Pro palettes.

## Download

Ready-made builds for Windows, Linux (x86-64) and macOS (Apple silicon and Intel) are on the
[releases page](https://github.com/Vancete/gitcraft/releases). Git must be installed. New
releases are built for Windows and Linux only for now: the release workflow's macOS job is off
until GitHub's macOS runners pick it up again, so on a Mac build from source (below) meanwhile.

- **Windows:** `gitcraft-…-setup.exe` installs it for your user (no administrator rights), with a
  Start menu entry, an optional desktop icon, _Open in gitcraft_ on folders and an uninstaller.
  The `.zip` is the same program without installing.
- **Linux:** `gitcraft-….deb` (Debian, Ubuntu, Mint…) and `gitcraft-….rpm` (Fedora, openSUSE…)
  install it with its applications menu entry, from the software centre or with
  `sudo apt install ./gitcraft-….deb` / `sudo dnf install ./gitcraft-….rpm`.
  `gitcraft-….AppImage` runs on any distribution: make it executable (`chmod +x`) and run it;
  _Add gitcraft to the applications menu_ on its start page (or in Settings) adds a menu entry,
  which follows the AppImage when you replace it with a newer one. The `.tar.gz` holds the bare
  binary and the `.desktop` files (the menu entry, the `gitcraft://` handler and Dolphin's service menu).
- **macOS** (11 or later): open `gitcraft-…-macos-universal.dmg` and drag gitcraft to
  Applications; the `.zip` holds the same app. It isn't notarized by Apple, so the first time
  macOS refuses to open it: go to _System Settings › Privacy & Security_ and click _Open
  Anyway_, or run `xattr -dr com.apple.quarantine /Applications/gitcraft.app`. git comes with
  the Xcode command line tools (`xcode-select --install`) or Homebrew.
- **Portable** (a USB stick, a locked-down machine): put the AppImage, or what the `.zip` or
  `.tar.gz` unpacks to, in a folder of its own, and add an empty `gitcraft.portable` file or an
  empty `data` folder next to the program (next to `gitcraft.app` on macOS, once
  `xattr -dr com.apple.quarantine` has been run on it). gitcraft then keeps its settings, open
  tabs, profiles and their `.gitconfig` files, the avatar cache, downloaded updates and Mergiraf
  in that `data` folder instead of your user's app data, starting fresh, and updates replace
  the files there. SSH passwords are kept in this computer's credential store only when
  Settings → General → Portable mode → _Remember SSH sign-ins in this computer's credential
  store_ is on (it starts off). If the folder can't be written to, gitcraft says so and uses the
  usual place. Git itself still uses your own config, credential helper and `~/.ssh`.

A release is built by `.github/workflows/release.yml` whenever a `v*` tag is pushed
(`packaging/windows/gitcraft.iss` for the installer, `packaging/linux/build-appimage.sh` for the
AppImage, `[package.metadata.deb]` / `[package.metadata.generate-rpm]` in `Cargo.toml` for the
packages, `packaging/macos/build-app.sh` for the universal app, its `.dmg` and `.zip`).

## Build and run

```bash
cargo run --release                   # start
cargo run --release -- path/to/repo   # open a repository directly
cargo test
```

Linux needs the usual eframe system packages (e.g. `libxkbcommon-dev`, `libgtk-3-dev` for the
file dialogs, `libdbus-1-dev` for notifications). Pull requests need the GitHub CLI (`gh`) or
GitLab CLI (`glab`), logged in.

Forgejo and Gitea need no tool of their own. gitcraft uses a login the user already has for the
host, and never stores one: `GITEA_SERVER_URL` + `GITEA_SERVER_TOKEN` when they name that server,
else a login of `tea` (`tea login add`, which works with Forgejo too), else what git's credential
helper keeps for `https://host` (a password or an access token). Public repositories can be read
without any. codeberg.org and hosts named like `forgejo…` / `gitea…` are recognised, as are the
hosts `tea` is logged in to; for any other server, tell git's config which forge it is:

```bash
git config --global gitcraft.git.example.org.forge forgejo   # every repository on that host
git config gitcraft.forge gitea                               # just this repository
```

The value is one of `github`, `gitlab`, `forgejo` or `gitea`.

### Gerrit

Hosts named `gerrit.…` and `*.googlesource.com` are recognised. For another host, set
`git config gitcraft.forge gerrit` (or `git config --global gitcraft.review.example.org.forge gerrit`).
The workflow uses `origin`, or the first remote when there is no origin, and git's existing
transport and credentials. Fetch first, then choose _Push for review_ in the toolbar, command
palette, or a branch's Push action. The dialog previews the exact commit and review ref.
The optional topic travels as a Git push option, so punctuation cannot become Gerrit options.

Install your server's `commit-msg` hook using its own instructions, and amend existing commits
that lack a `Change-Id`. gitcraft validates the complete series above the fetched target,
including commits outside the visible graph. Missing, malformed or duplicate IDs stop the push.
This flow submits linear series based on the target; fetch and rebase first when the target has
diverged. Merge commits are refused. It never changes the branch's upstream or force pushes.

Use _Amend last commit_ to update a reviewed change. Its original `Change-Id` is preserved even
when replacing the description; changing it to another ID is refused. _Push for review_ again
uploads the new patch set. The arrow on a graph row and _Open Gerrit change_ in its menu open
a project-qualified change search, which also works without knowing the change number.
Googlesource links use its corresponding `-review` host.
For SSH, the default web URL uses HTTPS without the SSH port. Override the review web address
(including a reverse-proxy prefix) with `git config gitcraft.gerritUrl https://review.example.org/gerrit`.

### Testing against a real Forgejo and Gitea

The Forgejo / Gitea code has tests that go through a pull request's whole life on real servers:
detection, opening, the list, reading it in full, fetching its changes, comments, pending reviews,
replies, dismissals, suggested changes and merging. `ci/forge-server.sh` downloads a release
(checksum verified), starts it on 127.0.0.1 with SQLite and two users, and writes their tokens to an
env file; CI does the same for the latest and the oldest supported releases (`.github/workflows/ci.yml`).

```bash
ci/forge-server.sh forgejo 3000 forges.env   # FORGEJO_VERSION=… for another release
ci/forge-server.sh gitea 3001 forges.env     # GITEA_VERSION=…
set -a; . ./forges.env; set +a
cargo test --lib forge::gitea::live -- --ignored --nocapture
```

A plain `cargo test` skips them.

### Remote repositories, against real machines

The tests of `src/remote/live_tests.rs` run only when given a machine:

```sh
GITCRAFT_TEST_SSH='build-host:~/repos/some-repo' cargo test live_tests -- --nocapture  # reads it
GITCRAFT_TEST_WSL=Ubuntu cargo test live_tests -- --nocapture  # works in a scratch repo in /tmp
GITCRAFT_TEST_WSL=Ubuntu cargo test never_holds_up_the_window  # drives the window against one
```

`cargo run --example ui_shot` ends by saying how many commands for another machine were asked
from the window's thread (each is a bug: refused, it would have frozen the window).

### Looking at the UI headlessly

`examples/ui_shot.rs` drives the app without a window (egui_kittest) and saves PNGs:

```bash
cargo run --example ui_shot -- path/to/repo /tmp/shot click:500,266 shot key:Escape shot
```

Steps: `click:x,y`, `dclick:x,y`, `rclick:x,y`, `cclick:x,y` (with Ctrl / Cmd), `sclick:x,y`
(with Shift), `drag:x1,y1,x2,y2`, `hover:x,y`, `wheel:x,y,dy` (negative scrolls down), `leave`,
`open:path` (another repository in a new tab),
`key:Name`, `ckey:Name` (with Ctrl / Cmd), `type:text`, `wait:ms`, `shot`. The README screenshot
was made this way, on a demo repository.

## Layout

| File                     | Job                                                                                |
| ------------------------ | ---------------------------------------------------------------------------------- |
| `src/git.rs`             | `git` CLI wrapper and output parsing (refs, log, status, stashes, remotes, search) |
| `src/git/simulate` | a merge, rebase or cherry-pick worked out with `git merge-tree`, without doing it |
| `src/git/errors.rs` | known git failures told apart by git's words; their fixes (`errors/fix.rs`, `errors/lock.rs`) and git's real output for tests (`errors/samples/`) |
| `src/error_card/` | the card that explains a git failure and offers its fix (confirmed, undoable) |
| `src/graph.rs`           | lane layout for the commit graph                                                   |
| `src/forge/mod.rs`       | the forges behind one `Provider` trait: which one hosts a remote, list, create, avatars, the page of one |
| `src/forge/github/`      | GitHub through `gh`: list, `gh pr view`, GraphQL threads, comments, reviews, merges |
| `src/forge/gitlab.rs`    | GitLab through `glab`: list, create, find by branch and retarget                   |
| `src/forge/gitea/`       | Forgejo and Gitea through their REST API: list, the page of one, comments, reviews, merges; finding the login |
| `src/forge/stack.rs`     | a stack's pull requests: planned, published bottom-up, retargeted, recorded         |
| `src/forge/http.rs`      | the REST client for forges without a command-line tool                             |
| `src/forge/cli.rs`       | running `gh` / `glab`                                                              |
| `src/forge/links/` | links to commits, branches, files and lines for each forge; the remote they go to |
| `ci/forge-server.sh`     | a throwaway Forgejo or Gitea for the live tests (`src/forge/gitea/live.rs`)        |
| `src/pull_request/mod.rs` | one pull request in full, whatever the forge: details, checks, comments, changes, safe markdown |
| `src/pr_post/mod.rs` | what goes out: the `Target` trait each forge implements, and what they share (suggestions, merge rules) |
| `src/pr_threads.rs` | review threads, the pending review and permissions                          |
| `src/codeowners/` | CODEOWNERS of GitHub, GitLab and Gitea / Forgejo: where each looks, its patterns, who owns a file, the check |
| `src/repo/owners/` | CODEOWNERS in the tab: owners on file rows, who a change needs, reviewers to suggest, the check's window |
| `src/forge/forks.rs` | where each forge's pull requests come from (forks), what their authors allow, runs awaiting approval |
| `src/repo/forks/` | pull requests from forks: badges, checking out to push to the contributor, rebase and push, tidying fork remotes |
| `src/release/` | cutting a release: versions, Conventional Commits, notes, manifests, each step |
| `src/repo/release/` | the release dialog: choices, what it will do, the steps as they run, resume and undo |
| `src/repo/hotfix/` | the hotfix flow: starting one from a tag, finishing it (tag, merges, push) and its undo |
| `src/avatars.rs`         | avatars from GitHub, Forgejo / Gitea, GitLab or Gravatar, cached on disk           |
| `src/update.rs`          | new-release check                                                                  |
| `src/desktop_entry.rs`   | applications menu entry for the AppImage                                           |
| `src/shell_path.rs`      | on macOS, the login shell's PATH for the commands gitcraft starts                  |
| `src/package/` | what a crate or npm package will publish: the tool's listing, what looks wrong, the registry's version |
| `src/repo/package/` | the panel that shows it |
| `src/remote/` | repositories in WSL or over SSH: their paths, the shells kept open there, their files |
| `src/agent/` | coding agents: the worktree setup recipe, the agent tasks, the terminal script that runs them |
| `src/repo/agents/` | the AGENTS section and the agent task and setup recipe dialogs |
| `src/agent/checkpoint.rs` | agent turns: `gitcraft checkpoint`, the turns of a worktree; `hooks.rs` installs the agents' hooks, `checkpoint.sh` is the script for other machines |
| `src/agent/handoff.rs` | hand-off: the messages typed into an agent's terminal (notes, conflicts, CI logs, threads), the notes tray |
| `src/agent/bakeoff/` | bake-off: comparing attempts, new dependencies, hunks, assembling a branch, builds |
| `src/repo/bakeoff/` | the bake-off page and its attempt picker |
| `src/app/title_bar.rs`   | Windows: the tab strip as the title bar (moving, caption buttons, resizing)       |
| `src/settings/`          | user settings, profiles, the settings pages, the theme gallery and the previews    |
| `src/ai/`                | AI commit messages and reviews: the prompt and diff cut to size, finding and running the tool on the repository's machine |
| `src/repo/ai/`           | the commit box's AI button and the branch review panel                             |
| `src/diagnostics.rs`     | Help → Copy diagnostics: what a bug report needs, nothing personal                 |
| `src/manual.rs`          | the manual (F1): features and keyboard shortcuts                                   |
| `src/keymap.rs`          | the keys of the main actions and tabs, read by handlers, manual, tooltips, menus   |
| `src/menu_item.rs`       | a menu entry with its key on the right and why it is greyed out under it           |
| `src/repo/keys.rs`       | what a repository tab's keys do, or why they can't now                             |
| `src/a11y/`              | keyboard focus ring, screen-reader names of painted widgets, reduced motion        |
| `src/theme/`             | colour tokens, fonts, egui style; themes: catalogue, palettes, theme files         |
| `src/icons.rs`           | embedded Lucide SVG icons, tinted                                                  |
| `src/terminal.rs`        | integrated terminal: pty, VT parsing (`vt100`), painting, keys, selection          |
| `src/repo/mod.rs`        | one repository tab: state, background git work, undo / redo, toolbar               |
| `src/mcp/`               | `gitcraft mcp`: the MCP server (`rmcp`), its tools, and the token-checked channel to the window |
| `src/repo/agent/`        | agents' requests in a tab: planned as undoable steps, queued, the approval card's state |
| `src/repo/timeline/`     | the timeline and undo tree: entries kept on disk, changes seen made elsewhere, jumps, the refs that keep old tips |
| `src/repo/sidebar.rs`    | branches, remotes, worktrees, stashes, pull requests, tags                         |
| `src/repo/pr_view.rs` | a pull request in the big area, its changed files on the right |
| `src/repo/pr_review.rs` | comments and reviews being written, and the threads on lines of a diff |
| `src/repo/pr_merge.rs` | the box that merges a pull request |
| `src/repo/push/` | Push and Sync; a refused push read from `--porcelain`, the remote checked, the banner and the force push with lease |
| `src/repo/pinned_sections.rs` | sidebar section headers that stay in view and jump to their section |
| `src/repo/branch_tree.rs` | branch names and file paths as a tree of folders, and its rows |
| `src/repo/branch_health.rs` | which local branches are merged (or rebased, or squash-merged), stale or gone |
| `src/repo/branch_cleanup.rs` | the branches _Select stale or merged branches…_ offers, read when the branches change |
| `src/repo/branch_order.rs` | pinned branches and the sort of _Local_, kept per repository |
| `src/repo/columns.rs` | widths of the graph's columns: what a drag changes and what gets squeezed |
| `src/repo/worktree_tab.rs` | worktrees in the same tab: parked views, the tab's chip and switcher |
| `src/repo/graph_view.rs` | commit graph, pills, search bar, commit menu                                       |
| `src/repo/detail.rs`     | commit / stash detail, changes and commit box                                      |
| `src/repo/detail/extras/` | a commit's signature, containing branches and tags, notes and backlinks          |
| `src/repo/diff.rs`       | diff loading, pairing and the diff view (unified, split, images)                   |
| `src/repo/diff_split.rs` | the diff below the graph instead of in its place |
| `src/repo/nav_history.rs` | the back / forward list of a tab's recent views |
| `src/repo/navigation/` | back and forward: the view shown as a place, showing one again, keys and buttons |
| `src/repo/fold/` | generated files and lockfiles folded in file lists and diffs |
| `src/repo/highlight.rs`  | syntax colours (syntect)                                                           |
| `src/vscode_theme.rs`    | VS Code's current colour theme as a syntect theme                                  |
| `src/repo/conflict.rs`   | three-pane conflict editor, each conflict's original, Keep both                    |
| `src/repo/conflict_check.rs` | conflict warnings in the menus and the toolbar, checked in the background and kept |
| `src/git/external_tool/` | external diff and merge tools: presets, how git starts them (here, in WSL, over SSH), how a merge ended |
| `src/repo/doctor/`       | the repository doctor: its page, sections and what they ask the tab to do          |
| `src/git/doctor/`        | what the doctor reads and changes: config origins, hidden flags, ignore rules and attributes, hooks and what they printed, health |
| `src/mergiraf.rs` | Mergiraf, run as the user's own command when it is installed |
| `src/repo/surgery/`      | history surgery: splitting a commit (`split/`), moving changes (`moving.rs`), absorbing staged fixes (`absorb/`), patches of some lines (`patchset.rs`) |
| `src/git/trees.rs`       | commits made in a scratch index (`tree_after`, `commit_tree`) for rewrites         |
| `src/repo/rebase.rs`     | interactive rebase editor and its todo list (`rebase/rows.rs`: its rows, drag, keys) |
| `src/repo/reflog/` | the reflog view and the scan for lost commits (`src/git/reflog/`: reading and parsing them) |
| `src/repo/drag_drop/` | drag and drop: what a drop offers, the drop menu, moving and squashing commits |
| `src/repo/dco/` | DCO sign-off: the commit box's toggle, commits without one, signing off a branch |
| `src/repo/versions/` | two versions of a series compared (range-diff), before / after rewrites, pull requests since last looked at |
| `src/git/range_diff.rs` | running and reading `git range-diff`; `rewrites.rs` and `reviewed.rs` keep the old versions in refs |
| `src/repo/file_view.rs`  | a file's history (or a few lines'), blame and contents at a commit                 |
| `src/repo/revert_piece.rs` | reverting a file, hunk or lines of a past commit into the working tree           |
| `src/git/stack/`         | stacked branches: the parents recorded in git's config, each layer's state, the parent suggested from history |
| `src/repo/stack/`        | stacks in the tab: the sidebar's STACKS, _This branch is based on…_, the graph's tooltips |
| `src/repo/file_text.rs`  | a whole file at a commit, coloured, for the file view and diffs                    |
| `src/repo/find/`         | find in the diff, file or blame shown: the bar, matching, marks, keys              |
| `src/custom_commands/`   | custom commands: what they are, the script each runs (its variables quoted), a repository's `.gitcraft/commands.json` and its trust |
| `src/repo/custom_commands/` | custom commands in a tab: menu and palette entries, the repository's file, running one (toast or terminal) |
| `src/repo/forge_link/` | the _Copy link to …_ / _Open … on GitHub_ menu entries, and the link made in the background |
| `src/repo/dialogs.rs`    | branch, tag, stash, remote, worktree, pull request and confirm dialogs             |
| `src/git/preview.rs`     | what a destructive operation would change, read before it runs                     |
| `src/repo/widgets.rs`    | shared painted widgets (panel headers, file rows, avatars, tabs)                   |
| `src/repo/palette/`      | what the palette offers a repository: commands, the selection's, branch verbs      |
| `src/repo/commit_actions.rs` | cherry-pick, revert and reset of a commit, for its menu and the palette        |
| `src/app.rs`             | window shell: tabs, start page, cloning, palette, settings, toasts                 |
| `src/app/palette/`       | the command palette: scored matching, recently used entries, the second step       |

Assets and their licences: [ATTRIBUTION.md](ATTRIBUTION.md).

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
