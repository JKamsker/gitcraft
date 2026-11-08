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

## Tests

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
