# Issue #51 fileviews: implementation corrections after independent R1 failure

Corrected application source: `10727c7e88725b4510350ec3416bd1e4ebfcfeda`. Branch: `t3/diff-staging-views`, write remote `JKamsker/gitcraft`. The source and every corrective unit were committed, pushed and checked against the remote SHA with author Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>. The checkout remains based on `a850de26`; parent owns integration with advanced origin/dev and final history assembly.

Status: **implementation corrected and locally validated; parent independent R2 review and combined validation pending**. This is an implementation handoff, not independent approval. Candidate `c82112a5` / application `5df74dbb` failed the three independent R1 reports. Their positive tests did not cover or clear these eleven findings. The earlier validation record is historical evidence only.

## All finding dispositions

| R1 finding | Correction and production source | New regression evidence |
| --- | --- | --- |
| Content 1, P1: untracked SOPS textconv | `8d87f59c7ea58129b1216be52f6d9964c402986f`: [work_diff](../../src/git/diff.rs) applies DiffOptions, including no_textconv, to the --no-index path. Existing SOPS recognition happens before generation. | Expanded `git_textconv_cannot_decrypt_ordinary_patches_or_run_before_opt_in` loads a recognizable untracked envelope through actual diff loading and paints its UI: converter marker absent, private marker absent from normal DiffData, Show decrypted retained. |
| Content 2, P1: initial/selected line history conversion | Same commit: [line_log](../../src/git/line_log.rs) suppresses textconv for the initial log-L query, work/index line mapping and selected commit patch. [load_lines](../../src/repo/diff/mod.rs) recognizes selected SOPS; explicit private decryption is labeled as complete-file comparison. | Same converter regression exercises initial revision history, worktree-to-HEAD mapping and a selected commit through load_lines; all retain ciphertext. Ten line-log parser/range tests also pass. |
| Content 3, P1: SVG local resources | `10447d86895f1068a9222ef2f148a385564cc6bd`, tightened by `87cdd2b5d5a146a8f9641688e1a2f82db2691feb`: [isolated SVG loader](../../src/svg.rs) runs ahead of defaults in [app initialization](../../src/app/mod.rs). External string resolvers return None; nested inline SVG is refused. XML/input/depth/node/effect/decoded-image/overdraw/output budgets precede rendering in shared cancellable background workers. | Existing external absolute and file URLs plus a real resolvable relative outside image remain transparent. Installed-loader test proves the default resolver is bypassed and the first call is Pending. Safe inline raster, oversized compressed raster, source/expansion limits and complex overdraw thumbnail tests pass. Rendered/source/quoted-rename regressions pass too. |
| Content 4, P2: unavailable working source looks empty | `8d87f59c`: [encrypted reader](../../src/repo/diff/encrypted/reader.rs) rejects Version.warning and unsupported modes before taking bytes; only actual absence is an empty side. Probe failure disables conversion. | `unreadable_oversize_and_nonregular_sources_fail_closed_before_textconv` tests small encrypted and ordinary indexed versions against oversized SOPS working files, explicit read failure, failed probe, no converter execution, actual absence and a nonregular source. |
| UI 1, P2: stale matrix pair | `6c4df1b1214c86c4b0d6484096584232d2f3ddbc`, completed by `10727c7e88725b4510350ec3416bd1e4ebfcfeda`: [State/comparison_cell](../../src/repo/file_comparison/mod.rs) revoke pair generations and clear pair/loading on every cell transition. data_request remains stable for displayed rows during pair requests; clearing rows revokes it so queued old cells cannot cancel a matrix refresh. | Actual cell test operates 2→1, 1→0 and 2→new-1; old completed/pending results are explicitly delivered and refused. Pair/spinner disappear. A queued cell during refresh leaves the new request and loading intact. |
| UI 2, P2: repeated moved-block LCS on paint | `6c4df1b`: [code_moves](../../src/repo/file_comparison/code_moves.rs) prepares immutable Opened rows once per accepted snapshot in the existing analysis worker, checks cancellation between blocks, and lets painting borrow cached rows. A refreshed snapshot clears selection and rejects its predecessor's results. | `near_limit_edited_move_is_prepared_once_and_repaints_do_no_comparison` uses the reviewed 1,000-line long-prefix edited fixture, repeated frames and wheel events with a construction counter, and stale/cancelled snapshots. Real navigation and existing moved tests pass. Debug preparation cost is logged; no native frame-rate claim is made. |
| UI 3, P2: no line-history Wrap | `6c4df1b`: [header controls](../../src/repo/diff/view.rs) expose Wrap independently from whole-file/whitespace/algorithm controls. | Actual `line_history_header_wrap_toggles_without_reloading_or_losing_logical_selection` operates Wrap both ways on a real log-L patch, retaining line ranges, chosen logical line and loaded data. Its font setup was corrected in `b40c0bf8`. |
| Core B1, P1: foreign lock publication/cleanup | `c0e30bf5a8b9f406c0973f1bd9442c5abad7a1a3`: [native publisher](../../src/git/file_comparison/index_transaction.rs) holds the parent directory and identities; [index reads](../../src/git/file_comparison/transaction.rs) are no-follow regular reads. Candidate bytes are never published from the mutable public lock pathname. A private sibling retains displaced inodes; final install is create-only. Lock release captures and inspects what moved, retaining/restoring foreign replacements into absence. Remote equivalent is in [remote_index.py](../../src/git/file_comparison/remote_index.py), added at `6be9a95d42bf2cb8f4a53cfb22d0740588be22af`; remote fingerprints gained descriptor-based no-follow reads at `6619c6da75b2a07fce2e67a205be3d0002f00e15`. | Native tests inject regular/symlink/last-moment lock replacement, changed parent, late index creator and failure cleanup; foreign locks/sentinel/work/unrelated entries survive and foreign bytes are never accepted as a successful index. The remote program runs in owned local fixtures with its own Git wrapper injecting regular/symlink foreign locks. New split-index test stages and reverses using the new publisher. |
| Core B2, P1: configured index policy lost | Same native/remote publisher commits: Git establishes core.sharedRepository + umask policy on the alternate candidate. Captured ownership/access metadata are copied and checked, and policy/identity/mode/ACL drift is rechecked. Unsupported preservation fails before moving the original. [Shared bounded policy API](../../src/standalone/access.rs). Native macOS remote attributes/empty ACL handling were fixed at `8559b0ad18b16b281b9021679ddf8fba65028110` and `9757594ab72efac9bb4f9c1e39a96ef99de516af`. | Explicit 0600 and group modes match actual native Git rewrites; a no-op add was not used as reference. Native/remote owned fixtures cover absent index and nonempty macOS ACL preservation. Native mode/config/ACL-only drift refuses publication; owner equality has a synthetic assertion and group drift runs where an alternate membership exists. |
| Core B3, P2: Windows SACL/integrity omitted | `9e97373771309b5d69ae189412618a529a22f3ca`: [Windows metadata protocol](../../src/standalone/windows_metadata.rs) replaces the Access/Owner/Group subset with bounded raw native descriptor requests for owner/group/DACL/SACL/label/attribute/scope/trust/access-filter components (0x1ff). Copy retains ACL protection flags, compares normalized full descriptors and refuses unavailable privilege/policy before moving MERGED. | Recording checks assert the requested native fields, full copy/check calls and limits. `3199544b66dcbe26b1f0587ad7ebbbecd564865e` adds literal label/audit fixture recording and an explicit cfg(windows), ignored privileged native gate for label/audit-only drift and descriptor equality. **Native Windows gate not executed here; this remains a parent platform-validation requirement, not a Windows approval claim.** |
| Core C1, P2: GNU-only hashing on macOS | `6be9a95d`, `6619c6da`, `ae84baeae5ebd13b7b353c8d895d6397de7eaf49`: fingerprint protocol probes a known hash and accepts GNU md5sum or Darwin md5 -q; errors name missing capabilities. Perl/Fcntl holds a bounded no-follow regular descriptor. | `stock_darwin_md5_is_accepted_without_gnu_md5sum` runs with a PATH containing only stock /sbin/md5 and /usr/bin/perl. Normal/missing sources return exact fingerprints; symlink/oversized sources refuse; outside bytes remain unchanged. Remote mutation script construction and its owned Darwin execution are separate tests. |

All eleven findings have implementation corrections. This table does not clear independent review; native Windows label/audit execution remains outstanding.

## Publication, privacy and operating limits

No atomic identity-conditioned rename is claimed. Preservation renames capture whichever inode actually moved and validate it afterward. Unexpected inodes remain in recovery; restoration and final installation are create-only. Failures preserve a concurrent current destination and retained prior bytes instead of overwriting another writer. Recovery folders are named `gitcraft-index-recovery-*` beside the index and are retained; successful mutation output and errors identify them. External writers that ignore Git's lock can still cause refusal and recovery rather than transparent success.

The remote program uses current shared task/transport/operation entry points. Python 3 standard-library POSIX/ctypes APIs are required for remote mutation, including descriptor-based macOS ACL/xattr handling; missing capability returns an actionable error before publication. Fingerprinting uses Perl/Fcntl plus verified md5/md5sum. It does not require GNU coreutils on stock Darwin. `f92660a4403ac0a0ee4eaa7e2c8c89e0e3bc802d` spools internal Git responses, reads them only after size checks and installs an OS file-size cap for this helper/its owned children. No host PID discovery or manual signal selection was added. Its recording gate passed before commit/push and owned execution. No live transport cancellation/cleanup test was run.

Index and remote Git response limits are 16 MiB; raw working versions remain 2 MiB. SOPS plaintext remains private to the explicit view-owned receiver/rendering path; normal DiffData never receives decrypted converter output in the newly covered routes. SVG input is at most 256 KiB, 4,096 elements and depth64, with bounded inline PNG/JPEG decoding and at most 1,024² output pixels. Complex geometry/overdraw gets a smaller thumbnail. Expansion/effect graphs such as use/filter/mask/pattern are explicitly refused with source-view guidance, and external resources are visibly disabled. Ordinary patch lines retain their existing explicit 5,000-character display cap; full-line copying and the separate snapshot/standalone text renderer are distinct behavior.

New direct resvg/roxmltree dependencies expose already-transitive APIs so the application can override resource resolution and preflight XML. Enabling resvg's raster-images feature adds its bundled gif/zune decoder dependencies; repository inline images are bounded/decoded/re-encoded to PNG by the application before rendering. No version upgrades of existing unrelated crates were made.

The full Windows policy protocol follows Microsoft's [SECURITY_INFORMATION component model](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-information). Reading/copying SACL components can require SeSecurityPrivilege; inability to establish or preserve the complete policy now refuses an existing-output save. There is no fallback to a truncated descriptor and no automatic elevation. The native label/audit fixture must be run explicitly in a suitable owned Windows session after its recording gate; it is not counted among macOS results.

## Exact-source validation

All commands ran against clean `10727c7e88725b4510350ec3416bd1e4ebfcfeda` with Cargo/Rust +1.95.0, CARGO_BUILD_JOBS=2, CARGO_INCREMENTAL=0, TMPDIR=/private/tmp, PATH prefixed by /tmp/gitcraft-integration-tools and SHELL=/tmp/gitcraft-integration-tools/validation-shell. The runner checked HEAD and clean status before each command. Its target directory belongs to this worktree.

| Exact command | Result |
| --- | --- |
| `cargo +1.95.0 test --lib encrypted::tests -- --test-threads=2 --nocapture` | 21 passed |
| `cargo +1.95.0 test --lib standalone:: -- --test-threads=2 --nocapture` | 24 passed |
| `cargo +1.95.0 test --lib diff::wrap:: -- --test-threads=2 --nocapture` | 1 passed |
| `cargo +1.95.0 test --lib file_comparison -- --test-threads=2 --nocapture --skip encoded_remote_readers_and_locked_staging` | 42 passed |
| `cargo +1.95.0 test --lib svg -- --test-threads=2 --nocapture` | 7 passed |
| `cargo +1.95.0 test --lib dvc:: -- --test-threads=2 --nocapture` | 2 passed |
| `cargo +1.95.0 test --lib diff::binary:: -- --test-threads=2 --nocapture` | 31 passed |
| `cargo +1.95.0 test --lib keep_reset -- --test-threads=2 --nocapture` | 2 passed |
| `cargo +1.95.0 test --lib keep_is_a_shared_reset_mode -- --test-threads=2 --nocapture` | 1 passed |
| `cargo +1.95.0 test --lib diff::moved_tests -- --test-threads=2 --nocapture` | 14 passed |
| `cargo +1.95.0 test --lib diff::tests -- --test-threads=2 --nocapture` | 57 passed |
| `cargo +1.95.0 test --lib diff::algorithm_tests -- --test-threads=2 --nocapture` | 6 passed |
| `cargo +1.95.0 test --lib diff::context:: -- --test-threads=2 --nocapture` | 15 passed |
| `cargo +1.95.0 test --test standalone_cli -- --test-threads=2` | 3 passed |
| `cargo +1.95.0 test --lib line_log -- --test-threads=2 --nocapture` | 10 passed |
| `cargo +1.95.0 check --all-targets` | pass |
| `cargo +1.95.0 build --bin gitcraft` | pass |
| `cargo +1.95.0 clippy --all-targets --no-deps` | pass |
| `cargo +1.95.0 clippy --all-targets --no-deps -- -D warnings -A dead_code` | pass |

Fifteen focused test commands reported 236 passing test executions; counts may overlap and are not a unique-test claim. Real SOPS3.13.3/age1.3.2 generated-key fixtures **ran without skip**. The official SOPS asset hash, age release-archive hash and extracted age-keygen equality were rechecked against the supplied verified parent artifacts. Binary/tool hashes, exact commands, timings and per-log SHA256 are in JSON. The default Clippy gate passes with unchanged baseline dead_code warnings; the qualified `-D warnings -A dead_code` gate passes. Strict unqualified -D warnings is not claimed.

The old encoded remote runtime fixture was explicitly excluded with --skip; full suite, legacy bisect timeout/cancel and legacy remote cleanup/runtime gates remain held for the parent ownership fix. No live SSH/WSL/DVC remote, native Windows or native Linux runtime was exercised. The new remote transaction program was executed directly in owned macOS fixtures through the shared process service, bypassing the held legacy transport cleanup paths. No CI command, PR creation, rebase, protected branch/tag push, subagent spawn or other worktree modification occurred.

The final archive contains only passing checks on 10727c7e88725b4510350ec3416bd1e4ebfcfeda. Earlier correction attempts exposed test-font initialization, reference-mode setup and fixture lint issues; they are not substituted for this final gate. Independent R2 review and combined validation belong to the parent.

Evidence: [JSON](issue51-fileviews-corrections-2026-11-08.json), [full logs](issue51-fileviews-corrections-2026-11-08-tests.log.gz).

## Corrective commit sequence

```text
8d87f59c7ea58129b1216be52f6d9964c402986f fix: close untracked and line-history SOPS conversion bypasses
10447d86895f1068a9222ef2f148a385564cc6bd fix: isolate and bound SVG resources in cancellable preview workers
6c4df1b1214c86c4b0d6484096584232d2f3ddbc fix: revoke stale matrix pairs and prepare moved comparisons outside paint
9e97373771309b5d69ae189412618a529a22f3ca fix: capture and preserve full Windows SACL and integrity policy
d4ae31a7f17eefd6c60b6ecb9dec8cceadb97816 test: initialize custom fonts before line-history interaction frames
b40c0bf84b95b0f97587f72602f6b1cc793acfb2 test: defer line-history paint until named fonts are installed
c0e30bf5a8b9f406c0973f1bd9442c5abad7a1a3 fix: publish staging indexes create-only with retained identities and Git access policy
6be9a95d42bf2cb8f4a53cfb22d0740588be22af fix: preserve remote index identities and policy with portable capability checks
63303228797e9ebc4d9b93d47984bf4a3de426ca test: use the completed child output in owned remote fixtures
8559b0ad18b16b281b9021679ddf8fba65028110 fix: use bounded native macOS index attribute APIs in remote transactions
87cdd2b5d5a146a8f9641688e1a2f82db2691feb fix: bound SVG overdraw and verify real relative-resource isolation
9757594ab72efac9bb4f9c1e39a96ef99de516af fix: distinguish an absent native macOS ACL from an unreadable policy
3199544b66dcbe26b1f0587ad7ebbbecd564865e test: record full-policy Windows fixtures and add explicit native label and audit gates
e1c5b3562e84b01d5bfe7d38ad2656bd764e26d9 test: compare actual native index rewrites and cover ACL owner and absent-index policies
cc60ef051868dde5557005606784f6e07e1ea586 test: propagate owned index race-fixture errors without panic helpers
6619c6da75b2a07fce2e67a205be3d0002f00e15 fix: fingerprint remote indexes through bounded no-follow descriptors
ae84baeae5ebd13b7b353c8d895d6397de7eaf49 test: execute bounded Darwin index fingerprints without GNU tools
6c54c2af6878f012851f8f621c7ae702d7cc031d test: keep fixture lint gates clean and recheck split-index publication
f92660a4403ac0a0ee4eaa7e2c8c89e0e3bc802d fix: bound remote Git output spools before loading transaction responses
10727c7e88725b4510350ec3416bd1e4ebfcfeda fix: reject queued matrix cells while its row snapshot is refreshing
```
