# Independent history content review R1 — FAIL

Immutable candidate **0de454f05f8fbdf932fc841b5d696bfc7d9b0cca**, base a850de26b0a0778ea96fe34cd1498e6e78928492. Worktree `/Users/jonas/.t3/worktrees/gitcraft/t3-4a6c0b9e`, branch `t3/history-investigation-views`. Scope is original issue36 items4,6,7,8: saved graph views, line dossier, symbol biography, cochange hints. Read original issue and `/tmp/gitcraft-51-audit-core.md`, handoff and exact gate records. Other six history/recovery features were not independently reviewed here. Candidate HEAD and clean worktree verified before and after; no tracked edits, CI, broad test suite or live forge/AI calls.

**Verdict: FAIL — five P2 findings.** Four are independently executed exact-source boundary reproductions; the saved-view/timeline finding is a source/control-flow reproduction, explicitly not an executed desktop interaction. These implementations are substantive and reachable; they cannot yet be accepted as complete because they can present incorrect creation/deletion/review evidence and stale path hints.

## P2-1 — An ordinary body edit becomes a false creation or deletion with complete-lineage status

Locations: `src/git/investigations/symbols.rs:295–318`, backwards539–552 and forwards614–621. Body similarity rejects a candidate even when its same path and qualified function name remain present; a rejected match is converted directly into “Created”/“Deleted” when the file-change set was within budget. Generic notes about heuristics do not explain this concrete break, and limited remains false.

Executed fixture: commit A introduces `a.rs: fn calc() -> i32 { 1 }`; commit B changes only1→2 in the same function. Tracing B/calc returns one event, **Created at B**, limited=false, omitting A. Tracing A/calc forwards returns Created at A and **Deleted at B**, limited=false, although calc still exists at B. Its three-token body has no shared trigram after this edit, so same-identity bonus cannot pass the20% filter. The same problem applies to larger complete rewrites, not just a synthetic ambiguous rename. This defeats the core function-lifetime requirement.

A=`b0ca83ab474e84c3735d1a99c7e4bb3a254a8948`; B=`13c6b52abecd54010a7ce7582ddb19c3993c096a`. Exact trace/log/fixture paths below. Positive control: exact line dossier traced both actual versions correctly in this same repository.

Correction: retain unambiguous same-path/same-qualified-name structural identity across body rewrites, or explicitly stop as uncertain/incomplete. Prove actual absence before asserting Created/Deleted; weak body similarity is not absence. Add backward and forward regressions for this one-line edit and full body rewrites, preserving ambiguity stops for truly duplicate identities.

## P2-2 — A shallow history boundary is presented as the function's proven creation

Location: `src/git/investigations/symbols.rs:576–596`, especially root detection582 and `Created`586. `show --format=%P` reports no traversable parents at a shallow boundary, and the code equates that with an actual root. Unlike saved views and dossier, this feature does not check shallow metadata or disclose this boundary; limited remains false if the available commits fit the requested depth.

Executed fixture: full source has function calc created at commit F and four later commits touching only another file. A local file:// clone with --depth=2 retains the latest two unrelated commits. Investigating HEAD/a.rs/calc reports **Created** at the older retained unrelated commit, with limited=false and no shallow warning. Actual creation is absent locally. This is an incorrect event/author/PR attribution, not just an incomplete scan.

Correction: distinguish genuine object-root commits from shallow cut points using repository shallow metadata or raw stored commit parents, without fetching. Render “Earlier shallow boundary (creation unproven)” and incomplete status. Add a real shallow-clone regression where the symbol predates the boundary.

## P2-3 — Gitea/Forgejo old-side comments are relabeled as exact head-side line evidence

Location: `src/forge/history.rs:598–618`, with proof labels475–489 and local proof fast path219–220. When position is0 and original_position is positive, the adapter uses **commit_id** plus original_position. Those positions address the old file side, whose ref is original_commit_id/OldRef, rather than that line number in the review head. The exact-head proof shortcut therefore accepts the wrong side without reading the old file at all.

Executed unchanged-parser fixture: comment has path a.rs, position0, original_position2, commit_id=H, original_commit_id=O, and refers to an old line removed/replaced in H. Query is H:a.rs:2. The adapter accepts the comment with proof **“Exact reviewed revision and line range”**. A dossier at a PR head can reach this path via commit→PR→reviews; line2 in H may have entirely different content. The existing candidate tests do not model old-side Gitea comments.

The API distinction is confirmed by primary upstream sources: [Gitea review-comment struct](https://raw.githubusercontent.com/go-gitea/gitea/main/modules/structs/pull_review.go) exposes LineNum/position, OldLineNum/original_position and OrigCommitID/original_commit_id; [Gitea converter](https://raw.githubusercontent.com/go-gitea/gitea/main/services/convert/pull_review.go) maps negative old-side lines to OldLineNum and OrigCommitID to comment.OldRef. [Official create-review documentation](https://docs.gitea.com/api/operations/repo-create-pull-review/) likewise distinguishes old and new file positions. These docs were read-only; no user account/API was contacted.

Correction: preserve side+reviewed ref as a pair; prove the old-side blob/line using the old ref, and leave comments unmapped when unavailable. Never reuse an old-side number in a head snapshot. Cover deletions/replacements, shifted current lines and unavailable old refs. Forgejo compatibility should be checked using its actual response contract rather than assuming a head-side fallback is safe.

Related coverage concern, not separately counted: GitHub LEFT handling542–546 uses current pr.base.sha as the diff's old revision. GitHub [documents three-dot PR diffs](https://docs.github.com/en/pull-requests/reference/branches), whose old revision is the merge base, which can differ after base-branch advancement. An exact-parser fixture also accepts such a base-head query with an “Exact” label. I did not prove that the commit-association endpoint returns the relevant PR for that query in a real account, so this remains a required adapter review/regression concern rather than a claimed end-to-end GitHub reproduction.

## P2-4 — Cochange suggests intentionally deleted companion paths as missing current work

Locations: `src/git/investigations/cochange.rs:151–184,194–238`; `src/repo/cochanges.rs:62–75,92–93`. Historical paths are filtered for generated/bulk status but never checked against the pinned HEAD tree. Rank only excludes paths currently changed. A historical deletion or rename leaves an obsolete companion in the index indefinitely, until it falls out of the600-commit sample.

Executed fixture: three commits change serializer.rs and compat.fixture together; a fourth commit deliberately deletes compat.fixture. Current HEAD has no compat.fixture (`ls-tree HEAD -- compat.fixture` empty); the working index has only serializer.rs staged. Exact load/rank still emits compat.fixture with3/3,100% evidence. UI consequently claims that obsolete file “has no working or staged change” as a missing companion. The past association is real, but does not establish a current companion path. Renamed paths can produce the same stale-name suggestion.

Correction: intersect candidate companions with eligible files in the same pinned HEAD tree, or explicitly follow proven rename identities to current paths. Preserve exact evidence and mark unresolvable/deleted historical paths as historical context, not actionable missing-file hints. Add deletion and rename regressions, plus current-file positive evidence.

## P2-5 — Undo timeline displays raw rows but keyboard navigation and scroll use the collapsed saved-view projection

Locations: `src/repo/graph_view/mod.rs:93–95,165–175,204–207,218–222,438–444`. Timeline mode intentionally suppresses the saved-view header and renders the raw graph (`filtered = active && !ghost && timeline_view.is_none()`). Arrow-key navigation instead checks only active, and scroll_to always calls saved_views.position. Ghost keyboard handling is correctly disabled; this finding is specifically the reachable Undo timeline mode.

Concrete source/control-flow reproduction: choose a saved relation whose12 raw rows0..11 collapse to visible matches0 and11 plus an intervening connector; keep row0 selected. Open the Undo timeline. All raw rows are now visible. Press Down: saved_views.next(0,true) returns11, skipping adjacent visible row1. Its raw11 scroll request is then mapped to projected2 (or1 for a trailing connector view), so scrolling also targets the wrong displayed position. If no next matched row exists, Down does nothing despite more visible raw rows. `toggle_timeline` does not reset the active preset. The existing projection/UI test demonstrates the underlying raw/expanded mapping but does not exercise keyboard interaction with timeline mode.

Correction: use one shared “projection currently displayed” condition for rendering, navigation, focus and scrolling. Use raw indices whenever timeline/ghost modes display raw rows. Add an egui key-navigation regression opening timeline with an active collapsed view and verify both selected adjacent raw row and scroll target. I did not execute this full UI sequence; the defect is established by differing branch conditions and exact projection next/position semantics.

## Whole-feature coverage and reviewed safety/resource boundaries

**Saved graph views:** real selector in graph header; four requested relation presets, named save/delete,32-preset persisted repository preferences and restart normalization; full immutable Git revisions rather than a new query language; exact author filtering; ancestry/descendant/first-parent merge queries; expandable long runs preserve raw IDs and lane origins, keeping selection/WIP/stash context. Input tips and selections capped30000 with explicit partial state; no lazy downloads; older context uses pinned tips and bounded pages. Shared retained Reader cancels superseded generations and keeps running ownership until completion. Projection lane arrays cap24, matching the existing displayed graph lane cap; I did not flag it as a new truncation regression. Finding5 prevents accepting all navigation mode claims. No other demonstrated saved-query/connector correctness bug was found.

**Line dossier:** file, blame and committed-diff line menus actually create pinned Seeds (`file_view/mod.rs:145–165`, `file_view/blame.rs:223–243`, `diff/select.rs:188–215`); Recovery supplies manual entry. Working/index line numbers are not silently reused as HEAD. Repeated blame -w -C -C -M uses raw pinned regular blobs, records actual versions separately from semantic attribution, follows rename/copy history, and stops with explicit ambiguous replacement/merge-parent choices. Source limits2MiB/file,32MiB cached blobs,20000 lines,128 steps,120s, continued display256 versions/32MiB. Existing file/commit buttons keep pinned source coordinates. Forge association, issue links and line-specific proof are substantive, but finding3 breaks correctness of one advertised forge's evidence.

Optional dossier AI is explicit, uses the configured tool and sends labelled Git-history cards; includes untrusted-data framing and requires every card label in returned output. It does not enforce claim-by-claim grounding, and I did not call any model. Format-ignore offers the full patch and acknowledgement, distinguishes -w from semantic proof (including strings), requires single-parent regular-file whitespace-only changes, revalidates the exact snapshot/root, appends through held identity/create-only guards, refuses links/hardlinks/special files and preserves staged state. File limit64KiB and patch1MiB. I read this implementation, but did not execute new ignore-file mutations or certify races/platforms beyond writer evidence.

**Symbol biography:** source-line and manual Recovery paths use real bundled tree-sitter parsers for Rust/Python/JS/TS/TSX/Go/C/C++; optional-feature-disabled builds explicitly refuse. Structural bodies, names/scopes, rename/copy change sets, similarity, possible-split evidence, author/commit/PR and body scrubbing are implemented. Known first-parent/similarity uncertainty is disclosed. Limits16–2048 revisions,1500 file versions,2MiB/file,64MiB raw source,10000 candidates, bounded per-file parsing/node/token counts and120s are real; display clips oversized lines with pinned-source access. Findings1/2 specifically violate the promise that unproved creation/deletion or truncation remains explicit, despite those positive bounds. General semantic continuity, arbitrary branch merges and external language runtime semantics are not proved by tree-sitter.

**Cochange:** reachable while staging from Changes; one pinned HEAD read cached across staging changes, no forge/AI, bulk>20 files and generated subjects/paths/attributes excluded, at least3 shared changes and60% source frequency, top20 hints from up to200 staged sources, concrete commit evidence/dismiss/restore. Current staged/unstaged/conflicted paths omitted. Latest-only reader handles supersession and tab lifetime; hidden/asleep tabs cancel work. Finding4 concerns pinned-tree file existence, which neither the cache key nor reranking supplies. No timing/UI benchmark or external repository-scale run was executed.

**Repository/profile/ref binding:** Git investigation Reader uses pinned IDs, --no-lazy-fetch/--no-replace-objects and draft_bytes' bounded accepted transport; avoids filters/textconv/hook execution on history reads. UI Reader tokens suppress superseded jobs. Opening profiles rebuilds/cancels tab scopes; in-profile author changes dirty saved-view scope. Forge lookup validates captured host/project against repository config and bounds requests48/90s, comment paging5, local proofs64 and HTTP bodies2MiB. I found no additional demonstrated cross-profile/stale callback defect in these four paths. These are source assessments, not live SSH/WSL/account-switch validation.

## Independent evidence and runtime limitations

Harness `/tmp/gitcraft-history-r1-harness/main.rs` includes exact original cochange.rs/dossier.rs/views.rs by absolute path. symbols.rs and symbols/syntax.rs are byte-identical candidate copies arranged under normal Rust module paths (verified equality); this avoids #[path] submodule-resolution differences. forge_history.rs is the entire unchanged candidate source plus a small appended wrapper calling private comment parsers. Stubs for unused forge HTTP/CLI functions return errors; no service can be contacted. Local Reader adapter runs installed Git with pinned fixture inputs, no-lazy/no-replace and cleared repository redirects. It does not model process supervision, transport overflow, remote shells or asynchronous UI. No production investigation, ranking, matching or comment-anchor algorithm was edited/copied into a substitute implementation.

Built once plus harness-only wrapper refinements with rustc1.95 and candidate's existing serde/tree-sitter artifacts, not an application build or repetition of the156-test gate. Independent output is `/tmp/gitcraft-history-content-r1.log`; parser output `/tmp/gitcraft-history-content-r1-comments.log`; disposable repo/OID metadata `/tmp/gitcraft-history-content-r1-fixtures.json`. Binary `/tmp/gitcraft-history-content-r1` supports symbol/dossier/cochange mode followed by repo,commit,path,name-or-line; comments mode takes a dummy repository path and executes synthetic adapter fixtures. Build requires --cfg feature="symbol-history", -L dependency=target/debug/deps and --extern serde,serde_json,tree_sitter plus the seven candidate language grammar rlibs.

Executed only safe local Git/tree-sitter and synthetic comment parsing. No full desktop session, native Windows, Linux, SSH/WSL, live forge, AI, unsafe process-control path, broad test suite or CI was run. Finding5 and GitHub-related concern were not end-to-end executed. Writer156 distinct targeted tests, draft repeat20, fmt/build/check/clippy remain writer evidence, read from `/private/tmp/gitcraft-history-gate-0de454f05f8f/results.json` and handoff; not claimed as independently rerun. A correction needs regressions and independent re-review bound to its new immutable candidate.

Artifact SHA256 values:
- `/tmp/gitcraft-history-r1-harness/main.rs`: `74fb47dcc3c7e31f58f0699176907c0a517ff445524f7cc9ee51a7173d71b357`
- `/tmp/gitcraft-history-r1-harness/symbols.rs`: `9a2b6ed00dfc954cdc6fcec5ec1e62e5647c1467ed2299ee36590865cdc567ad`
- `/tmp/gitcraft-history-r1-harness/symbols/syntax.rs`: `76b0e8078e963dddbc3043c447e6f383dd47228ec99a0731253d2637dc35f1ea`
- `/tmp/gitcraft-history-r1-harness/forge_history.rs`: `b4ceb6b6dbe129ee0e75e25de5c5d8296d1d086ffac576443751443c1e4292e3`
- `/tmp/gitcraft-history-content-r1`: `e807f7c5669be8762c2bd556416be03949ef70befd5884825149c10668da2150`
- `/tmp/gitcraft-history-content-r1.log`: `997e2b30ff47fd93c1a36a0090faeed83fa35539e2045043f1fa9163d6f932ac`
- `/tmp/gitcraft-history-content-r1-comments.log`: `50beef5e4d3c42ef241767e9857c15e8fb952eee233ab24a08f3c41ef2e8324c`

Fixture metadata:
```json
{
  "rewrite": {
    "repo": "/var/folders/0z/snd05v_n21jd6pkhvg2h0vlc0000gp/T/gitcraft-history-content-r1-akj7__9y/rewrite",
    "a": "b0ca83ab474e84c3735d1a99c7e4bb3a254a8948",
    "b": "13c6b52abecd54010a7ce7582ddb19c3993c096a"
  },
  "cochange": {
    "repo": "/var/folders/0z/snd05v_n21jd6pkhvg2h0vlc0000gp/T/gitcraft-history-content-r1-akj7__9y/cochange",
    "head": "1456041eed5cbdcc68d6ae255ccd9854b9e11100"
  },
  "shallow": {
    "repo": "/var/folders/0z/snd05v_n21jd6pkhvg2h0vlc0000gp/T/gitcraft-history-content-r1-akj7__9y/shallow",
    "head": "06ad83ae9d06340ae25d72ed201794893d93b14d",
    "actual_creation": "2d181da01b7f9da86347304b49b7076506ffffee"
  }
}
```
