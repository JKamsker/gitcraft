# PR 184 safety review validation

Tested commit `dcd0b82ed672b1fffb485f235f6864d378143c04`, tree `b7c87b81c796730f0671a7f274183a05cb17f946`. [Full results](full-results.json) record sixteen successful gates from one complete run on the exact candidate. [Assembly](assembly.json) preserves five ordered feature commits descending from published `c36d436e88508ebfbbbd85818b523ae44f670e3a`; each was committed and pushed before validation. The complete tested path/mode/blob map equals accepted source.

The fixes accept host-wide author-rule suffixes, serialize all keyed local mutations through a pure Local bucket, guard native pull autostash cleanup, refuse prefixed/TRY stash envelopes, preserve literal selected-submodule scope, coalesce status probes, retry queue BUSY through UI polling, and reconcile pending file/history/blame reads against captured input keys. Clone keyboard regressions exercise production widgets without invoking pickers or clones. [Findings](findings.json) retain every reported review and its disposition.

One global Local mutation bucket deliberately trades independent-repository concurrency for exclusion across aliases, lookup failures and pathname retargeting. Remote mutations retain their separate shared Remote bucket; unkeyed reads remain runnable when worker capacity permits. Scheduling performs no filesystem identity lookup. This provides app-internal exclusion, not immutable repository targeting or coordination with external Git, other app instances, or local/remote paths exposing the same storage. Background reads separately cache fetch ownership.

Native pull cleanup checks conservatively refuse excluded replacements even when native autostash would be disabled. Canonical sidebar submodule updates inspect literal selected gitlinks and descendants; noncanonical option envelopes receive a whole-repository populated-submodule scan rather than guessed scope. Attribution keys include effective Git attributes, symlink-followed configured dependency files through the repository transport, and resolved mailmap blobs. External textconv program state still requires explicit refresh.

Assembly pins each unit's immutable source, including a scheduler-only intermediate snapshot where shared files precede the file-view feature. The final complete tree equals the accepted combined source. Digest-bound independent assembly review covers that exact ordered history.

| Gate | Exit | Recorded summary |
|---|---:|---|
| fmt | 0 | exit 0 |
| check | 0 | exit 0 |
| clippy | 0 | exit 0 |
| build-tests | 0 | exit 0 |
| recording-outcomes | 0 | 3 passed, 0 failed, 0 ignored, 5021 filtered |
| recording-resolver | 0 | 1 passed, 0 failed, 0 ignored, 5023 filtered |
| runtime-gitcraft_os-lib | 0 | 24 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-lib | 0 | 4997 passed, 0 failed, 24 ignored, 3 filtered |
| runtime-checkpoint_cli-checkpoint_cli | 0 | 1 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-standalone_cli-standalone_cli | 0 | 3 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-gitcraft-main | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered |
| runtime-mcp_stdio-mcp_stdio | 0 | 6 passed, 0 failed, 0 ignored, 0 filtered |
| acl-save | 0 | 1 passed, 0 failed, 0 ignored, 5023 filtered |
| acl-index | 0 | 1 passed, 0 failed, 0 ignored, 5023 filtered |
| palette-isolated | 0 | 1 passed, 0 failed, 0 ignored, 5023 filtered |
| doc-tests | 0 | 0 passed, 0 failed, 0 ignored, 0 filtered; 0 passed, 0 failed, 0 ignored, 0 filtered |

The three filtered ACL-save/index/palette cases each passed separately by exact name. Recording 3+1 passes overlap the library; both doctest suites contain zero tests. [Ignored tests](ignored-tests.json) enumerate all 24 names and reasons. Other platform-gated cases are outside this Linux inventory.

Recording gates, runtime suites and palette use PID/proc/dev containment. Two exact native ACL/ownership metadata fixtures operate on owned temporary files, including ordinary Git children; no process-control test runs natively. Real supplementary-group drift remains conditional; a passing summary does not establish that branch executed. No host-wide signal selectors were used. This does not establish native Windows/macOS, live SSH/WSL, picker or screen-reader behavior.

[Reviews](reviews.json) distinguish trusted source, digest-bound execution and tool-design approvals. Task IDs must be nonblank, reviewer provider/model identities must be populated, and source/execution tasks must differ. These records are trusted inputs rather than cryptographic reviewer authentication. Archived runner and source-checker bytes match completed execution. All tracked bytes, modes and symlink targets were independently compared with the immutable tree at 34 recorded boundaries; hidden/sparse flags and observation errors refuse validation. Boundary observations do not establish an immutable filesystem throughout execution.

[Manifest](manifest.json) binds every portable file plus compressed/raw log hashes. All gzip timestamps are zero. `gzip -dc logs/runtime-gitcraft-lib.log.gz` reproduces the library log. Absolute paths in original results identify the execution machine; the manifest maps portable logs. The NUL-delimited tested map is archived losslessly. Tooling regression tests exercise hidden source edits, malformed approvals, rewritten results with stale approval, and invalid assembly chains.

[Historical provenance errata](provenance-errata.md) qualify the earlier completion-stage identity as unknown and disclose prior clean-status source-attribution limits. Every historical executed byte and archive remains unchanged; no hidden historical edit or archive corruption is established. Earlier application gates do not validate this changed source.

A documentation-only successor inherits application gates only after the archived verifier establishes sole ancestry, complete tested-map equality outside this new evidence prefix, complete committed archive inventory, approved gate-log correspondence, pinned accepted-source and assembly provenance, file/log hashes, and exact committed execution/source/design approvals. Its external proof identifies both commits, both trees, reviews and evidence digests. No new runtime execution is claimed on that successor.
