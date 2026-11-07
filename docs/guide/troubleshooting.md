# Troubleshooting

When git fails, gitcraft explains why and offers a fix. The repository doctor answers the "why does git do that here?" questions.

## When git fails

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

## Repository doctor

Open it with the wrench in the toolbar, or _Doctor: …_ in the command palette.

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

## Manual and bug reports

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
