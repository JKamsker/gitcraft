# Toolbar, terminal and palette

Fetching, pulling and pushing, the activity log, the built-in terminal, pop-out windows, the command palette and custom commands.

## Toolbar

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

## Pushing and progress

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

## Taskbar and jump list

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

## Background fetch and offline mode

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

## Terminal and pop-out windows

- Terminal panel with tabs: PowerShell, cmd, Git Bash, WSL… on Windows, your shells elsewhere, in a
  real pseudo-terminal (ConPTY on Windows) with colours, scrollback, mouse selection and copy
  (Ctrl+C with a selection, Ctrl+Shift+C or right-click).
- Pop out: the diff, the terminal, a pull request or the CI view moves into a window of its own
  (the button in its header, or the command palette), for a second monitor. It keeps working
  there (staging lines, comments, typing in the shell, undo) and stays open across tab switches
  and commits selected in the main window, which shows what was underneath and a reminder that
  raises the window. Closing the window or its bring-back button docks the view again; closing
  the view itself (its ×, or Esc outside the terminal) closes the window.

## Command palette

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

## Custom commands

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
