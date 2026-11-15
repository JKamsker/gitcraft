# Issue 189: round-3 simplification and ownership fixes

Application/test candidate: `af792d0f71c7b7eaeb2bfe23bfe14276a6666d00`.
The supplied independent read-only review of `1a33df2b` is complete and replaces
its pending re-review. It verified raw materialization, exclusive file creation,
modes/names/storage/deadline checks, submodule/sparse refusal, stale logic, layout,
file sizes and architecture declarations. The task explicitly requests these three
fixes without another independent review, followed by current-head CI and merge.

## Changes

1. V1 refuses every index mode 120000 with an explicit en-US/de message asking the
   user to test symlinks in their working tree. The entire target/chain/alias
   resolver and its traversal budget are removed. A host-independent pure test and
   real-Git contained/escaping/cyclic link cases verify refusal before execution.
   Follow-up: only add staged symlink support once filesystem alias containment can
   be guaranteed; no symlinks are materialized in this version.
2. The existing shared worktree backend remains for test-lane, rehearsal, interaction,
   resolver and review callers. Each exclusively allocated wrapper contains an
   ownership marker, with a matching create-new registry record in the scratch parent.
   Both record a random token, allocation ID, creation time, machine, PID and available
   boot ID. The checkout is a child directory, keeping metadata out of its file tree.
   Existing token generation and bounded safe-read helpers are reused. Every removal
   revalidates the records; live cleanup may force only its verified allocation.
   Crash sweeps require matching records and dead-owner evidence, use plain Git removal,
   and preserve modified/locked worktrees with .preserved. Legacy/unmarked/mismatched
   paths stay untouched. Unix probes only the recorded PID with signal zero; Windows
   uses a query-only handle, closes it, and treats unknown errors conservatively.
   There are no host-wide process selectors/signals or double-force removals.
3. After bounded cat-file, hash each exact raw blob using Git hash-object --no-filters
   --stdin without -w. Git includes the blob length/header and selects SHA-1/SHA-256
   from the repository. Compare the result with the manifest OID before creating the
   output file. A real loose-ODB test substitutes a valid same-length zlib blob under
   the wrong OID in each object format and verifies localized refusal and no output.
   Its test-owned loose object is made writable before corruption; no open-handle
   renames or sleeps are used.

## Validation

- **861 passed, zero failures, four existing ignored**, 237.85 seconds in the wider
  affected selection: [output](189-staged-round3/tests.txt). No test was newly ignored
  or weakened. The original composer resize test passed unchanged.
- **15 synthetic/preflight checks passed first**, before real owned-worktree/process
  tests, and repeated inside the wider run: [output](189-staged-round3/synthetic-tests.txt).
- **132 Python tooling tests passed**: [output](189-staged-round3/tooling.txt).
- Strict touched-crate all-target Clippy passed: [output](189-staged-round3/clippy.txt).
- cargo +stable fmt --check passed.
- Committed-HEAD size: 2,809 Rust files, zero errors (all <= 500 lines).
- Committed-HEAD complexity policy: zero errors.
- Committed-HEAD architecture fitness: zero violations:
  [output](189-staged-round3/architecture.txt).
- Baseline checks against origin/dev and previous candidate 1a33df2b passed.
  No architecture budget/increase declarations changed; merged declarations remain intact.

Existing ignored tests: the native HTTPS containment gate, resolver picture inspection,
real signed-in agent end-to-end exercise, and the existing R2 runtime GC hold. None
was opted into; no full application, Windows or forge-server suite ran locally.

The fresh build uses stable Rust/Cargo 1.100.0, a task-local Git 2.50.1, nice 19,
CARGO_TARGET_DIR=/home/jonas/work/gitcraft-targets/189 and TMPDIR below it. Rust gates
run in the foreground. That target is removed after delivery. No system tools change.
Base origin/dev remains `8e69afb7df4141eada62495793a9645d181abe0e`, including #240/#243
and d3a759bd; fetch/plain merge was already current after the wider run.
No implementation-stage CI was dispatched, rerun or watched. Only the final published
head's CI may be checked or rerun. CI/merge evidence is recorded in the PR and #189.

Wider command (direct_tests is a directory; its owner modules are selected explicitly):

```sh
nice -n 19 cargo +stable test --locked --package gitcraft --lib -- \
  processes::deadline::tests git::testlane repo::test_lane repo::layout \
  repo::direct_tests repo::detail repo::load repo::tests \
  repo::watch repo::lifecycle repo::keys repo::keyboard repo::toolbar repo::palette \
  repo::commit_changes repo::graph_view repo::apply a11y::tests::repo_view app::palette \
  git::command git::rehearse git::interaction git::stream git::bundles processes \
  repo::branch_cleanup_tests repo::branch_health repo::branch_tree repo::celebration \
  repo::columns repo::commands repo::conflict_check repo::file_text \
  repo::forge_selection_tests repo::graph_walk repo::growth repo::messages \
  repo::navigate repo::push_preflight repo::quick_edit repo::rebase repo::rewrite \
  repo::symbol_biography repo::publish resolver repo::detached repo::checkout_blocked \
  repo::review_workflow::contracts repo::review_workflow::choreography \
  repo::review_workflow::radar repo::worktree_tests git::worktree i18n::tests \
  --test-threads=2
```

The ci/check.py --dry-run plan was inspected. Task resource limits override its
all-package Clippy and compile-all defaults: only strict touched-crate Clippy, fmt,
FAST affected tests, Python tooling and committed-HEAD architecture/static checks run.

Subsequent publication/evidence commits change documentation only. Before binding these
application gates to a later head, verify:

```sh
git diff --name-only af792d0f71c7b7eaeb2bfe23bfe14276a6666d00 HEAD
```

Only documentation paths may differ. Final committed-HEAD architecture, size,
complexity and baseline checks also run after the documentation commit, before push.

Strict/static commands:

```sh
nice -n 19 cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings
nice -n 19 cargo +stable fmt --check
nice -n 19 cargo +stable run --locked --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD
python3 ci/rust_file_size.py --revision HEAD
python3 ci/complexity_policy.py --revision HEAD
python3 ci/arch_baseline.py --base origin/dev --revision HEAD
python3 ci/arch_baseline.py --base 1a33df2b --revision HEAD
```
