# Review-workflows content review R1 — FAIL

Immutable candidate `bf6089b684597d878d3c2f256b7c69d64c6785a0`, base `a850de26b0a0778ea96fe34cd1498e6e78928492`; checkout `/Users/jonas/.t3/worktrees/gitcraft/t3-940761ce`, branch t3/implement-review-workflow-ideas. HEAD matched, worktree remained clean. Read AGENTS.md, original issue35 and `/tmp/gitcraft-51-review-implementation-handoff.md`.

Independent scope: comment relocation across rewrites, review freshness and Rust dependency evidence, reviewer echo across push paths. Other reviewers own the other seven ideas. Read-only: no tracked edits, new commits/pushes, forge/model calls, CI/PR, full suite, or unsafe process selectors/control tests. No writer gate duplication. New small source harnesses cover cases absent from the writer's81 tests.

## Blocking findings

### 1. [P2] A relocated multiline comment loses its range, permitting false relocation after a later interior edit

`src/repo/review_workflow/anchors.rs:163–169` persists a successful relocation with `start: None`, regardless of the original start_line. The first relocation correctly verifies every line in the original range (`relocate`, lines99–109); later refreshes verify only its endpoint. Consequently a multiline comment can be labeled “unchanged text” after part of the originally commented range changes.

Independent exact-source reproduction: `/tmp/gitcraft-workflow-content-r1.rs` includes the candidate anchors.rs unchanged. Its store adapter serializes/deserializes the actual Snapshot JSON in memory and Git adapter runs local Git; it does not mock the relocation algorithm. Fixture immutable heads are recorded in `/tmp/gitcraft-workflow-content-r1-fixture.json`. Capture a published comment over original lines2..3 (`two`, `three`) at A. Rewrite B inserts one prefix line, correctly relocating the comment to endpoint4. Rewrite C changes mapped interior line3 from `two` to `CHANGED INTERIOR`, keeping endpoint4 (`three`) unchanged. The candidate again returns a successful relocation to4:

```
A relocated={}
B relocated={"thread": LineTarget { path: "comment.rs", line: 4, side: Right }}
C relocated={"thread": LineTarget { path: "comment.rs", line: 4, side: Right }}
```

Expected C: remain outdated; a portion of the anchor's reviewed range changed. Preserve the mapped start in persisted Anchor (and keep verifying its full span) rather than reducing range evidence to one endpoint. Regression must include at least two successive rewrites and a restart/serialization boundary. Existing single-line insertion/rename tests do not cover this.

### 2. [P2] Prior-head relocation coordinates remain usable while rebuilding or after failure on a new PR head

`src/repo/pr_view/mod.rs:202` installs new PrFiles, then queues asynchronous relocation; neither load_pr, pr_details_arrived nor pr_files_arrived clears `view.relocated` or binds it to base/head. The completion at lines217–221 replaces it only on Ok; Err merely writes a status message, leaving old coordinates indefinitely. Meanwhile `src/repo/pr_review/send.rs:38–51` verifies the **new** files/head and then applies the unbound old relocation map. The PR page likewise advertises its stale entries as “unchanged text.”

Concrete reproduction from source control flow: establish a relocated endpoint at head B, refresh to C that inserts lines before it, and delay the relocation worker. Once C's files arrive, open C's patch before the worker returns: B's endpoint is still marked and its original thread is shown at that wrong C line. Make the captured prior snapshot head unavailable/corrupt, or otherwise have relocate_threads return Err; the wrong marker survives after the error. The request guard correctly rejects late **callbacks**, but does not prevent retained old **state** being rendered against the current head. This is a source/control-flow finding; no live forge or injected app callback test was executed.

Clear relocation state immediately when its source/head/files generation changes, and store/check immutable base/head/request binding with any accepted map. On error keep comments outdated and display an unavailable-map status. Add delayed and failing-new-generation UI/state tests that assert no old marker or reply target is rendered on C.

### 3. [P2] Reviewer echo subtracts a fetch-server tip from a different push server and auto-continues with a false empty check

`src/repo/review_workflow/echo_push.rs:290–307` resolves the base as `refs/remotes/{remote}/{destination}`. It never checks whether the configured effective push destinations match the fetch destination or whether remote.fetch maps that ref to the chosen destination. A remote with a separate pushurl can therefore reuse an unrelated server's cached tip as its baseline. Hashing configuration in stamp notices *changes* but does not establish that a stable cached tip belongs to the push server. `echo.rs:358` auto-resumes when the resulting hits are empty.

Independent exact-source/local-Git reproduction: the same harness includes candidate echo_push.rs unchanged. Fixture has two actual local bare repositories, fetch-server and push-server. origin fetches fetch-server; remote.origin.pushurl names push-server. Local HEAD D and cached refs/remotes/origin/main both point to D. fetch-server/main is D; push-server/main is C, which lacks handler.rs. Source plan is ordinary `push origin HEAD:refs/heads/main`. Candidate returns pair(D,D), and its computed diff has0 bytes. The actual C→D outgoing diff contains:

```
+fn handler(){ value.unwrap(); }
```

An enabled cited `\.unwrap\(\)` rule gets no hit despite newly publishing exactly the violation. No actual push or external server was contacted. Full immutable IDs, harness output and actual local destination diff are in `/tmp/gitcraft-workflow-content-r1-fixture.json` and `/tmp/gitcraft-workflow-content-r1.log`.

Resolve the effective push destinations and validated tracking mapping. If their actual baseline cannot be established without a live query, use a conservative empty-tree scan or an explicitly unchecked/manual-review result rather than claiming an automatic clean diff. Multiple pushurls need independent coverage. Add split fetch/push and custom-fetch mapping regressions. Existing routing test only verifies that a changed pushurl changes the stamp, which cannot detect this stable misbinding.

### 4. [P2] Echo's final revalidation occurs before a queued UI callback, but the resumed push still resolves mutable refs

`src/repo/review_workflow/echo.rs:374–382` checks push_stamp in a background task, returns the original PendingOp, then a later UI callback sets passed and calls run_ops_ex. That passed token immediately bypasses the preflight (`echo.rs:331–332`). Normal Push still contains `push --porcelain` (`src/repo/push/mod.rs:163`) and explicit branch push retains mutable source refspecs; unlike force_push_ops, these operations have no expected-source-ref step. Nothing rebinds the pushed source to the inspected immutable source IDs or validates the stamp after the callback queue delay.

Concrete timing reproduction: echo's final background stamp succeeds at source D; before its Msg::Then is drained, an external Git process advances the local source ref/HEAD to E containing a violation. Drain the callback. The original operation is exempted by passed and Git reads/pushes E, while only D was checked. This does not require bypassing a force lease or changing the remote; a delayed UI frame creates an arbitrary callback window. This is established by source ordering and original command shape; no actual push/race runner was executed.

Bind accepted evidence to the exact pushed source OIDs and routing, and carry guards into the execution transaction. Prefer immutable source refspecs where possible; a UI-only earlier hash is not execution binding. Add a deterministic test that moves the source after the background result is ready but before the resume callback executes, and asserts rejection/recheck or that only the inspected OID can be published. Preserve the existing force/remote guard semantics when rewriting any arguments.

## Additional confirmed hardening gap (reachability limitation retained)

The resolver rejects pre-command `-C` but `push_at` skips arbitrary other dash-prefixed words and sources ignores them (`echo_push.rs:6–13,105–129`). Exact source experiment accepts `--git-dir=/tmp/other.git push origin HEAD:refs/heads/main` and analyzes the current RepoTab repository, not that Git directory; `-c core.worktree=...`, namespace/config-env and equivalent context-changing prefixes also need explicit handling. The current normal Push controls do not emit --git-dir, and I did not demonstrate a current user-facing generator for that prefixed PendingOp, so I do not use this helper-only case as a separate blocking user-path finding. Reject unknown/context-changing Git prefixes conservatively before advertising a generic safe resolver contract. Arbitrary shell/terminal custom-command pushes likewise bypass this Git-argv interception; avoid claiming they were covered.

## Requirement coverage and assessed limits

- Since-last-review still uses the preexisting reviewed refs/range-diff banner. New earlier published thread anchors are persisted per PR URL+number and can follow literal unchanged spans and Git-detected renames. Pending forge review anchors are skipped. Relocated threads display in gutter/panel and replies use the original thread ID; relocation does not authorize rewriting remote anchors or applying suggestions. New refresh callbacks have a PR request guard. These are reachable substantive features, but range persistence and current-head state binding fail as above.
- Review freshness is reachable within PR page scroll. It records original base/head, head blob and patch fingerprints and explicit watched-path blob witnesses, retains source commits under freshness-input refs, distinguishes unreviewed/direct/dependency/unchanged text, clears rows when a new query starts, and guards async results with PR request+freshness request+number. Mark validates displayed base/head before dispatch and writes evidence to the captured original PR, not whichever PR later opens. I found no additional demonstrated false-green receipt/callback bug in this pass.
- Rust dependency analysis is real syntax traversal using syn/quote, not a similarly named empty module. It indexes conventional Rust modules, imported/static calls and direct/transitive call edges; compares function tokens and changed call target sets. Duplicate identities, dynamic/method/external/local-import/macro calls, parser errors, cfg/custom-path/re-export/type resolution are explicitly displayed as unchecked. At most4096 Rust files and32MiB captured sources,8 distinct receipt indexes and1024 symbols per traversal are analyzed; other languages rely on explicit watched paths. “Unchanged since reviewed” means exact text/patch evidence, not semantic correctness, and the panel explains dependency limits. Lack of LSP or full Rust semantics is therefore an honest bounded first-language limitation, not itself a blocker under original issue35's staged language-analysis request. I did not execute repository Rust code or claim proof of semantic equivalence.
- Reviewer echo is opt-in, imports only current authenticated viewer's published nonpending thread comments with URLs from an explicitly opened supported-forge PR, and invokes only the selected trusted AI tool on explicit Suggest action. Inferred regex rules require an imported citation and start disabled. Enabled checks apply to added lines and retain original comment links; quoted/header-like source lines receive correct path treatment. Collection across opened PRs can accumulate a cross-forge personal book, but it is not an automatic full-account historical crawl. This collection limit is disclosed in handoff; retain it in final completeness wording.
- Shared run_ops_ex intercepts the normal Push/Sync/branch/Gerrit/force/retry operation plans while retaining original ops/kind/quiet/refresh. Unknown post-push options and compound mutation-before-push plans become explicit advisory/manual review, with no automatic clean claim. A cited result or human continuation still checks a fresh refs/config/rulebook stamp; current normal implementation is materially broader than one toolbar handler. Findings3/4 show why those checks are not yet sufficient source/destination binding.
- AI/import generic callbacks use request tokens; enabled rules and persisted rulebook are included in push stamps. Rulebook storage is global to this app's repositories rather than per-account/project; collection explicitly proves comment authorship at import. No real AI, forge account switch or external publication was exercised by this review. No new process-control logic was invoked.

## Independent evidence and honest validation boundary

New harness: `/tmp/gitcraft-workflow-content-r1.rs`; output `/tmp/gitcraft-workflow-content-r1.log`; immutable fixture metadata `/tmp/gitcraft-workflow-content-r1-fixture.json`. It includes the exact candidate anchors.rs and echo_push.rs; adapters supply local Git transport, a serialization-equivalent in-memory snapshot store, and an unused compound-operation classifier. New scenarios justify this small standalone compilation; it is not a rebuild of the application or duplication of the existing81-test suite. Compiled with Rust1.95.0 and existing candidate dependency artifacts, then invoked with fixture repo,A,B,C. No network, actual push, host selectors or remote cancellation.

Writer's exact final test log `/tmp/gitcraft-51-review-final-tests.log` reports81 passed and identifies `target/debug/deps/gitcraft-173ed11b373fb184`; I inspected the relevant source/tests/log but did not rerun that suite or claim its gate independently executed. Existing tests cover a single-line insertion/rename, exact watched-path receipts, Rust call changes, and broad push-option mapping; none demonstrate the blocking cases above. Existing final fmt/clippy/check remain writer evidence only. This report does not claim a full combined gate or review of unrelated batch features.

The stale-map and final-callback findings are source/control-flow reproductions with precise timing, not executed end-to-end forge/UI experiments. The two anchor/misrouting experiments are executed exact-source boundary tests. Corrections need immutable-candidate regression coverage and independent re-review before acceptance.

Reviewed existing test executable SHA256: `c4dadb23e9102a4b4a69e8f5d3d4988764b603391afaca452637e753e25f7b0f`.

Fixture immutable IDs: {"a": "4410e41ffa753832151530eaffc15244f820b1b6", "b": "6069316dcfb0414289a67687dd4bdce90deec9f3", "c": "a1c51858bc6fa9b22f1d52bdfc4687c59110ecdc", "d": "12e2c576976ae53f0e31c89c61e6fdb1c9001743"}.
