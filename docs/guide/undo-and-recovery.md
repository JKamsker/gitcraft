# Undo and recovery

Almost everything gitcraft does can be undone with Ctrl+Z. For what it can't reach, there are the reflog, the lost-commit scan and the timeline.

## Undo and redo

- Undo / redo (Ctrl+Z / Ctrl+Y) of commits, amends, merges, rebases, folds, pulls, resets,
  cherry-picks, reverts, checkouts, branch create / rename / delete, tags and stashes.
- _What will this do_: before a reset, deleting a branch (here, on the remote or both),
  dropping a commit or discarding a file, the dialog lists the commits leaving the branch (for a
  delete, those the checked-out branch hasn't), each marked _still on x_ or _only here_; the
  files whose uncommitted changes will be lost; that a force push will be needed when the
  commits rewritten are already on the upstream; the git commands it runs, folded; and
  _Ctrl+Z restores this_ when undo covers it. It is read in the background (also for
  repositories in WSL or over SSH); long lists end with _and 340 more_.

## Reflog and lost commits

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

## Timeline and undo tree

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
