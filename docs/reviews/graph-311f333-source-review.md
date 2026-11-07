# Graph source review — changes required

Reviewed immutable `311f333f3c4ddb63168fd921480a4f67eb87cd93` against
`ab68bdedd387236c03f26f839572860d405a2f7f`, covering #46 branch-model bands,
history minimap and postcards. This is an independent **source-only** review.
Direct Rust parsing succeeded. No compiler, application, native/export fixture,
audio, or CI execution occurred. This is not validation or a passing final review.

## Blockers

1. **P2 — An unchanged branch can acquire another branch's colour.**
   `src/repo/branch_bands.rs:90–106` sorts branches by band/name and lets the
   first traversal claim every unclaimed first-parent commit. Start with
   `feature/z` pointing to Z, then add `feature/a` whose first-parent history
   includes Z, with both in the feature band. On reload, the new `feature/a`
   traversal claims Z before `feature/z` is considered. Z's hint now contains
   the hash of `refs/heads/feature/a`, despite the existing branch remaining at Z.
   This violates the requested stable branch colours. Reserve actual ref-tip
   ownership before walking shared ancestry, and verify this same-band case;
   the current older-trunk test only covers different band priorities.

2. **P2 — Slash-containing remote names are parsed as branch prefixes.**
   `src/repo/branch_bands.rs:55–56` removes only the first component after
   `refs/remotes/`. With the supported remote name `my/fork`, the branch
   `my/fork/main` is classified using `fork/main` and goes into the feature
   band. Release/develop classification fails similarly. Use the actual remote
   identity already present in `Refs.remotes`, including its complete prefix,
   rather than splitting the full tracking name at its first slash.

3. **P2 — Closing/parking loses the owner of retained postcard disclosures.**
   `src/repo/postcards/mod.rs:151–165` sets `context = None` on any closed or
   parked poll, while retaining `commits` (and a parked preview). Close a
   loaded postcard, allow the closed poll, edit the same active profile's
   connections/Git configuration, then reopen. With the original owner erased,
   `profile_changed` is false; Compose reuses the old captured authors/mailmap
   result and records the new context at lines 273–276. No fresh range read is
   required. Keep dataset ownership independently of reader/open state, or clear
   cached disclosures whenever ownership is discarded. Cover closed/parked →
   changed context → reopen, in addition to an open-window profile change.

4. **P2 — New full-history analysis executes in the UI path.**
   `src/repo/branch_bands.rs:181–190` builds all hints and relayouts the entire
   loaded graph synchronously on a header interaction; `src/repo/load.rs:579`
   does the same during reload. `src/repo/graph_view/minimap.rs:42–102` scans
   projected history and all collapsed-span commits from the paint path. Saved
   Views marks the minimap dirty on selection-driven projection rebuilding
   (`src/repo/saved_views/mod.rs:238–241`). The 512-bin cap bounds painting and
   retained bin storage, but not this analysis work over up to 100,000 commits
   and their active edges. #46 explicitly asks that the analysis behind these
   views run off the UI thread. Prepare pinned results on an owned cancellable
   worker, reject obsolete generations, and show which loaded extent it covers.
   No latency measurement is claimed here.

## Compact notes

The range reader pins full object IDs, disables replacement/lazy fetching through
its accepted reader, caps history at 200 commits and applies mailmap. Snapshot
render input removes raw names/messages; the disclosure list and exported bytes
share a composition snapshot. The local export holds physical parent directories,
uses exclusive creation, preserves existing names, and reports that an I/O error
may leave its newly created file incomplete. Its documented cancellation boundary
is before publication; writes after exclusive creation are not transactional.

The renderer at this frozen commit also uses obsolete egui 0.36 APIs. The author
reported these already corrected in successor `dc91a04`, with further source
fixtures in `bfe75d631670513141bf1b86c62124b53c7d61ce`; those known fixes are not
counted among the four remaining findings above. Successors still require their
own exact-candidate compile, behavior checks and review.
