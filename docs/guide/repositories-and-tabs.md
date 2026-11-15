# Repositories, tabs and profiles

Opening repositories, the start page, tabs, profiles, and what gitcraft remembers for you.
Repositories in WSL or on another machine over SSH have [a page of their own](remote-repositories.md).

## The start page

- Start page: checks that `git` is installed; open, create (`git init`) or clone a repository;
  recent repositories, each with its branch, a dot for uncommitted changes and when it was last
  opened (read in the background). Type to filter them, ↑ / ↓ and Enter to open one.
- Drop a folder on the window to open it, or a `.patch` / `.diff` / `.eml` / `.mbox` on a
  repository tab to apply it.
- _Scan a folder for repositories_ finds the repositories a few folders down and lists them to
  open or add to the recent list, or _Save as root_ to keep finding them (see
  [All repositories](#all-repositories-roots-and-nearby-folders)).
- _All repositories_, under Recent: every repository gitcraft found for this profile, in folders
  you configured and next to repositories you opened, together with the Recent ones.
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

## All repositories: roots and nearby folders

**Recent** stays what it always was: the last 12 repositories you opened, in the order you
opened them. **All repositories** is a separate list of every repository gitcraft found for the
profile in use, as many as there are (up to 5,000), plus the Recent ones. Nothing found there
is added to Recent, opened in a tab or fetched: a row opens its repository as _Open_ does (and
only then does it go into Recent).

- **Roots** are folders you keep repositories in. Add one with _Repository roots… → Add a root
  folder…_ on the start page, from Settings → Profile → Repository roots, or with _Save as root_
  after _Scan a folder_. Each root has a **depth**: 0 is only the folder itself (when it is a
  repository), 1 (the default) its folders, up to 4 folders down. A repository inside another
  one is listed too when it is within reach. A root can be turned off (its repositories leave the
  list until it is turned on again), pointed at another folder, rescanned, stopped or removed.
  Turning off or removing a root never closes a tab or takes anything out of Recent or a
  workspace.
- **Nearby** folders are inferred: when you open a repository on this computer, the other
  repositories in its folder (its siblings, one level deep) are listed as _Nearby_. Opening
  `/work/projects/app` lists `/work/projects/tool`, but `/work` and folders further up are never
  looked in. It never looks in your home folder itself, a drive or file system root, a top-level
  system folder (`/Users`, `/home`, `/mnt`, `/Volumes`, `C:\Users`…), the temporary folder, a
  network share (`\\server\share`), WSL or a repository on another machine. The last five
  nearby folders are kept; they never become roots unless you add them. Switch it off in
  _Repository roots… → List repositories next to the ones I open_.
- **Filter and search**: _All_, _Recent_, _Nearby_ or one root; type to search by name or path,
  ↑ / ↓ and Enter to open, Esc to clear. Each row says where it was found. _Hide from library_
  (the eye on a row) keeps a repository out of the list for good, whatever finds it again; it
  stays in Recent and can still be opened (_Repository roots… → Show them again_ brings hidden
  ones back). Removing a repository from Recent is a different thing: the × on a Recent row.
- **When it scans**: the roots a couple of seconds after the window opens (never while it starts),
  then again when their last scan is 15 minutes old while the window is in use; a nearby folder
  when a repository in it is opened and it wasn't looked at in the last 15 minutes. _Rescan_ and
  _Rescan all_ scan at once. There are no folder watchers and no scanning while gitcraft isn't
  in use.
- **Limits**: at most two scans run at once, in the background. A root scan stops after 5,000
  folders, 1,000 repositories, 200,000 folder entries or 30 seconds; a nearby scan after 256
  folders, 128 repositories, 4,096 entries or 5 seconds. Hidden folders, links (and Windows
  junctions) and folders like `node_modules`, `target`, `build` or `vendor` are not looked in.
  A scan cut short says _Partial_ (and why); one that couldn't look at all (a folder gone, a
  drive not plugged in, not allowed) says _Unavailable_. A folder that is slow to answer holds
  its scan's slot until it answers; other scans wait rather than pile up.
- **What is kept**: the roots, the nearby folders and the list are saved with gitcraft's state,
  per profile, never inside your repositories, and shown at once on the next start. A complete
  scan takes away the repositories that are gone from that folder (unless another root or a
  nearby folder still has them); a partial or failed one keeps them, marked _Stale_. Only
  folders are read: gitcraft runs no git command, hook or credential helper for a repository it
  found until you open it, so a row's branch and changes show only when it is also in Recent.

## Tabs and keyboard shortcuts

- Browser-like tabs, one per repository (Ctrl+T, Ctrl+W, middle-click; Ctrl+Shift+T reopens a
  closed one; Ctrl+Tab / Ctrl+Shift+Tab go round them, also on macOS; Ctrl+1 … 8 and Ctrl+9 for
  the last pick one, Cmd on macOS). Open tabs, recents, theme and settings are remembered.
- Keys for the main actions of a repository tab: Fetch (Ctrl+Shift+F), Pull (Ctrl+Shift+L), Push
  (Ctrl+Shift+K), Stash (Ctrl+Shift+S), New branch (Ctrl+B), Terminal (Ctrl+J), Stage all
  (Ctrl+Shift+A) and Commit (Ctrl+Enter). Cmd on macOS. None of them fires while a text field or
  the terminal has the keyboard (Ctrl+Enter in the commit message still commits). The toolbar's tooltips, the palette and the context menus show
  each key, and a greyed-out menu entry says why under it.

## Profiles

- Profiles, as in GitKraken: switch between them from the title bar (or the palette). Each has
  its own open tabs, recent repositories, theme, git author and commit signing (Settings →
  Profile); the other settings pages are shared. Switching closes this profile's tabs and opens the
  other one's (refused while a tab has unfinished work).

## One window

- One window: `gitcraft <folder>` or _Open in gitcraft_ while gitcraft is running opens that
  folder in a tab of the running window (or switches to its tab) and brings it to the front; a
  bare `gitcraft` just brings it to the front. Where the system doesn't let a window come to
  the front by itself (Windows, Wayland), its taskbar button flashes instead. The handoff goes over 127.0.0.1 with a random
  token kept in a file of the data folder only you can read. Every build shares that folder, so
  `cargo run` while an installed gitcraft runs goes to that window: close it first.

## Staying in sync and keeping your work

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

## First steps and tips

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

## Lazy tabs

- Lazy tabs: tabs restored on start read the repository and watch its folder only once they are
  first shown, so many saved tabs don't mean as many history loads and folder watchers at
  startup (nor background fetches until then). A tab left in the background for 30 minutes with
  nothing going on (no operation, running terminal, typed message, open editor, unsent review,
  pull request or builds view) drops its graph, diff and commit details and stops watching,
  fetching and following builds; shown again, it reads afresh (the opening screen meanwhile),
  then selects the same commit and opens the same diff if they are still there.
