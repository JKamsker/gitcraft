# gitcraft documentation

New here? Start with the [README](../README.md) and [Installing gitcraft](install.md). Inside the
app, **F1** opens the manual with every keyboard shortcut, and **Ctrl+P** (Cmd+P on macOS) finds
any action by name.

## User guide

| Page | What's in it |
| --- | --- |
| [Repositories, tabs and profiles](guide/repositories-and-tabs.md) | the start page, workspaces, _Where's my work?_, the pull-request inbox, tabs, profiles, drafts that survive a crash |
| [Command line, file manager and links](guide/command-line.md) | `gitcraft .`, `gitcraft log`, `blame`, `show`, `compare`; right-click menus; `gitcraft://` links |
| [Remote repositories (WSL and SSH)](guide/remote-repositories.md) | git running on another machine while the window stays here; Teleport |
| [The graph and history](guide/graph-and-history.md) | the commit graph, search operators, comparing, file history and blame, commit details |
| [Rewriting history](guide/rewriting-history.md) | interactive rebase, drag and drop, ghost-run previews, split / move / absorb, bisect, test lanes |
| [Branches, worktrees, stashes and tags](guide/branches-and-worktrees.md) | branch cleanup, stacked branches and restacking, worktrees, submodules, stashes |
| [Staging and committing](guide/committing.md) | staging hunks and lines, the commit box, templates, sign-off, AI commit messages, fixups |
| [Diffs](guide/diffs.md) | side by side, pictures, notebooks, LFS, Unity, binary files, prose / JSON / CSV / lockfile views |
| [Merge conflicts](guide/conflicts.md) | the conflict editor, Mergiraf, warnings before you merge, external tools, rerere |
| [Undo and recovery](guide/undo-and-recovery.md) | undo / redo, the reflog, lost commits, the timeline and undo tree |
| [Pull requests, issues and forges](guide/pull-requests.md) | GitHub, GitLab, Forgejo and Gitea setup; reviews, forks, stacked pull requests, CODEOWNERS |
| [Releases and packages](guide/releases.md) | cutting a release from Conventional Commits, hotfixes, what a package will publish |
| [Coding agents](guide/coding-agents.md) | agent tasks in worktrees, per-turn checkpoints, bake-offs, the MCP server |
| [Toolbar, terminal and palette](guide/app.md) | fetch / pull / push, offline mode, the terminal, pop-out windows, custom commands |
| [Settings and appearance](guide/settings-and-appearance.md) | settings, 40 themes and your own, title-bar tabs, commit signing, accessibility |
| [Troubleshooting](guide/troubleshooting.md) | git errors explained and fixed, the repository doctor, copying diagnostics for a bug report |

## Other topics

- [Installing gitcraft](install.md): every platform, portable mode, optional tools, updates.
- [Mergiraf](mergiraf.md): syntax-aware conflict solving.

## Contributing

- [Development](development.md): building, testing (also against real forges and machines),
  headless screenshots, releases.
- [Code layout](code-layout.md): where things live in `src/`.
- [CPU performance](cpu-performance.md), [workflow suite](cpu-workflows.md),
  [process orchestration](process-orchestration.md): measurements and how to reproduce them.
