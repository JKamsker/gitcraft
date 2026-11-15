# Issue 189: round-2 fixes and historical evidence

Superseded by the [extra inline-review evidence](189-staged-inline-review.md).

Application/test candidate: `b560095f8c5e7930dfa991954c80656baf7fcfce`.
Base `origin/dev` at `8e69afb7df4141eada62495793a9645d181abe0e` was merged
normally, including #240, #243 and `d3a759bd`. Subsequent evidence commits change
only documentation. Bind these gates to a later head only after checking that
`git diff --name-only b560095f8c5e7930dfa991954c80656baf7fcfce HEAD` contains documentation paths only.

The supplied independent read-only round-2 review of `040dedb7` is complete and
replaces the previously requested re-review. It verified the round-1 hook,
symlink, sparse, overall-deadline and layout fixes. Its five remaining findings
are addressed below. Direct raw-blob materialization replaces checkout, so this
candidate is returned for another independent review before CI and merge.
The [round-1 record](189-staged-review-fixes.md) remains historical evidence.

## Fixes and regressions

1. **Filter refusal race:** preparation uses the existing bounded inert-object
   command API and writes blob bytes directly into the registered scratch worktree.
   Neither checkout nor diff-files runs. No live attributes, filters, LFS hydration,
   CRLF conversion or working-tree-encoding conversion can affect materialization.
   Preflight still refuses effective filter declarations with the existing actionable
   LFS message. A deterministic real-Git regression changes info attributes, process
   and smudge drivers, required=true, autocrlf and encoding after preflight; original
   blob bytes are written and neither driver marker appears.
2. **Read limits/deadline:** index and info attributes are opened once, checked
   through the handle, and read with Take(limit+1), bounded chunks and cancellation/
   deadline checks. Unix opens are nonblocking/no-follow; Windows opens reparse points
   themselves and rejects them through handle metadata. The exact bounded bytes
   captured from the source are written to the private index. Tests cover endless
   readers/growth, exact limits, oversized metadata, pinned Unix inodes across rename,
   directories, FIFO and symlink refusal, expired deadlines and cancellation between
   reads. Windows tests release handles before portable mutations.
3. **Storage expansion:** raw indexed bytes cannot expand through checkout conversion.
   A real staged .gitattributes encoding/CRLF regression preserves the indexed LF bytes.
   The existing 512 MiB/10,000-entry preflight counts indexed sizes, block rounding and
   namespace overhead. Index/info files each remain capped at 64 MiB. Build outputs
   have no filesystem quota; the popup (en-US/de) and guide state the raw-byte policy
   and require commands to bound their output files. The overall 20-minute deadline
   and separately bounded cleanup remain in force.
4. **Changing configuration dependencies:** config/HEAD events rebuild watchers off
   the UI thread. Dependencies include inactive includeIf paths, nested includes and
   absent targets; their closest existing parent directories are watched. Setup checks
   dependencies again after registration to cover changes during setup. A generation
   guard and verifying indication cover that interval before a new fingerprint can
   mark a verdict fresh. Real-Git dependency tests and repo state tests passed.
5. **Protected Git-directory aliases:** one host-independent predicate rejects NTFS
   GIT~1/stream forms and Git's HFS ignorable Unicode aliases, irrespective of Git's
   protect settings. Entries and every resolved symlink component use it, including
   a protected component followed by '..'. Pure tests cover both entry and link paths.

The source index/ref and forgotten-helper acceptance tests, unresolved/deleted/renamed/
mode tests, owned-child cancellation/cleanup tests, and original composer resize test
all passed unchanged. Symlinks are installed after regular files; Windows link kind
is derived from resolved staged targets, with no open-file rename requirement.

## Gates

Linux; Rust/Cargo stable 1.100.0; Git 2.50.1 built inside the disposable task target.
Every Rust command ran at nice 19, serially, with
`CARGO_TARGET_DIR=/home/jonas/work/gitcraft-targets/189` and TMPDIR under it.
The target is removed at handoff. No system tools changed.

- **689 passed, 0 failed, 1 pre-existing ignored** in the wider affected-module run:
  [output](189-staged-round2/tests.txt). The existing ignored native HTTPS gate retains
  its own explicit containment prerequisites; no test was skipped or weakened here.
- **10 synthetic/preflight tests passed first**, before the wider owned-child run:
  [output](189-staged-round2/synthetic-tests.txt).
- Strict touched-crate all-target Clippy, including tests and deny panic/expect/unwrap:
  [output](189-staged-round2/clippy.txt).
- `nice -n 19 cargo +stable fmt --check` passed.
- **132 Python CI tooling tests passed**: [output](189-staged-round2/tooling.txt).
- Committed-HEAD file-size: **2,804 Rust files, zero errors**. Complexity policy: zero errors.
- Architecture fitness `check --revision HEAD`: **zero violations**:
  [output](189-staged-round2/architecture.txt).
- Baseline checks against origin/dev and prior reviewed `040dedb7` passed. This fix set
  adds no budget growth/declarations; the normally merged #243 declarations are preserved.
- `python3 ci/check.py --dry-run` confirmed the CI plan. The task's resource limits
  override its all-package Clippy/compile-all defaults: touched-crate gates and the
  following affected groups ran manually. No full application, Windows or forge-server
  suite ran. No CI dispatch, rerun or wait was performed for this review candidate.

Exact wider selection (repo::direct_tests is a directory; its real owner modules are
also selected):

```sh
nice -n 19 cargo +stable test --locked --package gitcraft --lib -- \
  processes::deadline::tests git::testlane repo::test_lane repo::layout \
  repo::direct_tests repo::detail repo::load repo::tests \
  repo::watch repo::lifecycle repo::keys repo::keyboard \
  repo::toolbar repo::palette repo::commit_changes repo::graph_view \
  repo::apply a11y::tests::repo_view app::palette git::command \
  git::rehearse git::interaction git::stream git::bundles \
  processes repo::branch_cleanup_tests repo::branch_health repo::branch_tree \
  repo::celebration repo::columns repo::commands repo::conflict_check \
  repo::file_text repo::forge_selection_tests repo::graph_walk repo::growth \
  repo::messages repo::navigate repo::push_preflight repo::quick_edit \
  repo::rebase repo::rewrite repo::symbol_biography repo::publish \
  --test-threads=2
```

Synthetic selection:

```sh
nice -n 19 cargo +stable test --locked --package gitcraft --lib -- \
  git::testlane::staged::bounded git::testlane::staged::preflight::names \
  git::testlane::staged::preflight::tests git::testlane::staged::preflight::links::tests \
  processes::deadline::tests --test-threads=2
```

Strict gates:

```sh
nice -n 19 cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings
nice -n 19 cargo +stable fmt --check
nice -n 19 cargo +stable run --locked --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD
python3 ci/rust_file_size.py --revision HEAD
python3 ci/complexity_policy.py --revision HEAD
python3 ci/arch_baseline.py --base origin/dev --revision HEAD
python3 ci/arch_baseline.py --base 040dedb7 --revision HEAD
```
