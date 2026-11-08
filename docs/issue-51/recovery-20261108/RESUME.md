# Gitcraft issue #51 — disk-cleanup recovery checkpoint

Created 2026-11-08. This folder is deliberately OUTSIDE both directories the user will delete:
`/Users/jonas/.t3/worktrees/gitcraft` and `/private/tmp`.

## Start here

- Independent standalone checkout: `source/` (its own `.git`, no alternates or worktree link).
- Application checkpoint: `886b42b54ba23e01e0cdfc477282bbc4afca3de3` on GitHub `JKamsker/gitcraft`, branch `t3/implement-issue-51-ideas`.
- The separate `t3/issue51-resume-20261108` branch adds only this recovery documentation/evidence index. Its application tree must equal 886b42b5.
- Full local Git backup: `all-refs.bundle` (91 local heads plus other refs; verified self-contained). `worktrees.json` records the 74 existing worktrees; ALL were clean at the scan. `github-reachability.json` distinguishes exact remote tips from commits retained in existing remote history. 90 of the 91 local heads are independently verified reachable from GitHub; the old unrelated local dev tip is preserved in the bundle. A broad snapshot push was rejected by GitHub GH007 for that old private-email dev tip; no privacy setting/history was changed. Five otherwise missing non-dev branch tips were then successfully preserved under t3/recovery-20261108/*. Do not rewrite old commits or disable email privacy.
- Evidence: `evidence/private-tmp/` contains 4,031 text files, 298,714,169 bytes, copied with original bytes and verified SHA-256. `evidence-manifest.json` maps every original `/private/tmp/...` path to its backup and hash. An additional 1,779 source/support files (24,044,817 bytes) are indexed in `additional-evidence-manifest.json`. Original `/tmp/...` references resolve to the same files (`/tmp` was a symlink to `/private/tmp`).
- No build targets, dependency caches, fixture executables, private test HOME/config directories or running processes are needed for recovery. Rebuild caches when work resumes.
- Never execute an old script blindly: most hard-code the deleted checkout and `/tmp` target/tools paths. Read and update an external copy for the new checkout/target/private fixture locations. Do not change immutable historical logs or their recorded hashes.
- Native agent `/root/audit_operation_opacity_fix` was interrupted before producing a report. Restart only that bounded review using a fresh compact brief if still needed; do not reawaken long retired writers.

## Objective and delivery policy

Implement all 93 original missing/partial ideas from JKamsker/gitcraft issue #51. NOT complete; no final PR or CI was opened/run. Original source issue bodies and audits are in the backed-up `gitcraft-issue51-issues.json`, `gitcraft-51-audit-{core,content,platform}.md` and repository `docs/issue-51/`.

Final delivery requires combined local checks/tests, independent adversarial review on the final immutable candidate, then fresh fork `dev` history with 93 feature commits plus F000 (94 total), exact accepted tree equivalence, one consolidated PR. History assembly has NOT started. Existing history allocation drafts are old inputs, not an accepted final allocation. Reverify current fork dev; it changed during the long task. Do not force push dev/master/tags.

Every completed unit: commit and push to JKamsker/gitcraft, verify BOTH identities exactly `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>` and exact remote SHA. Preserve all published work. No upstream Vancete writes. No CI invocation/dispatch/rerun/watch/wait during implementation. PR only after required final gates pass; register it with T3 link_pull_request.

User preference: one bounded task per FRESH agent, let it finish, no endless follow-up chatter. Latest maximum was four active total including root. No active source writers remain at this checkpoint. Do not start new feature/gap-hunting streams; finish the closing steps.

Process safety: never derive signals from host-wide PID selectors. Root-only real suite was authorized after contained source review and prior narrow owned runtime gates. Individual agents may do inert supplied-state/headless tests only, no App/RepoTab/discovery/Git/native/process/network fixtures. Preserve/push exact source before risky tests.

## Exact current state

`886b42b5` integrates mechanical worker `8733bd8be5fe2a23edab0da99efd956aca9befa9` from base cbaf via its exact 180-file selective patch. The patch applied cleanly after later functional work; catalogs and later closed functional units were not replaced. Root is clean and application source pushed. No post-integration root test pass is claimed.

Mechanical handoff: `evidence/private-tmp/gitcraft-mechanical-final/HANDOFF.md`. Worker all-target compiler check and changed-file rustfmt pass. Retention proof covers 11,950 production literals, 1,666 assertion macros, 311 test names, 1,721 function bodies, 22 hard exclusions. Normal Clippy at worker source fails on exactly two excluded operations.rs test unwrap/expect sites. Strict diagnostics reduced 905 to 136; not a strict-green claim. No worker test execution occurred.

The root qualified Clippy command at 886 was interrupted during compilation by the user's disk-cleanup request. `gitcraft-root-final-lint/886-qualified.{jsonl,stderr}` has NO build-finished record; treat it as incomplete, not pass/fail evidence. No process still held these log files when checked. No source changes from the exploratory lint inspection were made.

Parent `844d74093b23a7520d68d57f5e2311e56190b0aa` fixes OP-NOTICE-1 by removing generic push-preflight diagnostic parsing from notice::operation fallback. Generic fallback now uses existing `git::display_error(raw)` only. Adds adversarial provider LFS-suffix EN/DE/EN cases across Failure/status/finished/activity. Exact root gates PASS: all-target check plus 16 individually selected inert tests. `gitcraft-root-op-status-opacity/gates.json` contains all 17 complete exit-zero records and exact log hashes. Independent correction review remains pending (reviewer interrupted).

Parent `203b6180c244d467bba693d5f28d8f495a2024ec` integrates operation/Gerrit unit owner 918889d5. 9/10 source paths exact owner; activity/mod retains canonical workflow map/private binary exclusion. `gitcraft-root-op-status/source-integration.json`. Prior independent report `gitcraft-51-operation-gerrit-notice-review.{md,json}` found ONLY OP-NOTICE-1, other scoped source/control/witness checks pass. Actual CHECK invokes same validator once; only finite fields from the SAME successful log survive, with full final error equality. No dynamic source on cancellation/failed log reads; unknown provider details opaque. Raw auth extraction/APPROVED/FIX/EXPECT/undo/callback order retained. Activity receipt retires on mismatch/redaction/bounds/plain repeat. Owner check+13 exact inert tests passed.

Parent `af4dc2700e7455a04cfd68eea46a50574622ab47` closes WF C1/C2 after cbaf assembly. Exact all-target check +75 individual inert tests PASS (`gitcraft-root-workflow-r2/{gates,validation}.json`). Fresh independent STATIC PASS at `gitcraft-51-workflow-final-correction-review.{md,json}`; MD SHA256 0627c3aef3e142f213bc9c87173478af9575e362f0b152179d19babd2b4c5c98, JSON 03a6fed52ae803e678e9c2380ec9b3f723b86458293488bd2f21f0152eff63d2. All 773 old tests retained with only approved fixture/presentation adaptations. Nested worker channels remain OPEN/excluded, not an overall repaint acceptance.

`cbafbbac903456da94b69d5eb2eee51115fe2005` integrates owned workflow R1 6b13 (50 paths), preserving canonical Notice wrappers, Result-style deferred fixture, closed Agent/Ghost/DCO/package/review source. Root finite map adds 17 fixed sentinels +3 operation templates, excludes private binary warning. `gitcraft-root-workflow-r1/` contains source/map/validation proof.

`e4a884ee332e020fe49e9aca713c2f8e907cc318` integrates Home INT-FINAL-1 exact owner7518 eight paths. Root all-target+4 exact inert/catalog PASS and independent STATIC PASS `gitcraft-51-home-error-correction-review.*`. AppError keeps raw plus private source; unmarked equal strings stay opaque, current-locale Home paint; historical Toast delivery remains a snapshot.

Earlier accepted canonical source: c2da523e custom completed Notice (STATIC PASS), 692aa6da completed parallel Notice (STATIC PASS), 5544af0d common Notice transport (STATIC PASS), 7c513842 workflow compile reconciliation, 66eaec74 owned workflow383 migration, 3cf0803e generated Compare provenance, de8ca529 interaction932 migration, df33060b Recipe/Gerrit/Custom helpers+trust paint, bbde07f4 fork map, 10275336 execution witnesses, 3fce31af finite600 maps, a9c32d99 package/updater witnesses, 4417a3cc Ghost bridge, 5ec7a250 report/Previous paint, e585e595 remote composition, fb17281 guard consumers, da12461 typed compare captions. Do not overwrite those with entire stale agent branches. Full reports and source manifests are in the evidence backup and Git history.

## Remaining work, in order

1. Finish root-owned lint normalization. Start from current source, rerun qualified all-target Clippy (`-D warnings -A dead_code`) after inspecting incomplete log; no false strict pass. Root normal denied tests in repo/operations.rs need Result propagation. Remaining ~136 worker diagnostics include ~88 compatibility dead_code, 18 package witness result_large_err, three private large enums, bundle argument-count lint, and excluded paths. Prefer narrow mechanical fixes; no blanket lint suppression or speculative API framework. Keep raw producers/control/assertions intact. Read mechanical handoff before editing.
2. Fresh bounded independent review of 203..844 OP-NOTICE-1 correction using existing prior report and 17 exact gate logs. Keep generic provider details opaque. No re-review expansion or long-thread continuation.
3. Combined source check/fmt/appropriate audited inert tests on the final source. Review mechanical integration/preservation independently. Retain source/receipt hashes on immutable pushed candidates.
4. Root-only actual default local macOS suite after source freeze/push/review. PREPARED but NOT RUN: `gitcraft-final-combined/run-local-suite.py`. Inspect/update deleted paths first. It makes short physical private TMP, HOME/XDG/config, inspected sample-only Git templates; clears ambient GIT/GITCRAFT/SSH optins; sets installed CARGO_HOME/RUSTUP_HOME and RUSTUP_AUTO_INSTALL=0; CARGO_BUILD_JOBS=2. Optional git-lfs/filter-repo binaries were in deleted tmp tools and are NOT backed up; provenance JSON is backed up, reinstall them explicitly when needed. No host process selectors/watchdogs. Suite uses 2 threads and excludes ONLY separately run exact owned timeout case `parallel_bisect::tests::a_test_past_its_limit_is_stopped_and_skipped`; must run that separately and disclose exact evidence. No runtime currently running.
5. Triage actual failures, preserve first failures, finite correction tasks only. Initial root19 actual baseline was 3915 pass/78 fail/21 ignored/1 filtered, exit101; its logs/triage are backed up. Most failure groups received narrow source fixes and individual actual reruns. Full combined green has NEVER been claimed. Native macOS GUI/provider timing/Windows/Linux live acceptance remains held; no fabricated platform claim.
6. Final independent combined review, final feature history from fresh fork dev (94 commits), tree/source equivalence, exact authorship, local checks, then consolidated PR and only then final CI. No PR currently exists for this final work.

Known explicit scope limits: nested email/devcontainer/LocalActions worker String channels retain emission locale (see `gitcraft-workflows-r1/nested-worker-channels-OPEN.json`); do not claim those repaint. Native/source-exit/platform runtime held. Earlier accepted physical Darwin dispatch retains documented same-user administration replacement limitation. Common resolved-path I/O without producer context remains opaque, not guessed.

## Recovery commands

Use the retained `source/` directly, or clone:

    git clone --branch t3/issue51-resume-20261108 https://github.com/JKamsker/gitcraft.git gitcraft-resume

The recovery branch contains the resume guide and selected immutable closing receipts; application bytes equal 886b42b5. Full local evidence and 91-ref bundle are in this backup folder. To recover local branch refs in another repo, inspect `git bundle list-heads all-refs.bundle` and fetch named refs explicitly; don't overwrite unrelated current refs or force push old dev.

Before doing anything, verify origin URL, clean status and current SHA. Use tool calls with the retained checkout as explicit workdir: the old T3-bound cwd will cease to exist after the user clears it. Do not recreate the dozens of old agent worktrees/targets. Continue with one checkout and only fresh bounded agents if actually needed.
