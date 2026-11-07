# Branches, worktrees, stashes and tags

The left panel: local and remote branches, stacked branches, worktrees, submodules, stashes and tags.

## Branches

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

## Stacked branches

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

## Remotes

- Remotes: add, edit (name and URL), fetch one, remove; their branches below each.

## Worktrees

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

## Submodules

- Submodules (shown when the repository has any): each with its commit and whether it is
  modified, not initialised or out of date. Update one or all (`git submodule update --init
  --recursive`), open one in a new tab (or double-click it), show it in its folder. _Keep
  submodules up to date_ in Settings → Git updates them after every checkout, pull, merge and
  rebase.

## Stashes

- Stashes: stash (with a message, untracked files included or not), see what a stash holds,
  apply, pop, drop. Stash only some files (`git stash push -- <paths>`: Ctrl+click files in the
  changes to pick them, or _Stash this file…_ on a file's menu), only what is staged (`git stash
  push --staged`, git 2.35+), or save a _Snapshot_: a stash of every change that leaves the
  working tree and the staging exactly as they are. All of them are undone with Ctrl+Z.

## Tags

- Tags: create (light or annotated, optionally pushed), push, delete locally or on the remote.
