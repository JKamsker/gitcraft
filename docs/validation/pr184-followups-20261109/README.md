# PR 184 follow-up validation

Tested commit: `1eee7e703ce629441eee166b2718021394c9fe09`. Tree: `e52b17e34a9001bbe204447e459e5446d40d80e7`. Accepted source: `7346090eb38f4332774a06473a02d60262585f32`.

All sixteen gates in [full-results.json](full-results.json) passed on the exact clean candidate. Twelve successful gates came from [v2](runners/full-run-v2.py); [completion](runners/complete-gates.py) verified those records and executable hashes, then ran the remaining four gates. The initial contained ACL-save attempt failed because the namespace mapped only one UID while the fixture names UID+1; its results and failure log are preserved separately. The two ACL metadata fixtures therefore ran natively on owned temporary files. The library and other executable suites ran once and were not repeated. Six test harnesses and the separate application executable were identified from Cargo JSON and hashed before/after runtime gates. Every gate records clean before/after states, raw-log hashes and exact commands; final cleanliness is enforced. No integration-only supplement was executed for this candidate; its separate application executable was hash-checked throughout both stages.

| Gate | Exit | Recorded summary |
|---|---:|---|
| fmt | 0 | exit 0 |
| check | 0 | exit 0 |
| clippy | 0 | exit 0 |
| build-tests | 0 | exit 0 |
| recording-outcomes | 0 | 3 passed, 0 failed, 0 ignored, 4978 filtered |
| recording-resolver | 0 | 1 passed, 0 failed, 0 ignored, 4980 filtered |
| runtime-gitcraft_os-lib | 0 | 24 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-lib | 0 | 4954 passed, 0 failed, 24 ignored, 3 filtered |
| runtime-checkpoint_cli-checkpoint_cli | 0 | 1 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-main | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-standalone_cli-standalone_cli | 0 | 3 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-mcp_stdio-mcp_stdio | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered |
| acl-save | 0 | 1 passed, 0 failed, 0 ignored, 4980 filtered |
| acl-index | 0 | 1 passed, 0 failed, 0 ignored, 4980 filtered |
| palette-isolated | 0 | 1 passed, 0 failed, 0 ignored, 4980 filtered |
| doc-tests | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered; 0 passed, 0 failed, 0 ignored, 0 filtered |

The main harness legitimately has zero tests. Recording tests overlap library coverage and must not be counted as distinct additional tests. The three separately executed ACL/index/palette cases are filtered out of the parallel library gate and each required to pass by exact name with exactly one executed test. [ignored-tests.json](ignored-tests.json) enumerates all 24 ignored tests and their recorded reasons; they are separate from those three filtered cases. Documentation-test summaries are preserved in the raw log; no nonzero documentation-test count is claimed.

The compiled runtime suites, recording gates and isolated palette case ran inside a separate PID namespace with private proc/dev mounts. ACL metadata fixtures ran natively because their named UID cannot be represented in that namespace; they do not perform process-control tests. Recording-only safety gates passed before the process test suites. No host-wide signal selection was used. The ACL test's real supplementary-group branch is conditional; a passing summary alone does not establish that branch ran. Execution identity and groups are recorded.

[reviews.json](reviews.json) identifies each reviewer, exact commit/tree, scope and verdict. Distinct independent source and execution-evidence reviews passed for the tested candidate. Reviewers inspected source and parent-produced records; they did not rerun tests unless their record expressly says so.

[assembly.json](assembly.json) binds eight follow-ups descending from published `5988524c23e43d8862225e44b09fbc104a735cc6`: seven feature commits and historical scheduler evidence. Every author matches the required identity. The complete accepted and tested path/mode/blob maps are equal. [The binary map](tested-path-mode-blob-map.bin.gz) decompresses to the exact NUL-delimited bytes whose SHA-256 is recorded; [the readable map](tested-path-mode-blob-map.txt) has its own file hash.

The prior `049a29c0` run is archived separately under `historical/` and `logs/failed049-*`. It failed with 4,952 passed, one failed, 24 ignored and three filtered, stopping at the library gate; later gates did not run. Its sole failure expected unfocused Enter to submit Fold. Accepted source `7346090e` corrected that production-dispatcher test to require explicit primary activation; this candidate's complete run passed. None of the prior failed run's gates is accepted as validation of the changed application.

Historical remote-scheduler archives remain bound to implementation `28e6a8b1`; current `tasks.rs` adds a documented test-module lint allowance. Older original full-suite evidence under `adversarial-review-20261109` remains historical. This package supplies validation for the current changed application.

Limits: scheduling exclusion is per pool/process; remote tests are synthetic; inert egui tests do not prove live SSH/WSL, native-picker or screen-reader behavior. Conservative blame/line-history refresh remains; display continuity and narrower invalidation are deferred UX/performance work.

[manifest.json](manifest.json) records raw and zero-mtime gzip hashes for all logs, hashes all archived files, and binds the tested source map and application binary. Original absolute paths identify the execution machine; use the manifest's log mapping for portable archived files. For example, `gzip -dc logs/full-runtime-gitcraft-lib.log.gz` reproduces the library log.

This evidence-only successor inherits the tested application gates only after a post-commit comparison proves every path/mode/blob outside `docs/validation/pr184-followups-20261109/` is unchanged. The external inheritance verification record names both commit SHAs, both tree IDs and map digests, and requires only additions under the stated prefix; no new execution on that documentation successor is claimed.
