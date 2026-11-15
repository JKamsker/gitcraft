# Issue 189: Windows fixture Clippy correction

Source/test candidate: `d1198622e6d075e5414cdc06ac909252196cd7b9`.

CI run [43699480413](https://github.com/JKamsker/gitcraft/actions/runs/43699480413),
head d57666a6, failed in the completed Windows build job
[132767675934](https://github.com/JKamsker/gitcraft/actions/runs/43699480413/job/132767675934)
on clippy::permissions_set_readonly_false in materialize_tests.rs.
[Exact diagnostic excerpt](189-staged-windows-fixture/ci-failure.txt).
The overall run was still running, so --log-failed was unavailable; the completed
job log was read directly. No test-report artifact had been published at inspection.
The old run was not rerun or watched.

The only source change narrows the non-Unix branch to Windows and allows that lint
on its single set_readonly(false) call, documenting that Windows has a read-only
attribute rather than Unix permission bits. Unix retains explicit mode 0600.
No production code, test assertions, process logic, timeouts or budgets changed.
The supplied round-3 review remains complete; no further review was requested.

Local Clippy is 0.1.100 (0ef1bc3860 2026-11-09), matching CI. Strict all-target
Clippy for the touched gitcraft crate passed. No Windows suite was run locally;
the Windows-only branch was inspected and is left to this new head's CI.
A scan of the staged/scratch test code found no unwrap() or expect() calls.
The only set_readonly(false) call has the Windows-only statement allowance.

Validation on the source/test candidate:

- **65 passed, zero failed/ignored** in the complete git::testlane group, including
  the SHA-1/SHA-256 corruption regression: [output](189-staged-windows-fixture/tests.txt).
- Strict all-target Clippy 1.100 passed: [summary](189-staged-windows-fixture/clippy-summary.txt).
- cargo +stable fmt --check passed.
- 132 Python tooling tests passed.
- Committed-HEAD size (2,809 Rust files) and complexity: zero errors.
- Committed-HEAD architecture fitness: zero violations:
  [output](189-staged-windows-fixture/architecture.txt).
- Baseline growth checks against origin/dev and d57666a6 passed. No budgets or
  increase declarations changed.

The first affected run had 60 passes and five setup errors (missing files). A fresh
init probe established that the reused Git executable points at a removed installation
prefix, so its implicit template directory is absent. Git printed the missing-template
warning, and .git/hooks and .git/info did not exist. Setting GIT_TEMPLATE_DIR to the
stock /usr/share/git-core/templates produced both directories without warning.
The complete affected group then passed with that override and GIT_EXEC_PATH set
explicitly to the read-only tool installation. No repository code or assertion changed
between the failed and successful attempts.
The previous [861-test production evidence](189-staged-round3.md) remains historical:
source comparison with d57666a6 shows only the fixture source file changed.
The full git::testlane group is rerun for this correction; unchanged repo/UI groups
are not claimed as newly executed.

All Rust commands run at nice 19 with cargo +stable, target
/home/jonas/work/gitcraft-targets/189, TMPDIR below it and four build workers.
Git 2.49.1 is reused read-only from an existing validation tool installation.
Only this task-created target is removed at handoff.
The ci/check.py --dry-run plan was inspected; task resource limits replace its
all-package Clippy/compile-all defaults with strict touched-crate Clippy and
FAST affected-module tests. No full application, Windows or forge-server suite ran.

The final evidence commit changes documentation only. Bind these application/test
gates to that head only after verifying the diff from d1198622 contains docs paths
only. Size, complexity, baseline and architecture checks also run on that final
committed HEAD before publishing. Fresh fetch/plain merge found origin/dev
8e69afb7 current (including #240/#243 and d3a759bd).

Commands (the target/TMPDIR/PATH environment described above applies):

```sh
nice -n 19 cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings
GIT_TEMPLATE_DIR=/usr/share/git-core/templates \
GIT_EXEC_PATH=/home/jonas/work/gitcraft-evidence/233/local-git/libexec/git-core \
  nice -n 19 cargo +stable test --locked --package gitcraft --lib -- git::testlane --test-threads=2
nice -n 19 cargo +stable fmt --check
python3 -m unittest discover -s ci -p 'test_*.py'
python3 ci/rust_file_size.py --revision HEAD
python3 ci/complexity_policy.py --revision HEAD
python3 ci/arch_baseline.py --base origin/dev --revision HEAD
python3 ci/arch_baseline.py --base d57666a6 --revision HEAD
nice -n 19 cargo +stable run --locked --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD
```
