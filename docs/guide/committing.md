# Staging and committing

The right panel: staging files, hunks or single lines, the commit box, sign-offs, hooks and AI commit messages.

## Staging

- Conflicted / unstaged / staged lists with stage, unstage, discard and mark-resolved buttons,
  _Stage all_ / _Unstage all_; right-click a file to open it, open it in your editor, show it in
  its folder, see its history or blame, or ignore it (the file, its extension or its folder go
  into `.gitignore`; a tracked file stops being tracked).
- _Path_ / _Tree_ at the top of the pane lists the files of the changes, a commit, a comparison
  or a pull request flat with their paths, or as a tree of their folders; a folder folds with a
  click and says how many files it holds. The choice is remembered.
- Part of a file: _Stage hunk_, _Discard hunk_ and _Unstage hunk_ on each hunk of its diff, or
  click the +/− gutter beside the changed lines you want (Shift+click for a range) and stage, unstage or discard only
  those. Discarding can be undone with Ctrl+Z.

## The commit box

- Commit box (Ctrl+Enter, from anywhere in the tab: with no summary yet it goes to the summary;
  with nothing staged it stages everything), _Amend last commit_
  (warns when the commit is already pushed), the identity you commit as (and whether it is the
  repository's own, the profile's or the global one), and saved identities or profile authors
  to switch a repository to; _Use the profile's author_ takes the repository's own out again.

## Templates, trailers and issue links

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

## AI commit messages

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

## Sign-off (DCO) and hooks

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

## Fixing older commits

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

## Reverting part of a past commit

- Undo part of a past commit: _Revert changes to this file_ in a commit's file list, _Revert
  file_, _Revert hunk_ and _Revert N lines_ (after clicking lines) in its diff. The piece is
  reverse-applied to the working tree (`git apply -R`), to look over before committing; when it
  applies as it is, Ctrl+Z takes it out again. When the file has changed since, a 3-way merge
  puts it in (staged); overlapping changes are left as conflicts to settle. Binary files and
  submodules are refused.
