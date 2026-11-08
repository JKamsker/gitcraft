# Workflow content/UI fixed-objection review R2

**Verdict: SCOPED PASS.** All nine named R1 groups (Content 1–4 and UI 1–5) are corrected in the immutable source reviewed below. I found no retained blocker in those fixes or their directly changed integration. This is source plus pure/headless acceptance of those objections, not acceptance of the six separately reviewed Core objections, the whole batch, actual publication/mutation behavior, or the final combined application.

## Binding and permitted work

- Candidate/application: `fe88e1dc52828c278515e2a7fa9cb6c6347ad1c4`.
- Prior reviewed application: `bf6089b684597d878d3c2f256b7c69d64c6785a0`.
- Source Git repository: `/Users/jonas/.t3/worktrees/gitcraft/t3-940761ce`; inspected an immutable archive at `/tmp/gitcraft-workflows-r2-fe88e1dc`, not mutable writer files. Independently compared 21 reviewed source/manifests byte-for-byte with `git show <candidate>:<path>`.
- Read both full R1 reports, the full R2 handoff and exact-candidate status JSON, original issue #35, correction source and the relevant tests. Per-file SHA-256, report/log hashes, source method, execution classes and exclusions: `/tmp/gitcraft-51-workflows-content-ui-review-r2-manifest.json`.
- No tracked edits, commits, pushes, CI, PR, subagents, application launch/discovery, actual Git-under-test, forge/helper/network/auth/Docker/native/process-control fixtures, or full suite. Only source-object Git reads and 12 explicitly selected audited pure/headless test entries were executed. The test runner processes are not process-control fixtures.

## Nine dispositions

### Content 1 — CLOSED: preserve a multiline span over serial relocations

`src/repo/review_workflow/anchors.rs:35` verifies every line of a bounded range maps contiguously, rather than checking its endpoint alone. `relocate` returns both target and mapped start; `relocate_threads:184` persists that start together with the mapped end and source side. The former `start: None` reduction on the successful relocation path is gone. Snapshot serialization therefore preserves the span needed for the next rewrite. Unknown, edited, deleted, noncontiguous or oversized spans refuse relocation; pending-review anchors remain excluded.

Independently reran the pure production regression: original lines 2..3 map to 3..4, the actual Anchor is serialized/deserialized, a later edit of interior line 3 refuses relocation despite an unchanged endpoint, and a later prefix insertion preserves the whole mapped range. This closes the specific two-rewrite/restart false-success case. Actual Git rename/rewrite integration was not rerun.

### Content 2 — CLOSED: bind and clear relocation maps across refresh/error

`anchors.rs:10–24` carries base/head and accepts a map only if both match and no error exists. `src/repo/pr_view/mod.rs:41` additionally requires accepted details/files with matching head and no details load. `load_pr:121` immediately clears the map; `pr_files_arrived:197` clears it again before replacing the files. Errors replace rather than retain targets, and the existing PR-request callback guard rejects superseded work. `pr_review/send.rs:43`, `pr_review/line_panel.rs:160` and the PR page consume the current-map accessor rather than raw retained coordinates.

The pure current/wrong-head/error/default-map checks passed independently. Source tracing confirms the previous B map cannot remain actionable while C is loading or after its relocation fails. No provider or injected full application PR-message fixture was run.

### Content 3 — CLOSED: do not infer the push baseline from another fetch server

`src/repo/review_workflow/echo_push.rs:326–341` uses the empty tree for every resolved source, instead of subtracting a cached fetch tip. Split fetch/push URLs, custom fetch maps and multiple push destinations therefore cannot produce the demonstrated pair(D,D) merely because an unrelated fetched tip equals D. `echo.rs:199` reads the corresponding source diff with external diff/textconv disabled. The echo UI explicitly describes this as a conservative full-source scan; its likely extra matches are disclosed rather than advertised as an exact remote outgoing range.

The pure prefix/baseline-policy check and frozen config/environment checks passed independently. Unknown/context-changing prefixes are explicitly refused by `echo_push.rs:342`; unsupported plans retain the manual/advisory path. Actual split-server/custom-map publication fixtures remain held. This accepts the conservative correction explicitly allowed by R1, without claiming a remote baseline was queried or proven.

### Content 4 — CLOSED: carry the checked source through the queued callback into the actual push

`echo_push::prepare:39` captures an Arc-owned `git::CommandSource`, resolves evidence through it and binds the actual push argv to it. The source at `src/git/command/source.rs:31` copies refs and HEAD into a private Git directory and captures effective configuration/routing; `source_env:333` selects that private Git directory while sharing the original object store. Thus the push's original ref names resolve against the captured source, including when original refs advance after a UI callback was queued. The correction preserves the original push options/refspecs rather than deleting force leases or changing their semantics.

`echo.rs:328–444` retains source ownership in the waiting operation and then through operation execution/sign-in retry. Source-bound execution checks live refs/config/rulebook again inside the worker. Both ordinary command execution (`git/command.rs:152`) and watched operations (`git/command/watched.rs:196`) bind that source before dispatch and retain it for completion. An expired/mismatched registry marker refuses execution. The ordinary runner and watched runner both perform successful-push tracking/upstream reconciliation with an explicit post-publication uncertainty message if reconciliation fails (`source.rs:408`).

The audited pure source-env/config/prefix checks passed independently. These test the configuration/argument policy, not actual Git's publication, credential retry, leases, upstream reconciliation or the queued D→E fixture. The latter remains authored/ignored and unexecuted. Source ordering and immutable ref/HEAD routing close the prior mutable-source callback defect without treating an earlier stamp alone as proof.

### UI 1 — CLOSED: reload current membership and preserve hunk/document ownership

`src/repo/review_workflow/continuous.rs:10–16` retains an explicit Working/Commit/Comparison/PR descriptor. `load_review_document:164` re-enumerates working status, commit files or comparison files; PR loads require the current PR number/URL, matching details/files head and accepted files request, then capture that exact range. Identity is shown in the document. PR details reload invalidates the document immediately and accepted files trigger a reload (`pr_view/mod.rs:122,254`).

Every open/reload/close invalidates the old document request and cancels its between-file token; result installation requires current ownership. Hunk completion at `continuous.rs:113` checks the same owner before applying or replacing feedback. A stale patch-read error is explicitly “Hunk refused,” and a nonstarted operation is no longer reported as submitted. Source fingerprint revalidation before patch preparation remains in place.

Independent pure checks covered fresh added membership, unstaged→staged membership, old-request invalidation and close ownership. The writer's stale-hunk/late-callback entry was inspected, not independently rerun because it constructs a RepoTab; no application constructor was needed for this review. Its relevant early refusal/ownership source was inspected directly. Actual index/worktree mutation and complete PR-message/UI routes remain held.

The associated R1 scale observation is also addressed in source: 500 files, 250,000 rows and 32MiB retained text/header budgets, refusal instead of a partial document, between-file cancellation and clipped fixed-height visible row ranges (`continuous.rs:235–267,427–470`). Budget/viewport arithmetic checks passed independently. This is not a native performance or in-flight Git cancellation measurement.

### UI 2 — CLOSED: metadata-only sections are visible and inspectable

`continuous.rs:472` always renders the file comparison button, independently of hunk presence. `metadata_ui:485` shows work status/area and old→new paths where available, the raw parsed Git header (including mode/rename/copy metadata), and explicit no-text-hunk/binary-image-LFS guidance. Commit/comparison rename/mode information survives in the raw header even without a working status record.

Independently reran the actual egui headless metadata renderer and pointer-click test for mode-only, pure rename/copy, empty add/delete and binary-header examples. It asserts visible metadata and the actual `Action::OpenDiff` route. Those are captured header inputs, not newly executed Git fixtures or a native window.

### UI 3 — CLOSED: storyboard navigation uses the accepted pinned full file and logical line

`storyboard.rs:516–529` opens the exact commit/path in `FileTab::File`, sets a logical target/mark on that FileView and removes the independent default-options diff/index read. `src/repo/file_text.rs:157,232` consumes the target only after successful file text is available, then scrolls/marks by the full file's logical line. Its caller passes the FileView's own location (`file_view/mod.rs:158`). Existing accepted file messages match path and immutable revision (`messages.rs:564`); an unrelated file/revision result cannot supply this view's text. Reopening the same immutable file is consistent with its logical target.

The actual headless FileText renderer independently passed with no text initially (line 90 remains pending), then a 100-line file, and line 90 visible after the scroll. Since this route uses a full file rather than the commit patch, unchanged files/out-of-hunk lines and whole-file/whitespace/algorithm diff settings no longer select mismatched patch rows. Full card→Git-read callback ordering/native navigation remains unexecuted.

### UI 4 — CLOSED: invalidate/recompute storyboard freshness on accepted PR generations

`load_pr` clears visible storyboard rows immediately. `pr_files_arrived` calls `refresh_storyboard`; `storyboard.rs:466–514` requires accepted files request as well as matching details/files head, including a same-head refresh while base/files are still from the previous generation. Cached Cards are retained separately from Shown conclusions. Re-evaluation preserves imported/local provenance, clears prior rows, and accepts completion only for current PR and storyboard request. Edit/load/fetch/publish action entry paths also require the accepted details/files generation; edit actions retain their captured head checks.

Independent pure checks passed for invalidation, retained Card cache/import provenance, rejection of the old result, acceptance of new stale rows, and refusal of same-head old-files generations. Existing blob/tree/range-diff/rename freshness calculation is retained; cached old conclusions are no longer simply painted after a new accepted PR refresh. Actual H1→H2 code/test/rename message integration remains held; the tests supply state/evidence and do not execute those Git algorithms.

### UI 5 — CLOSED: adaptation reverse/reapply witnesses participate in newest-first survival

`radar.rs:145–195` derives both the linked adaptation's patch ID and its reverse only when it is reachable, then passes them alongside original fix/reverse IDs through one newest-first witness ordering. An ancestry-only Adapted fallback is gone. A later reverse-adaptation witness yields Reverted, whose existing UI renders amber (`radar.rs:443`); an ensuing positive witness can restore Adapted/Same. Missing/truncated evidence with no inspected witness remains NotChecked, and fix/tree/command behavior evidence keys remain independent (`evidence_key`).

Independently reran the pure ordering regression covering adaptation→revert→reapply, ordering with original/reverse witnesses and absent history. Real Git patch-ID generation/history order, a live branch revert, regression-command execution and native amber display were not executed.

## Preservation and remaining acceptance boundary

Independently verified `freshness.rs`, `rust_dependencies.rs`, `choreography.rs`, Cargo.toml and Cargo.lock are byte-identical to bf6089b6. Their prior scoped assessments and disclosed language/behavior limits are preserved; this is not new execution of their gates. Existing explicit viewer-comment import, opt-in cited disabled AI rules, manual handling of unknown push plans, shell/terminal exclusion, full-file/test-card distinction and separate patch-versus-behavior evidence remain in the reviewed routes. There is no new parity requirement in this review.

The six Core fixes, shared credential/process implementations, and final cross-batch integration are independent dependencies, not implicitly approved here. No old common finding is reintroduced as a new workflow blocker.

For later authorized runtime validation, the finite relevant held gates remain: serial actual Git rewrite/rename anchors; delayed/failing PR details/files and hunk callbacks with actual index effects; storyboard H1→H2 code/test/rename full-route navigation; exact adaptation patch-ID/revert/reapply UI observations; and the frozen-source queued push/ref-routing/lease/upstream/retry fixture. No such execution is represented as passing by this report.

## Evidence execution and provenance

- Copied the writer's named lib-test executable, checked SHA-256 before execution: `/tmp/gitcraft-workflows-r2-fe88e1dc-tests`, `fba747936ba8c153dbd59a7c25f1a654c88c76d4e76ac72c9fe792f759059343`.
- **Independently reran 12 exact audited pure/headless entries, all passed.** Commands use that copied executable with `--exact <full test name> --nocapture --test-threads=1`; names, return codes/durations and outputs are in `/tmp/gitcraft-51-workflows-content-ui-r2-pure.json` and `.log`. No broad module filter was used.
- Artifact-to-source binding relies on the writer's exact fe88 receipt and hash, plus this review's immutable source/test inspection. There was no fresh independent build-to-source attestation and no application rebuild. This distinction is preserved in the manifest.
- Inspected writer exact-candidate 28-test/check/qualified-lib-clippy/fmt logs and hashes from the status JSON. Those remain writer evidence, not independently executed compile/lint gates. The recorded lint qualification allows baseline dead_code.

No additional scoped correction is requested. The nine demonstrated source defects are closed within these stated execution limits.
