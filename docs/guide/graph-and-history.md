# The graph and history

Reading history: the commit graph, branch pills, search, comparing commits, and a file's history and blame.

## The graph

- Every branch, coloured lanes, ↑/↓ to move. History loads 1500 commits at a time as you scroll.
- Branch and tag pills in their own column left of the graph, on each branch's tip only, joined
  to their commit: white text, a laptop icon when the branch is local, a cloud when a remote has
  it at that commit (`x` and `origin/x` make one pill), a pull-request icon when one is open,
  `+N` when more refs share the commit, the author's avatar and how long ago it was ("2 hours
  ago"). Author, date and hash columns are optional (Settings).
- Description takes the flexible width. Branch / Tag fits visible pills and shrinks when none
  are in view; dragging its border keeps a fixed width. Restore _Fit visible branch / tag
  pills_ from the header's menu. Faint branch names (or _detached_) identify lanes whose tips
  are offscreen. Long summaries end in an ellipsis; hover the description to read its original
  message, loaded in the background. Messages over the 256 KiB tooltip limit open in the detail
  pane by selecting the commit.
- Drag the column borders in the header to resize. Right-click the header to pick the columns,
  make the graph column compact (lanes closer together), or _Reset columns_ to their first
  widths.
- The branch column can shrink to 70 px for short names such as `dev`. Long pill names keep
  their end (`…xyz`) by default; Settings → Graph → Shortened branch names can keep the start
  (`xyz…`) instead. Pill icons give way to the name when the column is narrow.
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

## AI authorship

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

## Search

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

## Commit menu

- Right-click a commit: create a branch, a tag or a worktree on a new branch there, check it
  out, interactive rebase onto it,
  cherry-pick, revert, reset the current branch to it (soft / mixed / keep / hard, confirmed with what it changes;
  _Keep_ drops the commits after it but keeps uncommitted changes, and stops with git's message
  if a file with changes would be touched), the menu
  of each branch on it, copy the hash, message or branch name. Right-click a branch's pill to get
  check out, fast-forward, merge, rebase and interactive rebase for that branch on top.

## Comparing

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

## Comparing versions of a series

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

## File history and blame

- File history (every commit that changed the file, following renames; click one for its diff
  or the file as it was, beside the list), the history of a few lines (select them in a diff or
  a file, right-click → _History of these lines_: every commit up to that version that touched
  them, `git log -L`, followed back through earlier edits and renames, each showing only what it
  did to them), blame (who last changed each line, newer changes
  marked brighter; click to go to the commit) and the whole file at a commit (_View file_ in a
  file's menu).
- _View all files_ under a commit's files lists every file of the commit, not only those it
  changed; click one to read it as it was in that commit.

## Links to the forge

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

## Commit details

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
