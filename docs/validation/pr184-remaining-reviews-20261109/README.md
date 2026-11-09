# PR 184 review follow-up validation

Tested commit `96584b3a69234822ca9f8c299ec13e32929ac2a0`, tree `cad977982c89d3662c7ea9ec05b49b38e105f539`. [Full results](full-results.json) record sixteen successful gates on the exact clean candidate. [Assembly](assembly.json) preserves four feature commits descending from `a437a10a`, with the complete accepted source map unchanged. Each unit was pushed before validation.

The remaining review fixes add localized suspicious-rule advisories without rewriting imported rules, refuse stale Files stash selections before dispatch rather than silently adding untracked scope, keep successful blame visible during replacement reads, and skip line-history reloads when the resolved commit, historical path and mapped range are unchanged. Line mapping pins one commit throughout rename discovery, diff and log; failed reads retry. [Findings and dispositions](findings.json) bind each unresolved review to its implemented fix or retained safety constraint. The shared remote bucket is retained: literal per-host keys would reopen alias races. Historical executed tools and prior evidence remain unchanged; successor tools use exclusive temporary files and committed-archive integrity checks.

| Gate | Exit | Recorded summary |
|---|---:|---|
| fmt | 0 | exit 0 |
| check | 0 | exit 0 |
| clippy | 0 | exit 0 |
| build-tests | 0 | exit 0 |
| recording-outcomes | 0 | 3 passed, 0 failed, 0 ignored, 4989 filtered |
| recording-resolver | 0 | 1 passed, 0 failed, 0 ignored, 4991 filtered |
| runtime-gitcraft_os-lib | 0 | 24 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-lib | 0 | 4965 passed, 0 failed, 24 ignored, 3 filtered |
| runtime-standalone_cli-standalone_cli | 0 | 3 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-checkpoint_cli-checkpoint_cli | 0 | 1 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-main | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-mcp_stdio-mcp_stdio | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered |
| acl-save | 0 | 1 passed, 0 failed, 0 ignored, 4991 filtered |
| acl-index | 0 | 1 passed, 0 failed, 0 ignored, 4991 filtered |
| palette-isolated | 0 | 1 passed, 0 failed, 0 ignored, 4991 filtered |
| doc-tests | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered; 0 passed, 0 failed, 0 ignored, 0 filtered |

The three filtered ACL-save/index/palette cases each passed separately by exact name. Recording 3+1 tests overlap library coverage. Both doctest suites contain zero tests. [Ignored tests](ignored-tests.json) enumerate all 24 names and reasons; platform-gated cases are outside that Linux inventory.

Recording gates, runtime suites and palette use PID/proc/dev isolation. The two native ACL metadata fixtures operate on owned temporary files, including ordinary Git children; no process-control tests run natively. Real supplementary-group drift is conditional, and a passing summary does not establish that branch executed. No host-wide signal selectors were used. Remote and UI tests do not establish live SSH/WSL, native picker, or real screen-reader behavior.

[Reviews](reviews.json) distinguish source, execution, and verifier-design approvals. The source and digest-bound execution approvals are independent and name this exact tested commit/tree. Reviewers inspected parent-produced evidence rather than rerunning the suite unless explicitly recorded otherwise.

[Manifest](manifest.json) binds every archive file and each raw/compressed log pair. Gzip timestamps are zero; for example `gzip -dc logs/runtime-gitcraft-lib.log.gz` reproduces the library log. Absolute paths in original results identify the execution machine; the manifest maps portable archived logs. The NUL-delimited tested path/mode/blob map is archived losslessly.

Earlier validation remains historical and does not validate these changed application files. An initial helper-export compile failure remains historical; no passing execution is attributed to that attempt. Earlier review findings on error caching and commit pinning were corrected before this combined run. No earlier failed or partial gate supplies this full run.

A documentation-only successor inherits these gates only after the archived verifier proves sole ancestry, complete application path/mode/blob equality outside this newly added evidence directory, and all committed archive/file/log hashes. Its external proof names both commits, both trees and digests; no new runtime execution is claimed on that successor.
