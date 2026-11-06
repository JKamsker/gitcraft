# gitcraft

A visual git client: a simplified yet powerful take on the usual graph-based git GUIs.
Rust + [egui/eframe](https://github.com/emilk/egui), the same technical stack of the ArtCraft suite, so it builds on Windows, Linux and macOS and stays light and fast. It drives the installed `git` command line,
so your config, credentials and hooks apply as usual.

![gitcraft: branches, remotes and worktrees on the left, the graph with branch pills and relative dates, uncommitted changes on the right and the integrated terminal below](docs/screenshot.png)

## Features

**Repositories and tabs**

- Start page: checks that `git` is installed; open, create (`git init`) or clone a repository;
  recent repositories.
- Browser-like tabs, one per repository (Ctrl+T, Ctrl+W, middle-click; Ctrl+Shift+T reopens a
  closed one). Open tabs, recents, theme and settings are remembered.
- Profiles, as in GitKraken: switch between them from the title bar (or the palette). Each has
  its own open tabs, recent repositories, theme, git author and commit signing (Settings →
  Profile); the other settings pages are shared. Switching closes this profile's tabs and opens the
  other one's (refused while a tab has unfinished work).
- Changes made elsewhere (editor, terminal) show up by themselves: the folder is watched.
- Repositories on another machine, in a WSL distribution or over SSH: the window stays here and
  git runs there (see below).

**Remote repositories (WSL, SSH)**

- Start page → _Remote_ (or the palette's _Open remote repository_): pick a WSL distribution
  or an SSH host (the hosts of `~/.ssh/config` are offered), and the repository's folder there
  (`/home/me/repo` or `~/repo`), typed or found with _Browse…_, which lists that machine's
  folders and marks its repositories. A `\\wsl.localhost\…` folder chosen with _Open_ is a WSL
  repository too, and `gitcraft ssh://host/path/to/repo` opens one from the command line.
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
- Network operations (fetch, pull, push, clone) run in shells of their own, so the reads never
  wait behind them. A command that hangs is stopped there, with what it started, after 5
  minutes (30 for network operations).
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

**Graph**

- Every branch, coloured lanes, ↑/↓ to move. History loads 1500 commits at a time as you scroll.
- Branch and tag pills in their own column left of the graph, on each branch's tip only, joined
  to their commit: white text, a laptop icon when the branch is local, a cloud when a remote has
  it at that commit (`x` and `origin/x` make one pill), a pull-request icon when one is open,
  `+N` when more refs share the commit, the author's avatar and how long ago it was ("2 hours
  ago"). Author, date and hash columns are optional (Settings).
- Drag the column borders in the header to resize. Right-click the header to pick the columns,
  make the graph column compact (lanes closer together), or _Reset columns_ to their first
  widths.
- The checked-out branch's pill is solid, the others a darker shade. Double-click a pill to
  check that branch (or tag) out; hover `+N` to list every pill of the commit and double-click
  the one you want.
- Each commit's summary, followed by the rest of its message, faded and cut to fit.
- An _Uncommitted changes_ row on top while the working tree is dirty.
- Search (Ctrl+F) through messages, authors and hashes of all history, or switch to _Code_ to
  find the commits that added or removed a piece of text in a file (`git log -S`); Enter /
  Shift+Enter jump between results, loading older history when needed.
- Right-click a commit: create a branch, a tag or a worktree on a new branch there, check it
  out, interactive rebase onto it,
  cherry-pick, revert, reset the current branch to it (soft / mixed / hard, confirmed), the menu
  of each branch on it, copy the hash, message or branch name. Right-click a branch's pill to get
  check out, fast-forward, merge, rebase and interactive rebase for that branch on top.
- Compare: Ctrl+click a second commit (or _Compare with…_ in the commit menu, the changes row
  standing for the working tree) to list the files that differ between the two and open each
  diff. On a branch, _Changes x adds to main_ shows only what it changed since the two diverged,
  as a pull request shows it, and _Compare main with x_ every difference; a checkbox switches
  between the two. The right pane says how many commits each side has that the other hasn't.
- Patches: _Create patch from commit…_ saves a commit as a file (`git format-patch`). _Apply
  patch…_ (the latest commit's menu, the changes row's menu or the palette) turns one from
  `git format-patch` into commits (`git am`) and puts any other diff on the working tree
  (`git apply`). Ctrl+Z undoes either.
- Interactive rebase: the commits to rewrite, newest on top; pick, reword (edit the message in
  place), squash, fix up or drop each one, move them up and down, then _Start rebase_. Ctrl+Z
  undoes it.
- Bisect: _Start bisect here…_ in a commit's menu (or the palette) marks it good (HEAD is then
  the bad end) or bad, and checks out commits between the two for you to test. A banner over
  the graph names the commit to test and about how many steps are left, with _Good_, _Bad_,
  _Skip_ and _Stop_; the graph tints good, bad and skipped commits. _Run a command…_ has `git
  bisect run` test each commit on the repository's machine, its output streamed under the
  banner and stoppable. At the end the first bad commit is selected and _Finish_ (or _Stop_,
  which asks first) checks out again what was checked out when it began. A bisect started in a
  terminal is picked up and goes on from the banner. Refused while there are uncommitted
  changes or conflicts, or a merge, rebase, cherry-pick or revert is in progress.
- Test lane: _Test commits up to here…_ in a commit's menu (or _Test commits…_ in the palette)
  runs a command such as `cargo test` at each commit from the base branch, on the repository's
  machine, in scratch worktrees, several at once and in the background. A pass / fail square by
  each commit fills in as results come; click one for what the test printed. Results are kept
  by tree in git notes in git-test's format (`refs/notes/tests/default`, `test.default.command`),
  so `git test` and gitcraft share them and a rebase that keeps a commit's files keeps its
  result.
- Quick edits from a commit's menu, for commits of the checked-out branch: _Edit commit
  message_, _Drop commit_, _Move commit up_ / _down_, each a rebase without the editor and undone
  with Ctrl+Z. What can't be done (a merge, merges after the commit) is greyed out with why.

**Branches, remotes, worktrees, stashes, tags** (left)

- Local branches with ahead/behind; right-click: check out (or in a new worktree), fast-forward,
  merge into / (interactively) rebase onto the current branch, push, create a pull request,
  rename, delete (here, or here and on the remote in one go)…
- Hide a branch from the graph with the eye on its row (on hover), or _Solo_ some from their
  menu to see only those; the graph then has just the commits of the branches it shows and the
  checked-out one. _Viewing 12 of 55_ at the top says how many it shows, with _Show all_. Kept
  per repository.
- Remotes: add, edit (name and URL), fetch one, remove; their branches below each.
- Worktrees: a yellow dot in place of the folder icon marks the others with uncommitted changes,
  here and in the switcher. Add (existing or new branch), open in the same tab (the tab then shows the branch, a searchable switcher and a button to close the worktree, and what you left keeps running; can be turned off in Settings) or a new one, open a terminal there, lock, remove,
  prune.
- Submodules (shown when the repository has any): each with its commit and whether it is
  modified, not initialised or out of date. Update one or all (`git submodule update --init
  --recursive`), open one in a new tab (or double-click it), show it in its folder. _Keep
  submodules up to date_ in Settings → Git updates them after every checkout, pull, merge and
  rebase.
- Stashes: stash (with a message, untracked files included), see what a stash holds, apply, pop,
  drop.
- Pull / merge requests of GitHub (through `gh`), GitLab (through `glab`), Forgejo and Gitea
  (through their API; Codeberg is Forgejo): list, open in the browser, check out, create from a
  branch. On GitHub, Forgejo and Gitea a click opens one in the big area: description, checks,
  commits, conversation and changed files; comment, review (with comments on lines, pending until
  sent), reply, apply suggested changes, dismiss reviews and merge. Forgejo's and Gitea's API can't
  resolve conversations, so that stays on their web page. _Open in worktree_ (in a request's menu,
  and above the changed files of an open pull request) opens the worktree its branch is checked
  out in, or makes one beside the repository first: on the local branch, on a new one tracking
  the remote branch, or fetched from the forge for a fork that isn't a remote here.
- Tags: create (light or annotated, optionally pushed), push, delete locally or on the remote.

**Changes and commits** (right)

- Conflicted / unstaged / staged lists with stage, unstage, discard and mark-resolved buttons,
  _Stage all_ / _Unstage all_; right-click a file to open it, open it in your editor, show it in
  its folder, see its history or blame, or ignore it (the file, its extension or its folder go
  into `.gitignore`; a tracked file stops being tracked).
- _Path_ / _Tree_ at the top of the pane lists the files of the changes, a commit, a comparison
  or a pull request flat with their paths, or as a tree of their folders; a folder folds with a
  click and says how many files it holds. The choice is remembered.
- Part of a file: _Stage hunk_, _Discard hunk_ and _Unstage hunk_ on each hunk of its diff, or
  click the changed lines you want (Shift+click for a range) and stage, unstage or discard only
  those. Discarding can be undone with Ctrl+Z.
- Commit box (Ctrl+Enter; with nothing staged it stages everything), _Amend last commit_
  (warns when the commit is already pushed), the identity you commit as (and whether it is the
  repository's own, the profile's or the global one), and saved identities or profile authors
  to switch a repository to; _Use the profile's author_ takes the repository's own out again.
- Undo / redo (Ctrl+Z / Ctrl+Y) of commits, amends, merges, rebases, pulls, resets,
  cherry-picks, reverts, checkouts, branch create / rename / delete, tags and stashes.

**Diffs and conflicts** (centre)

- Unified or side by side (a new file takes the whole width, with no empty old side), syntax
  colours, the changed part of each line marked, whitespace changes optional, images shown
  before / after. Esc closes.
- A file kept in Git LFS shows as its LFS objects before and after, with their sizes, rather
  than the text of the pointer git stores. With git-lfs installed, the palette (Ctrl+P) of a
  repository that uses LFS has _LFS: Fetch_ and _LFS: Pull_.
- The arrows in a diff's header (F7 / Shift+F7, or Alt+↓ / Alt+↑) jump to the next and previous
  change. The strip beside the scrollbar marks where the changes are, removals red and additions
  green; click a mark to jump to its change, or anywhere else on the strip to scroll there.
- The _Diff_ / _File_ switch shows the whole file as it is after the change instead (before it,
  for a deleted file), coloured and with line numbers.
- A Markdown file's diff has an eye button that renders it: the changes block by block (removed
  ones red, added ones green, the rest as they are) or, with _File_, the whole file. Images of
  the repository show as they are on each side of the change; any other image is a link, so a
  file can't make gitcraft contact a server.
- Code takes the colours of the theme VS Code (or Insiders, VSCodium) is using, read from its
  settings and extensions, when it is dark or light like gitcraft's; a setting turns it off.
- Conflicts open in a three-pane editor: each conflict's two sides with _Ours_, _Theirs_ and
  _Both_ buttons, and the result below (editable). _Use mine_ / _Use theirs_ for a whole file.
- With [Mergiraf](https://mergiraf.org) installed, _Solve with Mergiraf_ lets it solve the
  conflicts it can from the syntax of the code; the rest stay to choose, and nothing is saved
  before you check the result. Settings → Git installs it in one click and can have git merge
  with it everywhere ([docs/mergiraf.md](docs/mergiraf.md)). gitcraft only runs the `mergiraf`
  program: it neither bundles nor links Mergiraf (GPL-3.0), so gitcraft's licence is unchanged.
- A merge, rebase, cherry-pick, revert or patch stopped on conflicts shows a banner with
  _Abort_; the commit box finishes it (_Continue rebase_ during a rebase, _Continue patch_ while
  `git am` applies one).
- File history (every commit that changed the file, following renames; click one for its diff
  or the file as it was, beside the list), blame (who last changed each line, newer changes
  marked brighter; click to go to the commit) and the whole file at a commit (_View file_ in a
  file's menu).
- _View all files_ under a commit's files lists every file of the commit, not only those it
  changed; click one to read it as it was in that commit.

**Toolbar, terminal, app**

- Undo, redo, Fetch, Pull and Push (with incoming / outgoing counts), new branch, stash, terminal,
  search, open the folder (Explorer / Finder, your editor), refresh (F5).
- Pull's arrow (or a right-click on it) sets what it does: fast-forward, merging when the branch
  has diverged; fast-forward only; or rebase. Also in Settings, and the menu has _Fetch all_.
- Background fetch every few minutes (Settings; 0 turns it off).
- HTTPS sign-in: when a remote wants a username and password (or token) and no credential helper
  provides them, a dialog asks and the operation runs again. They stay in memory for the tab;
  git's own credential helper, if there is one, stores them. Cloning asks the same way.
- Terminal panel with tabs: PowerShell, cmd, Git Bash, WSL… on Windows, your shells elsewhere, in a
  real pseudo-terminal (ConPTY on Windows) with colours, scrollback, mouse selection and copy
  (Ctrl+C with a selection, Ctrl+Shift+C or right-click).
- Command palette (Ctrl+P): every action and branch, fuzzy matched.
- Manual (F1, or the book in the title bar): what each part does and every keyboard shortcut.
- Avatars: on a GitHub repository, the authors' GitHub accounts (through `gh` when installed, else
  GitHub's API); on Forgejo and Gitea, the accounts the host ties the commits to; on GitLab, the
  host's avatar API; elsewhere Gravatar (a hash of the email).
  Cached on disk; can be turned off.
- Settings (Ctrl+, or the gear in the title bar), in pages listed on the left: the profile in
  use and what is its own, zoom, branch column width, graph columns, avatars, notifications,
  update check (with a Check for updates button), background fetch, keeping submodules up to
  date, editor command, default shell and terminal font size, saved identities, SSH sign-ins,
  and every keyboard shortcut. Ctrl+F searches all pages at once (Esc clears it); Alt+↑ /
  Alt+↓ go through the pages.
- Commit signing in Settings → Commit signing: the format (OpenPGP, SSH or X.509), signing key
  and program, and whether commits and tags are signed by default. These are your global git
  config's `gpg.format`, `user.signingkey`, `gpg.program` (`gpg.ssh.program`, `gpg.x509.program`),
  `commit.gpgSign` and `tag.gpgSign`, shown as they are set and written back there; git does
  the signing as usual. A profile can sign its own way instead (Settings → Profile).
- How a profile's git author and signing reach git: gitcraft writes them to a config file of the
  profile's (in its data folder, `profiles/<id>.gitconfig`) that includes your own global config
  first and then sets the profile's values, and runs git with `GIT_CONFIG_GLOBAL` pointing at it
  (git 2.32 or later). So the profile wins over `~/.gitconfig`, and a repository's own config
  (`git config --local`, `--worktree`) wins over the profile, as it does over `~/.gitconfig`.
  `git config --global` from gitcraft still reads and writes your own file. The built-in
  terminal keeps your own config.
- Looks on GitHub for a newer release on every start, of the main channel (releases) or the dev
  channel (a prerelease of every change to `dev`). It downloads it in the background and parks it
  on disk; "Update now" in the banner then swaps it in place of the running version and restarts.
  A copy gitcraft can't replace (a .deb or .rpm, an all-users install) offers the release page
  instead. Looking and downloading can each be turned off.
- Toasts, and a desktop notification when a long operation ends while the window is in the
  background.
- Themes: Dark, Medium Gray and Light, from PhotoCraft's Pro palettes.

## Download

Ready-made builds for Windows, Linux (x86-64) and macOS (Apple silicon and Intel) are on the
[releases page](https://github.com/Vancete/gitcraft/releases). Git must be installed. New
releases are built for Windows and Linux only for now: the release workflow's macOS job is off
until GitHub's macOS runners pick it up again, so on a Mac build from source (below) meanwhile.

- **Windows:** `gitcraft-…-setup.exe` installs it for your user (no administrator rights), with a
  Start menu entry, an optional desktop icon, _Open in gitcraft_ on folders and an uninstaller.
  The `.zip` is the same program without installing.
- **Linux:** `gitcraft-….deb` (Debian, Ubuntu, Mint…) and `gitcraft-….rpm` (Fedora, openSUSE…)
  install it with its applications menu entry, from the software centre or with
  `sudo apt install ./gitcraft-….deb` / `sudo dnf install ./gitcraft-….rpm`.
  `gitcraft-….AppImage` runs on any distribution: make it executable (`chmod +x`) and run it;
  _Add gitcraft to the applications menu_ on its start page (or in Settings) adds a menu entry,
  which follows the AppImage when you replace it with a newer one. The `.tar.gz` holds the bare
  binary and a `.desktop` file.
- **macOS** (11 or later): open `gitcraft-…-macos-universal.dmg` and drag gitcraft to
  Applications; the `.zip` holds the same app. It isn't notarized by Apple, so the first time
  macOS refuses to open it: go to _System Settings › Privacy & Security_ and click _Open
  Anyway_, or run `xattr -dr com.apple.quarantine /Applications/gitcraft.app`. git comes with
  the Xcode command line tools (`xcode-select --install`) or Homebrew.

A release is built by `.github/workflows/release.yml` whenever a `v*` tag is pushed
(`packaging/windows/gitcraft.iss` for the installer, `packaging/linux/build-appimage.sh` for the
AppImage, `[package.metadata.deb]` / `[package.metadata.generate-rpm]` in `Cargo.toml` for the
packages, `packaging/macos/build-app.sh` for the universal app, its `.dmg` and `.zip`).

## Build and run

```bash
cargo run --release                   # start
cargo run --release -- path/to/repo   # open a repository directly
cargo test
```

Linux needs the usual eframe system packages (e.g. `libxkbcommon-dev`, `libgtk-3-dev` for the
file dialogs, `libdbus-1-dev` for notifications). Pull requests need the GitHub CLI (`gh`) or
GitLab CLI (`glab`), logged in.

Forgejo and Gitea need no tool of their own. gitcraft uses a login the user already has for the
host, and never stores one: `GITEA_SERVER_URL` + `GITEA_SERVER_TOKEN` when they name that server,
else a login of `tea` (`tea login add`, which works with Forgejo too), else what git's credential
helper keeps for `https://host` (a password or an access token). Public repositories can be read
without any. codeberg.org and hosts named like `forgejo…` / `gitea…` are recognised, as are the
hosts `tea` is logged in to; for any other server, tell git's config which forge it is:

```bash
git config --global gitcraft.git.example.org.forge forgejo   # every repository on that host
git config gitcraft.forge gitea                               # just this repository
```

The value is one of `github`, `gitlab`, `forgejo` or `gitea`.

### Testing against a real Forgejo and Gitea

The Forgejo / Gitea code has tests that go through a pull request's whole life on real servers:
detection, opening, the list, reading it in full, fetching its changes, comments, pending reviews,
replies, dismissals, suggested changes and merging. `ci/forge-server.sh` downloads a release
(checksum verified), starts it on 127.0.0.1 with SQLite and two users, and writes their tokens to an
env file; CI does the same for the latest and the oldest supported releases (`.github/workflows/ci.yml`).

```bash
ci/forge-server.sh forgejo 3000 forges.env   # FORGEJO_VERSION=… for another release
ci/forge-server.sh gitea 3001 forges.env     # GITEA_VERSION=…
set -a; . ./forges.env; set +a
cargo test --lib forge::gitea::live -- --ignored --nocapture
```

A plain `cargo test` skips them.

### Remote repositories, against real machines

The tests of `src/remote/live_tests.rs` run only when given a machine:

```sh
GITCRAFT_TEST_SSH='build-host:~/repos/some-repo' cargo test live_tests -- --nocapture  # reads it
GITCRAFT_TEST_WSL=Ubuntu cargo test live_tests -- --nocapture  # works in a scratch repo in /tmp
GITCRAFT_TEST_WSL=Ubuntu cargo test never_holds_up_the_window  # drives the window against one
```

`cargo run --example ui_shot` ends by saying how many commands for another machine were asked
from the window's thread (each is a bug: refused, it would have frozen the window).

### Looking at the UI headlessly

`examples/ui_shot.rs` drives the app without a window (egui_kittest) and saves PNGs:

```bash
cargo run --example ui_shot -- path/to/repo /tmp/shot click:500,266 shot key:Escape shot
```

Steps: `click:x,y`, `dclick:x,y`, `rclick:x,y`, `cclick:x,y` (with Ctrl / Cmd), `sclick:x,y`
(with Shift), `drag:x1,y1,x2,y2`, `hover:x,y`, `wheel:x,y,dy` (negative scrolls down), `leave`,
`open:path` (another repository in a new tab),
`key:Name`, `ckey:Name` (with Ctrl / Cmd), `type:text`, `wait:ms`, `shot`. The README screenshot
was made this way, on a demo repository.

## Layout

| File                     | Job                                                                                |
| ------------------------ | ---------------------------------------------------------------------------------- |
| `src/git.rs`             | `git` CLI wrapper and output parsing (refs, log, status, stashes, remotes, search) |
| `src/graph.rs`           | lane layout for the commit graph                                                   |
| `src/forge/mod.rs`       | the forges behind one `Provider` trait: which one hosts a remote, list, create, avatars, the page of one |
| `src/forge/github/`      | GitHub through `gh`: list, `gh pr view`, GraphQL threads, comments, reviews, merges |
| `src/forge/gitlab.rs`    | GitLab through `glab`: list and create                                             |
| `src/forge/gitea/`       | Forgejo and Gitea through their REST API: list, the page of one, comments, reviews, merges; finding the login |
| `src/forge/http.rs`      | the REST client for forges without a command-line tool                             |
| `src/forge/cli.rs`       | running `gh` / `glab`                                                              |
| `ci/forge-server.sh`     | a throwaway Forgejo or Gitea for the live tests (`src/forge/gitea/live.rs`)        |
| `src/pull_request/mod.rs` | one pull request in full, whatever the forge: details, checks, comments, changes, safe markdown |
| `src/pr_post/mod.rs` | what goes out: the `Target` trait each forge implements, and what they share (suggestions, merge rules) |
| `src/pr_threads.rs` | review threads, the pending review and permissions                          |
| `src/avatars.rs`         | avatars from GitHub, Forgejo / Gitea, GitLab or Gravatar, cached on disk           |
| `src/update.rs`          | new-release check                                                                  |
| `src/desktop_entry.rs`   | applications menu entry for the AppImage                                           |
| `src/shell_path.rs`      | on macOS, the login shell's PATH for the commands gitcraft starts                  |
| `src/remote/` | repositories in WSL or over SSH: their paths, the shells kept open there, their files |
| `src/settings/`          | user settings, profiles, and the settings pages                                    |
| `src/manual.rs`          | the manual (F1): features and keyboard shortcuts                                   |
| `src/theme.rs`           | colour tokens, fonts, egui style (PhotoCraft's Pro themes)                         |
| `src/icons.rs`           | embedded Lucide SVG icons, tinted                                                  |
| `src/terminal.rs`        | integrated terminal: pty, VT parsing (`vt100`), painting, keys, selection          |
| `src/repo/mod.rs`        | one repository tab: state, background git work, undo / redo, toolbar               |
| `src/repo/sidebar.rs`    | branches, remotes, worktrees, stashes, pull requests, tags                         |
| `src/repo/pr_view.rs` | a pull request in the big area, its changed files on the right |
| `src/repo/pr_review.rs` | comments and reviews being written, and the threads on lines of a diff |
| `src/repo/pr_merge.rs` | the box that merges a pull request |
| `src/repo/pinned_sections.rs` | sidebar section headers that stay in view and jump to their section |
| `src/repo/branch_tree.rs` | branch names and file paths as a tree of folders, and its rows |
| `src/repo/columns.rs` | widths of the graph's columns: what a drag changes and what gets squeezed |
| `src/repo/worktree_tab.rs` | worktrees in the same tab: parked views, the tab's chip and switcher |
| `src/repo/graph_view.rs` | commit graph, pills, search bar, commit menu                                       |
| `src/repo/detail.rs`     | commit / stash detail, changes and commit box                                      |
| `src/repo/diff.rs`       | diff loading, pairing and the diff view (unified, split, images)                   |
| `src/repo/highlight.rs`  | syntax colours (syntect)                                                           |
| `src/vscode_theme.rs`    | VS Code's current colour theme as a syntect theme                                  |
| `src/repo/conflict.rs`   | three-pane conflict editor                                                         |
| `src/mergiraf.rs` | Mergiraf, run as the user's own command when it is installed |
| `src/repo/rebase.rs`     | interactive rebase editor and its todo list                                        |
| `src/repo/file_view.rs`  | a file's history, blame and contents at a commit                                   |
| `src/repo/file_text.rs`  | a whole file at a commit, coloured, for the file view and diffs                    |
| `src/repo/dialogs.rs`    | branch, tag, stash, remote, worktree, pull request and confirm dialogs             |
| `src/repo/widgets.rs`    | shared painted widgets (panel headers, file rows, avatars, tabs)                   |
| `src/app.rs`             | window shell: tabs, start page, cloning, palette, settings, toasts                 |

Assets and their licences: [ATTRIBUTION.md](ATTRIBUTION.md).

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
