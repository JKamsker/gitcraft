# Issue #51: diff, staging and file-tool implementation batch

Source validated: `5df74dbb3792a34ee0b1d13c301c9aa552aa6c69`. Base: `a850de26b0a0778ea96fe34cd1498e6e78928492`. Work branch: `t3/diff-staging-views` on `JKamsker/gitcraft`. All implementation units and fixes were committed and pushed with matching remote SHAs. Author throughout: Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>.

Status: **locally validated; awaiting parent independent adversarial review and combined gate**. This handoff covers the ten assigned ideas from #40, #44 and #50. Requirements were read from the original issue bodies in `/tmp/gitcraft-issue51-issues.json` and the final content audit. The parent-owned combined ledger and audit commit were not edited or cherry-picked.

## Requirement coverage

| Assigned idea | Reachable behavior and implementation | Feature checkpoint(s) |
| --- | --- | --- |
| #40 migration map | File views menu and palette open old/new folder columns, rename/copy ribbons and Git similarity scores. Clicking a ribbon reads pinned raw blobs and shows content edits separately from movement. Backend: [migration.rs](../../src/git/file_comparison/migration.rs); [ribbon view](../../src/repo/file_comparison/view.rs). Tests exercise renames, copies, immutable sides and the actual ribbon click. | `479f9655d6e2d998e05e8e8d1370a46e30b0917d` |
| #40 encrypted SOPS diffs | Recognizes JSON/YAML/dotenv/INI and binary envelopes. Show decrypted explicitly invokes local SOPS with local keys; SSH/WSL provide ciphertext only. Content changes are separate from recipient/wrapped-key and other encryption metadata changes. Ambiguous JSON/binary envelopes require a choice. [Private owner/reader](../../src/repo/diff/encrypted/mod.rs), [tool](../../src/repo/diff/encrypted/tool.rs), [view](../../src/repo/diff/encrypted/view.rs). Tests cover formats, recipient rotation, stale sources/actions, close/reload/park lifetimes, diagnostics, cancellation and bypass of egui's shared galley cache. | `0fe738232fc3c627474ab96e58d0de4dff9be880`; lifetime fix `52df7dbfb568d4c836256ac784850926dad812a4`; read-only controls `a3073837fa1df8b95f3a4e9a994bb326d8d98c9d`; private wrap `8c8286230c319ac4d03ebe6ab73ea6539c86fded`; final controls `5df74dbb3792a34ee0b1d13c301c9aa552aa6c69` |
| #40 moved code across files | File views menu, palette and normal diff header open change-wide analysis of staged, tracked working, commit, commit-range or commit-to-working changes. Shared M-markers and muted block colors pair source/destination; Moved to shows edits/re-indentation and opens either numbered location after its patch arrives. [Reader and view](../../src/repo/file_comparison/code_moves.rs) reuse the existing moved-block matcher/comparison. Tests cover edited/re-indented blocks, same-file exclusion, sources, cancellation and actual UI navigation. | `1d9a7c1a4bb6a43350c4bcf834740831238f2302` |
| #40 prose/general wrapping | Wrap is available on ordinary unified and split patches, independent of existing prose rendering. Physical row geometry preserves logical selection, search highlights/reveal, change-map marks, navigation and algorithm restoration. [Geometry](../../src/repo/diff/wrap.rs), [rows](../../src/repo/diff/rows.rs), [painting](../../src/repo/diff/paint.rs). Private SOPS and standalone/snapshot comparisons also wrap; snapshot text is no longer shortened at 2,000 characters. [Full comparison text](../../src/repo/file_comparison/text.rs). Tests cover Unicode wrapping/selection rectangles and full long-line layouts; existing patch/algorithm/context/move regressions pass. | `e4904f7c54d642cf6625f0901c9d1b60ca16d5d5`; exact widths `b209213b9ead67eb2b1f5ad7b9e1fb094ccb13ad`; private wrap `8c8286230c319ac4d03ebe6ab73ea6539c86fded`; standalone/snapshots `2746bf76b300a58d3421ba8c17c511a954639c2d` |
| #40 documents/binary SVG completion | SVG uses the image loader for before/after previews, with Rendered SVG / SVG source switches retaining the normal patch. Git-quoted rename names keep the before image. [Images](../../src/repo/diff/images.rs), [switch](../../src/repo/diff/view.rs). Existing document native/textconv extraction, archive members, sizes and hex remain in [binary](../../src/repo/diff/binary/mod.rs). SVG loader/patch/rename tests and 31 binary tests pass. | `c96182b5d8493177e0977b6ec83ba9c0efc8636f`; quoted rename fix `f03818bed50ddd144a1c00988a6138ee859a44c5` |
| #40 Tables/DVC missing Pull action | DVC: Pull is in the palette and DVC pointer view. A confirmation explains materialization and lack of automatic undo; its fixed command runs through the existing operation/mutation/process infrastructure with offline/network guards. [Plan and execution fixture](../../src/git/dvc.rs), [action](../../src/repo/apply.rs), [pointer entry](../../src/repo/diff/rich/dvc_view.rs). Tests record fixed argv/network classification and execute an owned fake DVC program to verify cwd and success/failure status. | `e21eb4e58a60dfd8bcdf059f2acd0fcc78638b91`; execution coverage `80baf62008bb0860f4ee109ed821c1557a4849dd` |
| #44 standalone diff/merge and arbitrary files | Native `--diff`/`--difftool A B` and `--merge`/`--mergetool BASE LOCAL REMOTE MERGED` route before repository/single-instance forwarding. Separate windows use Gitcraft comparison and conflict editing, binary side choices and correct success/cancel/error exits. Settings External tools previews and registers named Git tools; default-tool selection is explicit. [CLI dispatch](../../src/main.rs), [standalone modules](../../src/standalone/mod.rs), [registration](../../src/standalone/registration.rs). Output transactions detect drift/aliases/parent swaps, retain the old inode and preserve access policy; native macOS ACL drift tests pass. UI tests operate the editor, save and close flows. | `646b8b56039ea9bd7da918366c90e42aaddee1de`; APFS fixture `55adde631a4e02d35d7d8358af8660ecdba8666e`; macOS ACL fix `a4f568a533a424af7524e508e22c4a88ae56d8f8`; wrapping/accessibility `2746bf76b300a58d3421ba8c17c511a954639c2d`, `87f20f23a3242a939501b3c7876266795b7d5704` |
| #44 reset --keep | Keep is the fourth shared reset mode used by commit menus/palette, with preservation/refusal wording, expected branch/HEAD guards and safe --keep undo. The older graph-specific menu already contained Keep; its shared list still had three modes and was completed here. [Shared modes/plan](../../src/repo/commit_actions.rs), [real keep/refusal tests](../../src/repo/graph_view/menu.rs). | `b0d274fbc9c8b514aa4695e39be8263ed3c76f60` |
| #50 three-state staging lens | File views, palette and Three-state in a working diff show aligned HEAD → index → working versions. Both boundaries stage/unstage selected lines; later edits undoing staged content are marked. [Model/view/actions](../../src/repo/file_comparison/mod.rs), [raw lens](../../src/git/file_comparison/lens.rs), [guarded transaction](../../src/git/file_comparison/transaction.rs). Shared mutation tasks retain stale-snapshot/index/HEAD/work/attribute checks. Tests cover real boundary buttons, partial creations/deletions/renames, intent-to-add, modes, CRLF, symlinks and read-only conversion/submodule cases. | `479f9655d6e2d998e05e8e8d1370a46e30b0917d` |
| #50 file versions across branches | File views and palette choose up to eight branch refs and a folder. Pinned commit columns show equal-byte markers across rows/cells, missing files and modes; two cells open a read-only content diff. [Matrix backend](../../src/git/file_comparison/mod.rs), [view](../../src/repo/file_comparison/view.rs). Tests cover pinned ambiguous refs, literal/raw paths, missing modes/blobs, no lazy fetching and actual cell selection. | `479f9655d6e2d998e05e8e8d1370a46e30b0917d` |

Original conflict sections and Keep both binary conflicts were retained, not reimplemented. The profile/connection feature stack was not imported. Comparison reads adapt current Git/remote APIs; local reads and SOPS use the current owned process service. `processes::run_while` only polls/cancels its existing owned ticket; it adds no PID discovery or signal selection.

## Privacy, operation safety and limits

SOPS recognition runs before patch generation and disables Git textconv for recognized or unreadable/oversize candidates. Decrypted text never enters DiffData, the tab Msg queue, settings, activity, temp files, syntax caches or the shared galley cache. A private receiver owns results. Closing, refreshing, changing reading modes, opening snapshots or parking/leaving the tab drops private results and cancels work. Numeric wrap offsets stay private. Ciphertext-only reads run separately so a cancelled plaintext owner does not wait for bounded SSH cleanup. SOPS failures are generalized and never show raw diagnostics. The fixed stdin/format invocation agrees with [SOPS v3.10.0's upstream CLI](https://github.com/getsops/sops/blob/v3.10.0/cmd/sops/main.go).

Snapshot versions are bounded to 2 MiB; matrix/migration lists to 2,000 paths; branch selection to eight. Conversion-dependent and binary lens boundaries stay read-only. Cross-file code analysis is explicit and bounded to 300 changed files, 6 MiB patches, 50,000 lines and 40 seconds. New/untracked destinations must be staged first; non-UTF8 paths and encrypted content excluded from code analysis are reported. Mutable patches are checked again before results are accepted. SOPS displays UTF-8 text only, up to 2 MiB, 20,000 lines and 8,192 characters per line; unsupported binary output is refused explicitly.

Documents use the audited existing native extractors or configured Git textconv; automatic pandoc selection was not added. Incomplete native extraction remains visibly marked. Platform-specific native save policies fail closed when their metadata cannot be preserved. This machine exercised native macOS ACL behavior; Windows helper protocols have recording tests, not a native Windows runtime result.

New direct dependency `same-file = 1.0.6` exposes the already-transitive native file identity API for alias/race checks. macOS-only `exacl = 0.13.0` supplies safe native ACL reads/writes under the crate's unsafe-code prohibition. The existing rustix dependency enables its process feature for typed ownership IDs. No dependency is used as a replacement process supervisor.

## Validation on the exact source commit

All commands below ran with `TMPDIR=/private/tmp`, `PATH=/tmp/gitcraft-integration-tools:$PATH`, `SHELL=/tmp/gitcraft-integration-tools/validation-shell`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, Cargo/Rust 1.95.0 and this worktree's own APFS-cloned target.

| Exact command | Result |
| --- | --- |
| `cargo +1.95.0 test --lib encrypted::tests -- --test-threads=2 --nocapture` | 20 reported successes |
| `cargo +1.95.0 test --lib standalone:: -- --test-threads=2 --nocapture` | 23 reported successes |
| `cargo +1.95.0 test --lib diff::wrap:: -- --test-threads=2 --nocapture` | 1 reported successes |
| `cargo +1.95.0 test --lib file_comparison -- --test-threads=2 --nocapture` | 32 reported successes |
| `cargo +1.95.0 test --lib svg_ -- --test-threads=2 --nocapture` | 3 reported successes |
| `cargo +1.95.0 test --lib dvc:: -- --test-threads=2 --nocapture` | 2 reported successes |
| `cargo +1.95.0 test --lib diff::binary:: -- --test-threads=2 --nocapture` | 31 reported successes |
| `cargo +1.95.0 test --lib keep_reset -- --test-threads=2 --nocapture` | 2 reported successes |
| `cargo +1.95.0 test --lib keep_is_a_shared_reset_mode -- --test-threads=2 --nocapture` | 1 reported successes |
| `cargo +1.95.0 test --lib diff::moved_tests -- --test-threads=2 --nocapture` | 14 reported successes |
| `cargo +1.95.0 test --lib diff::tests -- --test-threads=2 --nocapture` | 57 reported successes |
| `cargo +1.95.0 test --lib diff::algorithm_tests -- --test-threads=2 --nocapture` | 6 reported successes |
| `cargo +1.95.0 test --lib diff::context:: -- --test-threads=2 --nocapture` | 15 reported successes |
| `cargo +1.95.0 test --test standalone_cli -- --test-threads=2` | 3 reported successes |
| `cargo +1.95.0 check --all-targets` | pass |
| `cargo +1.95.0 build --bin gitcraft` | pass |
| `cargo +1.95.0 clippy --all-targets --no-deps` | pass |

The 20 SOPS test successes include **one opt-in real SOPS/generated-age fixture that self-skipped** because explicit tool paths were unavailable. It did not perform real encryption/decryption. The other format, redaction, recording, private-cache and owned-child cancellation fixtures ran. To run the isolated real-tool fixture later, supply absolute `GITCRAFT_TEST_SOPS` and `GITCRAFT_TEST_AGE_KEYGEN` paths and select `real_sops_and_generated_age_compare_formats_and_key_rotation_offline` under the same environment.

No live SSH/WSL server or DVC remote was contacted. Encoded remote fixtures and an owned fake DVC executable ran. No host-wide process searches/signals were used. Pure command/recording checks preceded owned-child cancellation tests, and every process-bearing unit was preserved by commit/push/SHA verification first. Full-suite and parallel_bisect real timeout/cancel tests were deliberately excluded under the known STOP_SCRIPT safety restriction. Parent combined validation and independent adversarial review remain outstanding. No CI command or PR was created.

Clippy/check/build exit successfully. Remaining warnings are in unchanged baseline `desktop_entry.rs` and `diagnostics.rs`. During implementation, focused tests exposed and led to fixes for read-only decrypted controls, macOS clone ACL inheritance, APFS fixture filenames, texture-delta test cleanup, text accessibility and exact wrapping geometry. These final logs are the passing rerun, not claims about earlier failing checkpoints.

Machine-readable commands, statuses, timings, source SHA and log hashes: [issue51-diff-batch-2026-11-07.json](issue51-diff-batch-2026-11-07.json). Full passing output: [issue51-diff-batch-2026-11-07-tests.log.gz](issue51-diff-batch-2026-11-07-tests.log.gz). This documentation-only handoff preserves the application gates above; it does not claim new application test execution after the source SHA.

## Commit sequence

```text
479f9655d6e2d998e05e8e8d1370a46e30b0917d feat: recover migration map, staging lens and branch file matrix
53e1d57add50dbd9d8cd34a4d70900fa89f4f1a7 test: construct raw Git paths without invalid APFS filenames
0fe738232fc3c627474ab96e58d0de4dff9be880 feat: add private opt-in SOPS comparisons through shared tasks
646b8b56039ea9bd7da918366c90e42aaddee1de feat: recover standalone difftool and guarded mergetool windows
c96182b5d8493177e0977b6ec83ba9c0efc8636f feat: render SVG diffs with an editable-source patch switch
e21eb4e58a60dfd8bcdf059f2acd0fcc78638b91 feat: add DVC Pull to the palette and pointer view
e4904f7c54d642cf6625f0901c9d1b60ca16d5d5 feat: soft wrap every patch with logical navigation and selection
b0d274fbc9c8b514aa4695e39be8263ed3c76f60 feat: expose Keep reset in shared commit menus and palette
1d9a7c1a4bb6a43350c4bcf834740831238f2302 feat: detect and compare code moved across files in a change
52df7dbfb568d4c836256ac784850926dad812a4 fix: release private SOPS rows promptly during ciphertext revalidation
55adde631a4e02d35d7d8358af8660ecdba8666e test: keep standalone native path fixtures valid on APFS
a3073837fa1df8b95f3a4e9a994bb326d8d98c9d fix: hide whole-file staging controls in private decrypted views
b209213b9ead67eb2b1f5ad7b9e1fb094ccb13ad fix: share exact wrapped widths for paint geometry and search
a4f568a533a424af7524e508e22c4a88ae56d8f8 fix: preserve exact macOS ACLs on owned merge replacements
a8822798816063da4591c7f0393029e33e5b1f07 fix: use shared accessibility focus for comparison controls
8c8286230c319ac4d03ebe6ab73ea6539c86fded feat: wrap private comparisons without shared plaintext layouts
2746bf76b300a58d3421ba8c17c511a954639c2d feat: wrap standalone and snapshot comparisons without truncating text
c3d444d6b693f571f4a0e319890cd995dbef8994 refactor: satisfy comparison lint gates and keep test failures fallible
87f20f23a3242a939501b3c7876266795b7d5704 fix: expose comparison text to screen readers and UI navigation
80baf62008bb0860f4ee109ed821c1557a4849dd test: execute the DVC pull plan through the shared process service
f03818bed50ddd144a1c00988a6138ee859a44c5 fix: retain SVG before images for Git-quoted rename paths
5df74dbb3792a34ee0b1d13c301c9aa552aa6c69 fix: keep decrypted controls specific to the private reading mode
```

Recovery provenance: selected file-comparison modules came from `codex/file-comparison-views` (`cdd45af340b433b125f29e2347705cf2335053c3`), standalone modules from `codex/standalone-file-tools` (`f1ee4f33cea9f0b3efdd9c9f38e5afc4ea976083`), and SOPS modules from `codex/encrypted-diffs` (`60cb5790cd478a0b1811ee4a705fd758a0b61a14`). Their older connection/practice/process stack was not cherry-picked. Current task/process/profile APIs, output limits, cancellation, privacy ownership and UI wiring were adapted and validated here. The retained branches remain untouched.
