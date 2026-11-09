# Windows rehearsal test scheduling

Reviewed and locally tested code commit: `82c2d590613b8f02a250b8f9023a128c957d0b47`.
Parent candidate: `625edb0c6b9bfc1e2189ecaf63cc6da52418edda`.

[CI run 42543080508](https://github.com/JKamsker/gitcraft/actions/runs/42543080508)
passed Linux, lint, Windows build, core (including the repaired archive-path assertion)
and eight other Windows shards. The repo-q-r shard timed out at 20 minutes.
Inventory comparison and its over-60-second message identify the sole unfinished Windows
test as `repo::rehearse::tests::applying_does_it_for_real_and_it_can_be_undone`.
The same code passed this shard in the previous CI run. Existing bounded settle/review
loops and the absence of process-wait diagnostics do not identify the blocking operation.
No production deadlock or fixture path collision was established.

The Windows scheduler now executes this exact test in its own libtest process, alongside
the existing isolated reflog fixture. Main-shard skips derive from the complete isolated
list; partition verification still requires every discovered test exactly once. The
rehearsal test retains its confirmation, branch, tree and undo-journal assertions.
Gated progress messages identify setup, rehearsal, confirmation, apply, verification and
explicit teardown phases if another hang occurs. Production code is unchanged.

This is a scheduling mitigation, not a proven root-cause fix. Windows CI remains required.
Local Linux validation is recorded in the archived results and logs: partition coverage,
three exact rehearsal repetitions, the revised repo-q-r schedule, test compilation,
formatting and strict Clippy across all targets. The library partition covers 4,915
discovered tests exactly once. The revised repo-q-r schedule selected 199 tests: 193
executed successfully and six remained ignored (191 main-process passes plus two isolated
passes). Each runtime invocation used a fresh
scratch/config directory and a separate PID namespace. The trace environment was enabled.

Independent adversarial review `/root/review_windows_rehearsal_isolation`: PASS on the exact
code commit, no blocking findings; static review only, no tests or CI executed.

The full application suite was not rerun locally for this test/CI-only change. Earlier
full-suite and CI evidence remain bound to their original commits. Production source is
identical to application candidate `e2c6bb0f211144033b58621e18804f9d81dfc066`, excluding
three reviewed test files: `src/repo/detail/responsive_tests.rs`,
`src/resolver/park/tests.rs`, and `src/repo/rehearse/tests.rs`. A documentation-only delivery
commit binds these gates without claiming new execution. The manifest records hashes of
compressed and uncompressed evidence, including the original timeout log and runner.
