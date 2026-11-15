# Issue 192 pre-merge local validation

Application source: `3ea071d2614e7555fa5561a3e10a6d6a6b37e1bb` (immutable). All listed scoped gates and test
commands passed; logs and the command manifest are retained here. The initially guessed
`repo::layout::direct_tests` filter matched zero tests; the actual `repo::layout` (4 tests)
and `repo::tests` (33 tests) groups were then run and passed. The tooling suite passed
132 tests. Source identity is unchanged by this evidence-only commit.

The pre-push wrapper plan is retained. To respect the task's resource restriction,
its gates were executed individually with strict Clippy limited to the application
package/all targets, rather than executing the wrapper's all-package inventory.
No full suite, Windows suite, CI dispatch or CI wait ran.

Host Git 2.43 lacks the existing offline `--no-lazy-fetch` guard; the initial failed
resource-test log is retained as `tests.log.gz`. Git 2.49 was built locally under the
owned target directory, without HTTP support (local-only test fixtures), and supplied
on PATH for passing tests. No host installation or application guard was changed.

V1 is observation-only: no new cleanup execution, archive, purge, process control or
hibernation. Sizing is supported on Linux with descriptor/openat2 protections;
other platforms and SSH/WSL explicitly report limitations. Cargo is the only build
candidate allow-list. External process/port association remains unknown and blocks
all cleanup plans; local remote-tracking refs carry unknown remote freshness.
Preview observations expire after 30 seconds and retire on known repository reloads;
there is no continuous watcher for every linked worktree. Later execution must inspect
again. Retirement to recovery reclaims zero now; potential later purge size is separate.
