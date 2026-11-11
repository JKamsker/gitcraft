# Remote repositories (WSL and SSH)

Work on a repository that lives in a WSL distribution or on another machine over SSH: the window stays on your computer, git runs over there.

## Opening one

- Start page → _Remote_ (or the palette's _Open remote repository_): pick a WSL distribution
  or an SSH host (the hosts of `~/.ssh/config` are offered), and the repository's folder there
  (`/home/me/repo` or `~/repo`), typed or found with _Browse…_, which lists that machine's
  folders and marks its repositories. A `\\wsl.localhost\…` folder chosen with _Open_ is a WSL
  repository too, and `gitcraft ssh://host/path/to/repo` opens one from the command line.

## How it works

- The window never waits for that machine: everything asked of it is asked in the background,
  and a slow or unreachable machine only shows as work still going on (anything asked of it
  from the window's thread is refused rather than run, and a test drives the window against a
  WSL repository to keep it so).
- git, and every file read or written in the repository (diffs of the working tree, staging
  lines, the conflict editor, `.gitignore`, rebase todo lists), runs on that machine, with its
  own git, config, credentials and hooks. Nothing has to be installed there but git and a POSIX
  `sh` with the tools any Linux, BSD or macOS has: `mktemp -d` (each shell's temporary
  folder), `base64 -d` (what a command reads, a patch or a file's new contents), `cat` and
  `wc` (passing output back), `stat` (GNU or BSD: noticing changes), and `ps` with `awk`
  (stopping a command that hung, and all it started).
- A few shells per machine are kept open (`ssh -T host sh`, `wsl.exe -d distro --exec sh`), so a
  command costs one round trip, not a new connection; history is read page by page in them
  too. SSH signs in as it does in a terminal (keys, an agent, `~/.ssh/config`;
  `GITCRAFT_SSH_CONFIG` names another config): what it asks — a password, a key's
  passphrase, whether to trust a host seen for the first time — is asked in the window (ssh
  runs gitcraft as its `SSH_ASKPASS`). A password or passphrase typed is kept for that machine
  while gitcraft runs, so the other shells don't ask again, and forgotten as soon as ssh says
  it was wrong. Tick **Remember connection** to keep it after a restart too: once ssh has
  signed in with it (a wrong one is never saved), it is saved encrypted in the system's
  credential store (Windows Credential Manager, the macOS Keychain, the Secret Service on
  Linux), never in gitcraft's own files, and forgotten when it stops working or with
  **Forget saved SSH sign-ins** in the command palette (or Settings → Git).
- Names with a `\` in them (Linux allows one) work like any other.

## What behaves differently

- Network operations (fetch, pull, push, clone) run in processes of their own, so the reads
  never wait behind them, and show git's progress as they go. A command that hangs is stopped
  there, with what it started, after 5 minutes (30 for network operations); _Cancel_ stops one
  at once the same way.
- Changes made there show up by themselves: the repository is looked at every few seconds
  (every 15 while the window isn't focused).
- The terminal opens a shell on that machine, in the repository, whatever the login shell
  (bash, zsh, fish, csh…). Pull requests and builds use `gh` / `glab` of this machine, told
  which repository, and only for github.com, gitlab.com or a host they're signed in to.
  Forgejo and Gitea are asked from this machine too, with this machine's login for the host.
- Over SSH, programs of this machine can't open the files (reveal in the file manager, open in
  editor, Mergiraf). In WSL they can, through `\\wsl.localhost`.
- Settings that write this machine's global git config (commit signing, Mergiraf) don't apply
  there, nor do a profile's author and signing: that machine's own git config does (the commit
  box says `· global (SSH · host)`).

## Teleport: move work to another machine

- **Teleport** (a tab's right-click menu, or the palette's _Teleport to another machine…_):
  carries the tab's work to another machine — this one, a WSL distribution, an SSH host — where
  it arrives in a new worktree, a tab opening on it: the branch with its commits, the staged
  changes still staged, the unstaged ones still unstaged, and the untracked files (not the
  ignored ones). No push, forge or cloud is involved, and nothing of the source is changed: a
  snapshot is taken through a temporary index (one tree for the index, one for the working
  tree), sent as a `git bundle` of only the commits the other machine lacks, through the shells
  already open to it, checked there (`git bundle verify`, and every object as it is unpacked)
  and then checked out. The work goes in a clone of the same project found there (by its
  remotes' URLs, among the recent and open repositories), another repository named, a clone
  made first (the work then goes in its own working tree), or a new repository made from the
  bundle. A bundle over 100 MB asks first. Submodules, uncommitted files kept in Git LFS, files
  marked assume-unchanged, a shallow clone, a branch checked out there already (or there with
  commits this one lacks) and a merge or rebase in progress are refused, with the reason. What
  it made (temporary refs, index and bundle, a half-made worktree, a clone made for it) is
  removed again, also when it fails or is cancelled.
