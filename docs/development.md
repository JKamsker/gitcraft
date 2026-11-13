# Development

gitcraft is a single Rust crate (plus the small `crates/gitcraft-os`) built on
[egui/eframe](https://github.com/emilk/egui). It never links libgit2: every git operation runs the
installed `git` command line, so a user's config, credentials and hooks apply as usual.

- [Code layout](code-layout.md): where things live in `src/`.
- [Architecture](architecture.md): ownership conventions, reproducible hotspot metrics,
  lifecycle/operation inventories and the phased refactoring plan.
- [CPU performance](cpu-performance.md) and the [workflow suite](cpu-workflows.md): reproducible
  CPU benchmarks, stack profiles and source-line coverage.
- [Process orchestration](process-orchestration.md): subprocess limits, lifecycle handling and
  before/after stress measurements.
- [CI and releases](ci.md): the verification contract, the required check, merge-queue
  settings, the pre-push command (`python3 ci/check.py`) and how to read a failed job.

## Build and run

You need a Rust toolchain (1.95 or newer; see `rust-version` in `Cargo.toml`) and git.

```bash
cargo run --release                   # start
cargo run --release -- path/to/repo   # open a repository directly
cargo test
```

- The default syntax highlighter compiles bundled Oniguruma with the platform's C compiler.
  `cargo run --release --no-default-features` selects the pure-Rust regex backend instead.
- Linux needs the usual eframe system packages (e.g. `libxkbcommon-dev`, `libgtk-3-dev` for the
  file dialogs, `libdbus-1-dev` for notifications).
- gitcraft is one window: while an installed gitcraft is running, `cargo run` hands its folder
  to that window and exits. Close the installed one first.

## Rust source size

Try to keep each Rust source file below **300 physical lines**; **never exceed
500**. At 300–500 lines the checker warns with the path/count and asks for cohesive
decomposition. At 501 or more it fails. This covers every Git-tracked `.rs` file
anywhere, including tests, examples, build scripts, generated and vendored files.
No exclusions, grandfathering, or suppressions are supported. Preserve clear
ownership rather than minifying code or splitting responsibilities arbitrarily.

With Python 3.10+ and Git, run before committing:

```sh
python3 ci/rust_file_size.py                  # tracked paths, working contents
python3 ci/rust_file_size.py --staged         # complete index, not just changed files
python3 ci/rust_file_size.py --revision HEAD  # immutable committed candidate
python3 -m unittest discover -s ci -p test_rust_file_size.py -v
```

LF separates physical lines; CRLF gives the same count. Empty files have zero
lines; a final nonempty fragment counts even without a newline. Comments and
blank lines count, and Unicode line-separator characters are not LF delimiters.
Git supplies the paths, so ignored/untracked build output is excluded while
tracked ignored/generated/vendor Rust files still count. Working mode omits
unstaged deletions; index/revision modes check their own snapshots, including
staged renames/deletions. Symlinks count their Git link contents, never an external
target. Unmerged Rust entries or unreadable content fail closed.

The unconditional `rust-file-size` job in CI (pull requests, merge-queue
candidates, pushes to dev/master/main and tags) and in tag/manual release builds
calls the shared workflow and checks `--revision HEAD`. For PRs, checkout supplies the candidate merge tree. Downstream
checks/builds depend on this gate, and publishing depends on those builds.
CI remains authoritative even when local checks are skipped. The lexical
[architecture survey](architecture.md) stays advisory alongside this hard cap.

## Tests

With Python 3.11+ and Clippy installed, run `python ci/clippy_packages.py` to lint
every tracked Cargo package with locked dependencies, all targets, and warnings
as errors. This includes independent workspaces and members omitted from a
workspace's defaults. A new tracked package is included automatically; there are
no package exemptions. Missing or linked manifests fail before compilation.
The isolated recovery harness selects `--cfg recovery_recording`, its intended
recording profile; other packages use their default features and configuration.
Only code active for the selected platform, features, and cfg is analyzed. This
does not claim coverage of every inactive configuration.

A plain `cargo test` runs everything that needs no outside machine or server. The tests below are
skipped unless you give them one.

### Against a real Forgejo and Gitea

The Forgejo / Gitea code has tests that go through a pull request's whole life on real servers:
detection, opening, the list, reading it in full, fetching its changes, comments, pending reviews,
replies, dismissals, suggested changes and merging. `ci/forge-server.sh` downloads a release
(checksum verified), starts it on 127.0.0.1 with SQLite and two users, and writes their tokens to
an env file; CI does the same for the latest and the oldest supported releases
(`.github/workflows/ci.yml`).

```bash
ci/forge-server.sh forgejo 3000 forges.env   # FORGEJO_VERSION=… for another release
ci/forge-server.sh gitea 3001 forges.env     # GITEA_VERSION=…
set -a; . ./forges.env; set +a
cargo test --lib forge::gitea::live -- --ignored --nocapture
```

### Remote repositories, against real machines

The tests of `src/remote/live_tests.rs` run only when given a machine:

```sh
GITCRAFT_TEST_SSH='build-host:~/repos/some-repo' cargo test live_tests -- --nocapture  # reads it
GITCRAFT_TEST_WSL=Ubuntu cargo test live_tests -- --nocapture  # works in a scratch repo in /tmp
GITCRAFT_TEST_WSL=Ubuntu cargo test never_holds_up_the_window  # drives the window against one
```

`cargo run --example ui_shot` ends by saying how many commands for another machine were asked
from the window's thread (each is a bug: refused, it would have frozen the window).

## Looking at the UI headlessly

`examples/ui_shot.rs` drives the app without a window (egui_kittest) and saves PNGs:

```bash
cargo run --example ui_shot -- path/to/repo /tmp/shot click:500,266 shot key:Escape shot
```

Steps: `click:x,y`, `dclick:x,y`, `rclick:x,y`, `cclick:x,y` (with Ctrl / Cmd), `sclick:x,y`
(with Shift), `drag:x1,y1,x2,y2`, `hover:x,y`, `wheel:x,y,dy` (negative scrolls down), `leave`,
`open:path` (another repository in a new tab), `key:Name`, `ckey:Name` (with Ctrl / Cmd),
`type:text`, `wait:ms`, `shot`. The screenshot in the README was made this way, on a demo
repository.

## Releases

A release is built by `.github/workflows/release.yml` whenever a `v*` tag is pushed:

| Artifact | Built by |
| --- | --- |
| Windows installer | `packaging/windows/gitcraft.iss` |
| AppImage | `packaging/linux/build-appimage.sh` |
| .deb / .rpm | `[package.metadata.deb]` / `[package.metadata.generate-rpm]` in `Cargo.toml` |
| macOS universal app, `.dmg` and `.zip` | `packaging/macos/build-app.sh` (job currently off) |

Every change to `dev` also publishes a prerelease, which is what the dev update channel follows.
Nothing is published until CI passed for that exact commit, and pull requests already build
the same packages with a prerelease version ([CI and releases](ci.md)).
