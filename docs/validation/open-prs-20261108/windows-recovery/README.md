# Windows recovery path assertion

Tested and independently reviewed code commit: `acdd060448af9859fc47c1931fc34f054c16e6ef`.
Parent final candidate: `235a6b3d88a45931084b40623fd2f4ca8def2a64`.

[CI run 42540917025](https://github.com/JKamsker/gitcraft/actions/runs/42540917025)
passed Linux tests, lint, the Windows build and nine Windows shards. The Windows core
shard reported 1,729 passing tests and one failure in
`resolver::park::tests::dropping_to_a_branch_keeps_accessible_byte_exact_recovery`.
The aggregate Windows check consequently failed. The failed core harness stopped the
shard runner before its separately scheduled tests could execute.

The assertion compared displayed path strings. Production appends `parked` with
`Path::join`, while the expected path embeds `/parked` in a relative string.
On Windows, the resulting final separators differ even though both paths reach the same
archive. Existing raw-byte, index and metadata assertions passed before that assertion.

The test now extracts the advertised path from the branch description and compares its
canonical directory with the expected archive. This checks both accessibility and the
exact recovery destination, while accepting either Windows separator spelling. All other
recovery checks remain unchanged. No production code changed.

Local checks at the code commit:

- Test compilation, formatting and strict Clippy across all targets pass.
- The exact recovery test passes.
- The complete parking suite passes: 28 tests, zero failures.
- Runtime tests use Git 2.52, fresh temporary/config directories and separate PID namespaces
  (`bwrap --unshare-pid --die-with-parent --bind / / --dev /dev --proc /proc`).
- Independent adversarial review `/root/review_windows_recovery_assertion`: PASS on the exact
  code commit, no blocking findings; read-only static review, no tests or CI executed.

Windows execution of the revised assertion remains a final CI gate. These local tests ran
on Linux. The entire application suite was not rerun locally for this test-only change;
prior full-suite evidence remains in `../final/`, and previous CI results are archived here.
Production source matches `e2c6bb0f211144033b58621e18804f9d81dfc066`, excluding the two
reviewed test files `src/resolver/park/tests.rs` and `src/repo/detail/responsive_tests.rs`.
The subsequent documentation-only delivery commit binds these prior application gates
without claiming another execution.

`manifest.json` binds compressed and uncompressed hashes of the original CI log, prior CI
job summary, build output, targeted checks and runner to the tested code commit.
