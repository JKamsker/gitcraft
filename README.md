<div align="center">

<img src="assets/app-icon/gitcraft-256.png" alt="" width="96" height="96">

# gitcraft

**A fast, friendly git client for Windows, Linux and macOS.**<br>
The commit graph you know from the big git GUIs, without the weight — and with an undo button for almost everything.

[![CI](https://github.com/JKamsker/gitcraft/actions/workflows/ci.yml/badge.svg)](https://github.com/JKamsker/gitcraft/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/JKamsker/gitcraft?label=release)](https://github.com/JKamsker/gitcraft/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

[**Download**](https://github.com/JKamsker/gitcraft/releases/latest) ·
[Documentation](docs/README.md) ·
[Report a bug](https://github.com/JKamsker/gitcraft/issues)

![gitcraft: branches, remotes and worktrees on the left, the graph with branch pills and relative dates, uncommitted changes on the right and the integrated terminal below](docs/screenshot.png)

</div>

## What is gitcraft?

gitcraft is a visual git client: open a repository and you see its whole history as a graph,
your branches on the left and your changes on the right. Stage, commit, branch, merge, rebase,
review pull requests and resolve conflicts with the mouse or the keyboard — and press **Ctrl+Z**
when something didn't go the way you wanted.

It is written in Rust with [egui](https://github.com/emilk/egui), the same stack as the ArtCraft
suite, so it starts quickly and stays light. Under the hood it runs the `git` you already have
installed, so your config, credentials and hooks all keep working, and anything you do
in a terminal shows up in gitcraft right away.

## Highlights

- 🌳 **A clear commit graph.** Every branch in coloured lanes, branch and tag pills, avatars and
  relative dates, search with operators like `author:`, `path:` and `after:2w`.
- ↩️ **Undo almost anything.** Commits, merges, rebases, resets, deleted branches and stashes
  come back with Ctrl+Z. A timeline keeps the history of what you did across restarts, and the
  reflog view finds what was lost elsewhere.
- 👀 **See it before you do it.** Hover a rebase, reset or force push and the graph shows the
  result first. Merge and rebase menus warn about conflicts before you click.
- ✂️ **History editing without the fear.** Interactive rebase with drag and drop, split a commit,
  move changes between commits, fold staged fixes into older commits, bisect (even in parallel).
- 🧩 **Staging down to the line.** Stage, unstage or discard files, hunks or single lines. A
  three-pane conflict editor, with optional [Mergiraf](https://mergiraf.org) for syntax-aware
  merges.
- 🔀 **Pull requests built in.** GitHub, GitLab, Forgejo / Codeberg and Gitea: list, review,
  comment, check out, merge, and see what changed since you last looked. Stacked branches and
  stacked pull requests too.
- 🔍 **Diffs that understand the file.** Side by side, images (swipe, onion skin, difference),
  Jupyter notebooks, Office documents and PDFs, JSON / YAML / CSV by key or as a table, and
  lockfiles as "which packages changed".
- 🖥️ **Remote repositories.** Work on a repository in WSL or on a server over SSH: the window
  stays on your machine, git runs over there.
- 🤖 **Made for coding agents.** Start Claude Code, Codex and others in their own worktrees,
  review each turn, compare attempts, and let agents use gitcraft through its MCP server — with
  every change undoable and risky ones waiting for your approval.
- 🎨 **Make it yours.** 40 themes (Catppuccin, Tokyo Night, Dracula, Nord, Solarized…), VS Code
  themes, profiles with their own git identity, a built-in terminal, and a command palette that
  finds every action.

Curious about everything it can do? The [user guide](docs/README.md#user-guide) covers each part
in detail.

## Install

Download the latest build from the [releases page](https://github.com/JKamsker/gitcraft/releases/latest).
gitcraft only needs [git](https://git-scm.com/downloads) to be installed.

| Platform | Download | Notes |
| --- | --- | --- |
| **Windows** | `…-windows-x86_64-setup.exe` | Installs for your user, no admin rights needed. A portable `.zip` is there too. |
| **Debian / Ubuntu** | `…-linux-x86_64.deb` | `sudo apt install ./gitcraft-….deb` |
| **Fedora / openSUSE** | `…-linux-x86_64.rpm` | `sudo dnf install ./gitcraft-….rpm` |
| **Any Linux** | `…-linux-x86_64.AppImage` | `chmod +x`, then run it. |
| **macOS** | [build from source](docs/development.md#build-and-run) | Prebuilt Mac builds are paused for now. |

gitcraft keeps itself up to date (you can turn that off, or go back to the previous version in
one click). More details, including portable mode for a USB stick, are in
[docs/install.md](docs/install.md).

## Getting started

1. **Open a repository.** Start gitcraft and pick _Open_, _Clone_ or _Create_ on the start page —
   or drop a folder on the window, or run `gitcraft .` in a terminal.
2. **Look around.** Click a commit to see what it changed, double-click a branch pill to check it
   out, right-click anything for what you can do with it.
3. **Commit.** Your changes are on the right: stage what you want, write a message, press
   **Ctrl+Enter**.
4. **Share it.** Push, pull and fetch are in the toolbar. To work with pull requests, sign in to
   [`gh`](https://cli.github.com) or [`glab`](https://gitlab.com/gitlab-org/cli); Forgejo and
   Gitea [need no extra tool](docs/guide/pull-requests.md#setting-up-forges).

A few keys worth knowing (Cmd instead of Ctrl on macOS):

| Key | Does |
| --- | --- |
| **Ctrl+P** | Command palette: every action, by name |
| **Ctrl+Z** / **Ctrl+Y** | Undo / redo |
| **Ctrl+Enter** | Commit |
| **Ctrl+Shift+F** / **L** / **K** | Fetch / pull / push |
| **Ctrl+F** | Search history (or within the open diff) |
| **Ctrl+J** | Terminal |
| **Ctrl+T** / **Ctrl+W** | New tab / close tab |
| **F1** | The manual, with every shortcut |

## Documentation

- [User guide](docs/README.md#user-guide): every feature, area by area.
- [Installing gitcraft](docs/install.md): all platforms, portable mode, optional tools, updates.
- [Pull requests and forges](docs/guide/pull-requests.md): setting up GitHub, GitLab, Forgejo and Gitea.
- [Coding agents](docs/guide/coding-agents.md): agent worktrees and the MCP server.
- [Troubleshooting](docs/guide/troubleshooting.md): when git fails, and the repository doctor.

## Building from source

You need a recent Rust toolchain (see `rust-version` in `Cargo.toml`) and git:

```bash
git clone https://github.com/JKamsker/gitcraft.git
cd gitcraft
cargo run --release
```

On Linux, install the usual eframe system packages first (e.g. `libxkbcommon-dev`,
`libgtk-3-dev`, `libdbus-1-dev`). See [Development](docs/development.md) for tests, headless
screenshots and how releases are made.

## Contributing

Bug reports, ideas and pull requests are welcome. Please open them in the
[issue tracker](https://github.com/JKamsker/gitcraft/issues). In the app, _Help: Copy diagnostics_
in the command palette puts what a bug report needs on your clipboard (nothing personal).

To find your way around the code, start with [Development](docs/development.md) and the
[code layout](docs/code-layout.md).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Bundled fonts, icons and theme palettes are listed with their licences in
[ATTRIBUTION.md](ATTRIBUTION.md).
