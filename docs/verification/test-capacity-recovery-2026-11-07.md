# Local process containment evidence — 2026-11-07

Status: validated tiny owned-child smoke; no application build/test suite run.
Application baseline: `ab68bdedd387236c03f26f839572860d405a2f7f`.
This branch changes evidence only. It does not claim new application gates.

## Existing maintained subreaper

`/usr/local/bin/docker-init --version` reports Tini 0.19.0,
`git.de40ad0`. Its built-in help documents `-s` as registering a Linux child
subreaper, `-w` as warning on reaped processes, `-g` as child process-group
signal forwarding, `-p` as a parent-death signal, and `-e` as exit-code remapping.
The proposed command uses only `-s -w`; it requests no signal operations,
process-group forwarding, parent-death action, or exit-code remapping.
Inherited `TINI_KILL_PROCESS_GROUP`, `TINI_SUBREAPER`, and `TINI_VERBOSITY`
should be cleared so the command's behavior is explicit.

The kernel reparents orphan descendants to their nearest living subreaper.
Tini can wait for its children, including these adopted descendants. This
cannot adopt or reap the existing PID 1 zombies, and performs no host-wide
process selection or cleanup. The source of the pre-existing process ceiling
has not been established; cgroup and per-user counters alone do not prove it.

## Smoke and results

An inline ordinary Python fixture, supervised by existing Tini, created one
child and one grandchild. The intermediate child exited immediately and was
waited by its Python parent. The orphan grandchild waited 100 ms, printed its
PID and adopted parent PID, and exited 23. The main Python process remained
alive for 500 ms, then exited 37. No process was signaled. The outer diagnostic
waited for its own Tini child, read count-only zombie totals, and verified:

- Tini returned exact exit status 37.
- Main process parent and orphan adopted parent were both Tini PID 1638162.
- Tini logged reaping owned orphan PID 1638165.
- Visible zombie count stayed 31,300 before and after (delta zero).

The raw clean-run output is in
`test-capacity-smoke-2026-11-07.json`. An earlier smoke established the same
properties and exact status 37 but its outer assertion incorrectly expected
Tini to log the orphan's status 23. Tini logs the reaped PID, so the corrected
clean run checks that PID instead. No application test result is inferred.

## Bounded future commands

After the orchestration hold is released, the recommended form is:

```sh
source /workspace/.gitcraft-env/env.sh
export CARGO_TARGET_DIR=/workspace/target-OWNER-UNIQUE
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=1
export RUST_TEST_THREADS=2
export RAYON_NUM_THREADS=2
unset TINI_KILL_PROCESS_GROUP TINI_SUBREAPER TINI_VERBOSITY
exec /usr/local/bin/docker-init -s -w -- cargo <approved bounded command>
```

Each owner must supply a private target directory; do not reuse another
worktree's build artifacts. The source environment exports build jobs 4, so
apply the explicit override after sourcing it. These variables bound Cargo,
Rust's test harness, and Rayon respectively; they do not bound every possible
native library/compiler/helper thread. They do not restore consumed PID slots.
No full application build or full suite was performed here.

## Lifecycle limitation and release condition

Tini exits when its direct child exits. A descendant still alive at that point
can outlive Tini and subsequently be orphaned to PID 1. Thus this smoke proves
reaping while the supervised process is alive, not universal descendant
draining after Cargo/test exit. Fixture parents must wait for their children;
detached Git auto-maintenance should be disabled for local fixture commands
where the fixture does not intentionally test maintenance. Process-scoped Git
config overrides such as `gc.auto=0`, `maintenance.auto=false`,
`gc.autoDetach=false`, and `maintenance.autoDetach=false` may help, but preserve
existing caller config and fixture semantics. This evidence does not validate
all Git fixture paths.

Do not release arbitrary parallel fixtures solely from this smoke. Start with
one bounded command only after fixture-lifetime risks are addressed, and
compare count-only process/zombie totals. Do not signal PID 1, attempt foreign
reaping, select host PIDs for cleanup, or restart the workspace. Existing
zombies require an external environment owner to resolve if capacity remains
insufficient.

No CI was dispatched, re-run, waited upon, or otherwise used. This branch is
outside the repository's push triggers (`master` for CI, `dev`/`v*` for release).
