# Rewriting history

Interactive rebase, splitting and moving commits, bisecting, drag and drop, and previews that show what an operation will do before it runs. Everything here is undoable with Ctrl+Z.

## Patches

- Patches: _Create patch from commit…_ saves a commit as a file (`git format-patch`). _Apply
  patch…_ (the latest commit's menu, the changes row's menu or the palette) turns one from
  `git format-patch` into commits (`git am`) and puts any other diff on the working tree
  (`git apply`). Ctrl+Z undoes either.

## Interactive rebase

- Interactive rebase: the commits to rewrite, newest on top; pick, reword (edit the message in
  place), edit (stop there to change the commit; _Continue rebase_ amends it with what is staged
  and goes on), squash, fix up
  or drop each one, then _Start rebase_. Drag a row by its handle to move it, or click a row and
  use the keys: P, R, E, S, F, D set its action, ↑ / ↓ select another, Alt+↑ / Alt+↓ move it.
  Ctrl+Z undoes it.

## Quick edits

- Quick edits from a commit's menu, for commits of the checked-out branch: _Edit commit
  message_, _Drop commit_ (which asks first), _Move commit up_ / _down_, each a rebase without
  the editor and undone with Ctrl+Z. What can't be done (a merge, merges after the commit) is greyed out with why.

## Drag and drop

- Drag and drop: drag a branch's pill (or its row in the left pane) onto another pill, a commit
  or a branch in the left pane, and a small menu offers what makes sense there: _Merge x into
  main_, _Rebase main onto x_, _Interactive rebase main onto x…_, or _Fast-forward_ when one
  branch is only behind the other, which also works for a branch that isn't checked out. Drag a
  commit onto the checked-out branch to cherry-pick it, and a commit of the checked-out branch
  onto another commit of that branch to move it right above or below that one, or squash or fix
  it up into it. The targets that take what is dragged light up, the graph scrolls near its
  edges, Esc cancels, and each entry runs the same operation as the right-click menu, undone
  with Ctrl+Z.

## Ghost run: see the result first

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

## Split and move commits

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

## Distribute staged fixes (absorb)

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

## Bisect

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

## Testing commits

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

## Find the interaction

- Find the interaction: two branches pass alone but fail together. _Find the interaction…_ on a
  branch tests subsets of its commits and the checked-out branch's, applied where the two
  diverged, in a scratch worktree (delta debugging, `ddmin`), until it can say _these two
  commits fail together; either alone passes_. It has a test budget, and re-runs a failure
  before believing it, setting flaky results aside.
