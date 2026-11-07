# History investigations (#36): preserved implementation contract

Base: accepted `b5df68070a6df9d91cd29ca5d6270db80c9040c9` (17 features, local866passed/3ignored). Worktree `/workspace/work/history-investigations`, branch `codex/history-investigations`. No unreviewed Content dependency. Status: Saved Views usable feature cut80b2e50 is locally validated (fmt, strict all-target Clippy,878passed/3ignored), independently review pending. Remaining three named ideas retain their full scope below.

Implement sequentially and preserve a meaningful usable, locally tested feature cut SHA after each item. Root owns the final history with one correctly authored commit per feature and the sole final PR. Every completed plan/source/fix unit is committed and pushed before the next unit/check; verify Jonas Kamsker <11245306+JKamsker@users.noreply.github.com> and the exact remote SHA. No CI during implementation or review.

## Saved views of the graph (M).

- [ ] **Saved views of the graph (M).** Presets such as these:
  - _My unpushed work_
  - _My recent changes and what they depend on_
  - _Descendants of this commit_
  - _Only merges into main_

  Long runs of history in between collapse into a connector you can expand. They pick commits by how they relate to each other, which hiding or soloing branches can't do. They would be built on git revision queries rather than a new query language.
  _Seen in:_ [Sapling ISL](https://sapling-scm.com/docs/addons/isl/), [jj revsets](https://docs.jj-vcs.dev/latest/revsets/), [jjui](https://github.com/idursun/jjui).

Acceptance:
- Saved Git revision-query presets: my unpushed work, my recent work+dependencies, descendants of selected commit and only merges into default branch.
- Persist saved views, execute queries in background and render selected commits with collapsible connector spans for omitted history.
- Expanding connector restores intervening commits; do not confuse branch hide/solo with relation-based filtering.

Dependencies and exclusions:
- Existing git log/rev-list and lane layout.
- Open #73 search operators and #84 graph order/first-parent are distinct and should compose.

## Line dossier: why is this line like this? (M–L).

- [ ] **Line dossier: why is this line like this? (M–L).** Right-click a line and pick _Explain history_. A card shows every version of the line, following it through moves and reformatting (`blame -w -C -C -M`, repeated on each commit's parent). Each version shows its commit and the pull request that brought it in. The card also shows **the review comments that were made on those lines in that pull request**, and the linked issue. An optional LLM summary, run on your own key or a local model, cites each card. gitcraft can offer to add formatting-only commits to `.git-blame-ignore-revs`.
  _Closest:_ GitLens line history and its pull-request hovers, which only work on GitHub and don't show review comments.

Acceptance:
- Explain history card for a selected line: follow previous versions using blame -w -C -C -M repeatedly through parents, moves/reformatting.
- Each version has commit, introducing PR, review comments on corresponding lines and linked issue; show uncertainty/stop boundaries.
- Optional local/own-key LLM summary with citations; offer formatting-commit additions to .git-blame-ignore-revs after preview.

Dependencies and exclusions:
- Existing blame/file history/forge threads; robust line anchor mapping and PR lookup.
- Open #110 history of selected lines is narrower and does not provide dossier/review comments.

## Symbol biography (L).

- [ ] **Symbol biography (L).** Pick a function and see its life on a timeline like a video editor's: when it was created, renamed, moved to another file, split and deleted. Scrubbing the timeline steps its body through each version, with the author and pull request of each step. A symbol is found by name with tree-sitter, and by how similar its body is across files linked by rename detection.
  _Closest:_ `git log -L :func:file`, which loses the function once it moves to another file.

Acceptance:
- Tree-sitter symbol selection/search and history timeline tracking creation, rename, file moves, split and deletion.
- Use rename detection plus body similarity to link versions across files; expose ambiguous matches as such.
- Scrub symbol body versions with author/commit/PR and revision-aware navigation.

Dependencies and exclusions:
- Language parsers/grammars, rename/body-similarity index, repository history cache and PR linkage.

## What usually comes with this? (M).

- [ ] **What usually comes with this? (M).** While you stage a change to the serializer, gitcraft notices: _changes like this usually also touch the compatibility fixtures, and those haven't changed_. A click shows the past commits behind the hint. It counts which files change together, leaving out bulk and generated commits. Every hint can be dismissed and shows its evidence.
  _Closest:_ [CodeScene's change coupling](https://codescene.io/docs/guides/technical/change-coupling.html), a hosted analysis that doesn't help while you stage.

Acceptance:
- Build evidence of file co-change from historical commits, excluding generated/bulk changes and bounded by age/count.
- While staging, suggest usually-coupled missing files and list exact supporting commits; threshold/sample counts visible.
- Dismiss per hint/pair, background indexing/cache invalidation; never imply a deterministic requirement from correlation.

Dependencies and exclusions:
- Git commit-path history scanner, generated/bulk filters, persisted dismissals.
- Open #128 generated-file patterns may supply reusable exclusions.

## Implementation order and boundaries

1. Saved graph views: typed Git revision presets, repository-scoped persistence and background immutable query results; a graph projection with explicit expandable connector spans for omitted runs. Preserve raw commit identities, selection, lane semantics and navigation across collapse/expand. Presets compose with branch visibility/search; do not implement PR73's search language or PR84's general ordering/first-parent toggle. Include actual graph/preset UI and connector interaction, not only a commit-set backend.
2. Line dossier: line-context entry point in existing file/diff views, repeated whitespace/copy/move-aware blame through pinned parents and explicit mapping/stop uncertainty; commit/introducing-PR/relevant-review-thread/linked-issue cards. Reuse existing forge/account transports with bounded provider enrichment; unsupported/unavailable mapping stays explicit, never label all unrelated PR comments as line-specific. Optional explicit local/OpenAI-compatible own-key summary validates card citations, and formatting-only ignore-revision changes require a reviewed preview, stale-file guards and preserved staged state. Do not duplicate PR110's selected-line log or revert UI.
3. Symbol biography: use real tree-sitter grammars (at least project Rust and common supported languages, with explicit unsupported-language behavior), immutable symbol bodies and bounded parsing/history. Link rename/body-similarity candidates across renamed files; show uncertainty and alternative/split candidates, creation/deletion boundaries, author and PR provenance. A real scrubber/body viewer navigates revision-aware files. Do not silently replace syntax parsing with a regex or claim a guessed identity as proven.
4. Change coupling: count file co-changes over a bounded pinned history snapshot, skip bulk and generated/lock/minified cases, expose thresholds, denominator/sample counts and exact evidence commits. While staging, suppress suggestions for files already changed anywhere in the current worktree, persist pair/hint dismissals and invalidate asynchronous caches on tip/status/profile changes. Hints are evidence of correlation and remain optional; no automatic staging/editing. PR128 owns generated-file folding/settings UI; reuse basic exclusions without implementing that separate feature.

## Shared safety and local evidence

Use accepted raw binary/UTF-8 readers, captured immutable Profile context and Practice-first Git routing, with no-lazy-fetch/no-replace and disabled automatic hooks/signing/filters as applicable. Pin refs to object IDs before ancestry reads; bounded subprocess input/output, commit/path/blob counts, parser depth/time, provider payloads and total jobs. Cancellation must invalidate the current reader immediately and keep at most one owned reader plus latest pending request until actual transport completion. No wall-clock deadline substitutes for underlying completion, and no new process controller or host PID controls are introduced.

All data/provider/Git work runs on background workers. Real Git fixtures cover merges, rewritten/moving refs, shallow/promisor repositories with no-fetch positive controls, Unicode/option-like paths, rename/copy/formatting/ambiguous/deleted/split histories, malformed/large/binary inputs, stale/canceled jobs, lifecycle/profile/Practice boundaries, persisted restarts and non-destructive optional file edits. Provider/LLM tests use synthetic payloads and recording-only adapters first; no real credentials or services. Actual usable pointer/keyboard UI flows and bounded rendering are checked. New process/protocol fixtures require independent static review before real execution.

Local standing-authorized gates use private targets, dependency hardlinks only, unique locks, private app clean, INCR0/jobs2 and unset optional SSH/WSL env. Count non-zombie Cargo chains only and avoid starting when three are active. Each final cut and whole-batch candidate needs fmt, strict all-target Clippy, full local tests, bound evidence and independent adversarial review. CI remains deferred until all implementation, final history assembly, local checks and independent reviews are complete.

Saved Views first registered unit: typed pinned Git queries, bounded latest-only reader with accepted actual-completion ownership, persisted presets and days/anchor/branch parameters, graph projection/click expansion, boundary lane identities (reused columns never fabricated as continuous), visible-only sizing, selection/keyboard raw identity, explicit bounded older-context pages with no automatic object downloads. Source regressions cover four real merge relations, moved refs, read-only state, shallow/promisor helper positive control, actual pointer choose/save/expand/collapse/all-history flow, persistence and synthetic delayed completion/Profile ownership. Unvalidated until private local gates.

Frozen Saved Views application: `80b2e50f069d8215b0801c7d674a03331da8d819`, base `b5df68070a6df9d91cd29ca5d6270db80c9040c9`. All12feature cases pass in full878passed/3ignored/0filtered881total. Frozen binary and complete source/log hash manifest are in `/workspace/work/saved-graph-views-validation.json` and docs/validation/saved-graph-views-80b2e50. Native remote/Windows/macOS deferred; no CI. Existing graph geometry tests retain checkout/context/tooltip assertions and now locate painted targets after the new chooser.

Line Dossier committed read-engine WIP: source `fa7efd9` compiled in its private target and all4focused cases passed (versions/rename/whitespace/refs-index-work preservation, ambiguous replacement/merge alternatives, malformed paths/IDs, binary/large/link/cancel). UI/provider/summary/mutable ignore-file flow is not yet registered; expected dead-code warnings until registration, no strict/full Dossier gate claimed. Next corrective unit adds verified partial-copy fallback, followed by central dossier controls and exact line-context entry points; mutable helper reuse waits for parent-published green Content integration.

Read-engine contract: committed immutable versions only; do not silently treat working/index line numbers as HEAD line numbers. Blame origin fields can anchor a working Blame row to its actual committed source. Historical File/diff rows use the exact displayed commit and line; uncommitted lines must offer a committed view or an explicit unavailable explanation. Parent maps use unique normalized text/context, expose ambiguity, and merges require an explicit parent choice. Summary and forge comments must retain exact revision/line mapping or label uncertainty; no unrelated PR comments may be called comments on this line.
