# Merge conflicts

The three-pane conflict editor, Mergiraf, conflict warnings before you act, external merge tools and remembered resolutions.

## The conflict editor

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
  with it everywhere ([Mergiraf setup](../mergiraf.md)). gitcraft only runs the `mergiraf`
  program: it neither bundles nor links Mergiraf (GPL-3.0), so gitcraft's licence is unchanged.

## Resolving in the background with an agent

- _Resolve all with agent_ on the banner of a merge, rebase, cherry-pick or revert in progress
  (or _Resolve in background_ in the conflict editor, for that file) hands the conflicted files
  to Claude Code, or to Codex when Settings → AI picks it, running headless. You go on working
  meanwhile; nothing in your checkout changes until you apply the result.
- The agent works in a private copy of the checkout: a scratch worktree in the temporary
  folder, not listed among the worktrees and removed when the run ends (or, after a crash, the
  next time gitcraft makes one). The operation is replayed there so it sees all of it, and each
  conflicted file is copied in as it is in your checkout, with what you already chose there.
- A card over the big area shows its progress: the plan it reported (a step per file and any
  check it makes) as a bar of steps and a list, each step's note and time, what it is doing
  now, the time gone and an estimate of the time left (its own at first, then from the steps
  done). Fold it to a pill that keeps counting; _Show agent_ on the banner brings it back.
  _Stop_ ends the agent; what it resolved by then can still be applied.
- When it is done, the card lists each file: resolved, deleted (as one side did), still
  conflicted, or changed by you meanwhile (left as you made it). _Apply_ writes the files you
  tick and marks them resolved; _Discard_ forgets the agent's work.
- It may read the files and git (`git diff`, `log`, `show`, `blame`) and gitcraft's own reading
  tools, and edit its copy's files; it can't stage, commit, continue the operation or run
  anything else. It reports its plan and steps with the `resolve_plan`, `resolve_step` and
  `resolve_finish` tools of `gitcraft mcp`, which only an agent gitcraft started this way is
  offered. For repositories on this machine only, for now.

## Warnings before you act

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

## External diff and merge tools

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

## Understanding and remembering conflicts

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
