# PR 184 safety review validation

Tested commit `1e48ab83f98a8c34dc536408ee31915707365bcf`, tree `4683de7e10a326b6f2f6340f963952c99db8a45a`. [Full results](full-results.json) record sixteen successful gates from one complete run on the exact candidate. [Assembly](assembly.json) preserves five ordered feature commits descending from published `71ab301cf9bf976dab39956f1f22b66ab7d7f1c6`; each was committed and pushed before validation. The complete tested path/mode/blob map equals accepted source.

The fixes preserve the localized unborn-repository explanation, invalidate failed mutable file reads, preserve current File/History reads across unrelated status updates through captured input keys, refresh configuration-sensitive blame with an independent epoch, protect saved and recursive pulls before dispatch, serialize local metadata aliases by retained filesystem identity, and refuse excluded replacements of staged or unstaged removed paths. Case/Unicode aliases are checked on the repository's machine without trusting host case rules or `core.ignorecase`. Local scheduling lookup failure retains the documented path fallback, which cannot guarantee exclusion across unidentified aliases. Remote keys retain the alias-safe shared bucket. [Findings](findings.json) record review dispositions.

| Gate | Exit | Recorded summary |
|---|---:|---|
| fmt | 0 | exit 0 |
| check | 0 | exit 0 |
| clippy | 0 | exit 0 |
| build-tests | 0 | exit 0 |
| recording-outcomes | 0 | 3 passed, 0 failed, 0 ignored, 5005 filtered |
| recording-resolver | 0 | 1 passed, 0 failed, 0 ignored, 5007 filtered |
| runtime-gitcraft_os-lib | 0 | 24 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-lib | 0 | 4981 passed, 0 failed, 24 ignored, 3 filtered |
| runtime-checkpoint_cli-checkpoint_cli | 0 | 1 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-standalone_cli-standalone_cli | 0 | 3 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-main | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-mcp_stdio-mcp_stdio | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered |
| acl-save | 0 | 1 passed, 0 failed, 0 ignored, 5007 filtered |
| acl-index | 0 | 1 passed, 0 failed, 0 ignored, 5007 filtered |
| palette-isolated | 0 | 1 passed, 0 failed, 0 ignored, 5007 filtered |
| doc-tests | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered; 0 passed, 0 failed, 0 ignored, 0 filtered |

The three filtered ACL-save/index/palette cases each passed separately by exact name. Recording 3+1 passes overlap the library; both doctest suites contain zero tests. [Ignored tests](ignored-tests.json) enumerate all 24 names and reasons. Other platform-gated cases are outside this Linux inventory.

Recording gates, runtime suites and palette use PID/proc/dev containment. Two exact native ACL/ownership metadata fixtures operate on owned temporary files, including ordinary Git children; no process-control test runs natively. Real supplementary-group drift remains conditional; a passing summary does not establish that branch executed. No host-wide signal selectors were used. This does not establish native Windows/macOS, live SSH/WSL, picker or screen-reader behavior.

[Reviews](reviews.json) distinguish trusted source, digest-bound execution and tool-design approvals. Task IDs must be nonblank, reviewer provider/model identities must be populated, and source/execution tasks must differ. These records are trusted inputs rather than cryptographic reviewer authentication. Archived runner and source-checker bytes match completed execution. All tracked bytes, modes and symlink targets were independently compared with the immutable tree at 34 recorded boundaries; hidden/sparse flags and observation errors refuse validation. Boundary observations do not establish an immutable filesystem throughout execution.

[Manifest](manifest.json) binds every portable file plus compressed/raw log hashes. All gzip timestamps are zero. `gzip -dc logs/runtime-gitcraft-lib.log.gz` reproduces the library log. Absolute paths in original results identify the execution machine; the manifest maps portable logs. The NUL-delimited tested map is archived losslessly. Tooling regression tests exercise hidden source edits, malformed approvals, rewritten results with stale approval, and invalid assembly chains.

[Historical provenance errata](provenance-errata.md) qualify the earlier completion-stage identity as unknown and disclose prior clean-status source-attribution limits. Every historical executed byte and archive remains unchanged; no hidden historical edit or archive corruption is established. Earlier application gates do not validate this changed source.

A documentation-only successor inherits application gates only after the archived verifier establishes sole ancestry, complete tested-map equality outside this new evidence prefix, complete committed archive inventory, approved gate-log correspondence, pinned accepted-source and assembly provenance, file/log hashes, and exact committed execution/source/design approvals. Its external proof identifies both commits, both trees, reviews and evidence digests. No new runtime execution is claimed on that successor.
