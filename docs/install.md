# Installing gitcraft

Ready-made builds are on the [releases page](https://github.com/JKamsker/gitcraft/releases).
The only thing gitcraft needs besides itself is [git](https://git-scm.com/downloads), installed
and on the `PATH`; the start page checks for it.

> **macOS:** new releases are built for Windows and Linux only for now (the release workflow's
> macOS job is off until GitHub's macOS runners pick it up again). On a Mac,
> [build from source](development.md#build-and-run) meanwhile; the instructions below apply to
> the macOS builds of earlier releases.

## Windows

`gitcraft-…-windows-x86_64-setup.exe` installs it for your user (no administrator rights needed),
with a Start menu entry and an uninstaller. Optional, ticked in the installer: a desktop icon,
_Open in gitcraft_ on folders, _History_ / _Blame in gitcraft_ on files, and the `gitcraft://`
link handler.

The `.zip` is the same program without installing.

## Linux

- **Debian, Ubuntu, Mint…:** `sudo apt install ./gitcraft-…-linux-x86_64.deb`
- **Fedora, openSUSE…:** `sudo dnf install ./gitcraft-…-linux-x86_64.rpm`
- **Any distribution:** make `gitcraft-…-linux-x86_64.AppImage` executable (`chmod +x`) and run it.
  _Add gitcraft to the applications menu_ on its start page (or in Settings) adds a menu entry,
  which follows the AppImage when you replace it with a newer one.

The packages come with an applications menu entry, the `gitcraft://` link handler and a Dolphin
service menu. The `.tar.gz` holds the bare binary and those `.desktop` files.

## macOS

macOS 11 or later. Open `gitcraft-…-macos-universal.dmg` and drag gitcraft to Applications; the
`.zip` holds the same app.

It isn't notarized by Apple, so the first time macOS refuses to open it: go to
_System Settings › Privacy & Security_ and click _Open Anyway_, or run

```sh
xattr -dr com.apple.quarantine /Applications/gitcraft.app
```

git comes with the Xcode command line tools (`xcode-select --install`) or Homebrew.

## Portable mode

For a USB stick or a locked-down machine: put the AppImage, or what the `.zip` or `.tar.gz`
unpacks to, in a folder of its own, and add an empty `gitcraft.portable` file or an empty `data`
folder next to the program (next to `gitcraft.app` on macOS, once
`xattr -dr com.apple.quarantine` has been run on it).

gitcraft then keeps its settings, open tabs, profiles and their `.gitconfig` files, the avatar
cache, downloaded updates and Mergiraf in that `data` folder instead of your user's app data,
starting fresh, and updates replace the files there. SSH passwords are kept in this computer's
credential store only when Settings → General → Portable mode → _Remember SSH sign-ins in this
computer's credential store_ is on (it starts off). If the folder can't be written to, gitcraft
says so and uses the usual place. Git itself still uses your own config, credential helper and
`~/.ssh`.

## Optional tools

None of these are needed to use gitcraft; each one turns on more of it.

| Tool | What it adds |
| --- | --- |
| [`gh`](https://cli.github.com) (signed in) | GitHub pull requests, checks, issues and releases |
| [`glab`](https://gitlab.com/gitlab-org/cli) (signed in) | GitLab merge requests, pipelines, issues and releases |
| [`tea`](https://gitea.com/gitea/tea) | a login gitcraft can use for Forgejo and Gitea (not required; see [forges](guide/pull-requests.md#setting-up-forges)) |
| [Mergiraf](https://mergiraf.org) | solving conflicts from the syntax of the code ([setup](mergiraf.md)) |
| [git-lfs](https://git-lfs.com) | LFS fetch, pull, locks and storage |
| `claude`, `codex`, `gh copilot`… | AI commit messages and branch reviews (Settings → AI) |

## Updates

- gitcraft looks on GitHub for a newer release on every start, of the main channel (releases) or
  the dev channel (a prerelease of every change to `dev`). It downloads it in the background and
  parks it on disk; _Update now_ in the banner then swaps it in place of the running version and
  restarts. A copy gitcraft can't replace (a .deb or .rpm, an all-users install) offers the
  release page instead. Looking and downloading can each be turned off.
- Going back after an update: the update keeps the version it replaced next to the program
  (`<name>.previous`, such as `gitcraft.exe.previous` or `gitcraft.app.previous`, its version in
  `<name>.previous-version`; delete both to free the space). Settings → General → Updates →
  _Go back to 0.3.3_ swaps the two and restarts, then stays on that version (no update is
  offered, and Check for updates is off) until _Unpin_. The button then reads _Go forward to
  0.3.4_: that swaps back to the newer one, unpinned. A version from before pins existed doesn't
  know them: it skips the version you went back from instead, and offers later ones.

## Building from source

See [Development](development.md#build-and-run).
