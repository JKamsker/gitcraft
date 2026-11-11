# Final source freeze after archive-discovery correction

Before final validation, the recovery feature was rebuilt from verified base `c6d5a2bb8e4f933b5709f9b36e4e7b1e7a6a97a5`, retaining one commit per feature. No new dev source was merged. `final-freeze-provenance.json` records the preserved combined source and rewritten recovery/shard commits; previous proof files below remain historical records with their original bindings.

The recovery feature includes parent `51963cfc` production changes: archive discovery returns its Result to the actual RepoTab callback, preserving rows and showing an error notice on failure. Accepted tests `a9194e489382cd852dd72729a44129ee85305886` verify that callback submits no worker on Err and that an early merge restore retains a byte-exact archive row after completion. The former empty-list assertion contradicted retained archive discovery. No LFS production source changed; eleven former environment failures passed separately on the unchanged 81c5b89 binary under the prepared Git 2.56.1/LFS 3.8.1 environment. Those diagnostic passes do not validate this new candidate.

This document is committed before freeze and predicts no gate results. Fresh exact-candidate full static gates, complete/disjoint compiled inventory, contained runtime partitions and recovery recording evidence will be stored outside the tracked repository under `/tmp/gitcraft-final-validation-issue198`. Source remains frozen during these checks; failures require parent coordination and independent review. No pushes, CI or PR are authorized in this unit. Native Windows root cause and power-loss durability remain unproven. Independent adversarial review of the exact final candidate remains required before PR delivery.

---

# Current P2 final candidate

The two accepted final-review corrections are folded into the diff and recovery feature commits. `final-history-and-provenance.json` records the rewritten feature history, authors, exact scope blobs and original contributions. `final-correction-source-proof.json` records the exact nine-path application delta from e65. Source bytes equal accepted P2 commit `2ad1d508592607f69aa50cd00eb2305afc389a01` after excluding this evidence directory and the minimal compile fix in `src/repo/diff/view.rs` and lint fix in `src/repo/diff/presentation/layout_regressions.rs`. Full build of the exact accepted tree failed E0502 because a late `rows.layout_changed` read extended the immutable borrow across `self.find.saw`. The fix captures that boolean before the mutable borrow. The regression test now propagates a missing fixture row through its existing Result, replacing an unwrap denied by repository lint policy. The failed intermediate Clippy log is retained at `/tmp/gitcraft-validation-p2-before-lint-fix`; the exact extra delta is retained in `accepted-p2-to-compilable-source.patch`. Failed exact-tree build evidence is preserved outside the repository at `/tmp/gitcraft-validation-p2-exact-failed`. Policy and compiled-inventory scopes remain separate and unchanged. Local recovery refs preserve the prior history and the combined pre-fold candidate; nothing was pushed.

`p2-handoff.json` preserves the correction author's focused evidence and limits, including unchanged-offset geometry retargeting, stable archive lock re-resolution, negative controls and recording regressions. These results were not independently rerun by this assembly. Fresh final-commit fmt/build/clippy/test compilation, Python, cap and compiled inventory evidence is written outside the repository to `/tmp/gitcraft-validation-p2-final`. This committed document does not predict gate outcomes.

The original e65 source proof and application gate records remain valid historical evidence for their immutable commits. The new source differs in exactly the two correction scopes (nine files); those old gates are not full-candidate validation. Final reviews and contained runtime remain parent work. The read-only namespace-preparation agent may inspect the old precompiled e65 binary; these corrections do not change its namespace fixture.

The first e65 runtime run failed using Git 2.43. The parent reports that task-local Git 2.56.1 (`/tmp/gitcraft-git-env-e65df5b1/env.sh`) resolves all ten investigated assertions; one namespace GID prerequisite is being independently corrected. This is partial diagnosis, not full runtime success, and was not rerun here. No full runtime or process tests were executed during this final correction assembly.

Windows audit: the historical urgent-capacity defect does not explain failures observed before the urgent changes. Their root cause remains unproven. Serial/isolated fixture grouping retains coverage but does not prove a Windows fix; native Windows execution remains outstanding. Do not present the historical capacity defect as the cause of preurgent Windows failures.

The following original assembly report and its original JSON records are retained as historical evidence. Its original source/head identifiers and validation limits apply to that assembly only. The current delivery binding is in `delivery-proof.json`; the prior binding is preserved in `historical-delivery-proof.json`.

---

# Issue #198 final history assembly

This is a local source candidate, awaiting parent validation and independent adversarial review. It does not close #198 or claim #193–#195 complete.

## Immutable source and history

- Verified fork remote: `origin = https://github.com/JKamsker/gitcraft`.
- Fetched only `dev`: `c6d5a2bb8e4f933b5709f9b36e4e7b1e7a6a97a5` (PR #199), advanced from `a4d841713af3d31178a3b9212fd3c5adcb4172dc`.
- Preserved integration branch `t3/implement-issue-198` at `29d9695def41ccb2524b632bdfa334fb33470c92`, tree `0e6c1f6f8b5063f4cd76c052e857db9c9e822643`. Child refs are unchanged.
- Assembled source commit: `fc34a2bed9de2b60eb1a43ee1974b5907aaec773`, tree `8920fe858419ded92a023b1661475dea54882444`.
- `history-and-provenance.json` maps every final scope to its paths, original accepted commits, integrated commits and subsequent corrections. Fixes are folded into the feature snapshots, not replayed as repair commits. Policy/checker and compiled-shard validation are separate scopes.
- All new commits use Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>.

## Delivered slices and remaining work

- [#193](https://github.com/JKamsker/gitcraft/issues/193): shared global-option grammar, typed operation integration and immutable planning inputs, with command-context route snapshots. This is a delivered subset; broader plan migration and execution semantics still require parent validation.
- [#194](https://github.com/JKamsker/gitcraft/issues/194): private repository-tools/cochange owners, explicit effects and issue-work planning ownership. Other decompositions preserve existing central integration and do not prove full feature independence.
- [#195](https://github.com/JKamsker/gitcraft/issues/195): scoped identity, fail-closed exhaustion, context retirement (including A→B→A), stale-read rejection and preserved mutation settlement. Full application lifecycle runtime validation remains outstanding.
- Recovery: versioned journal transitions, restart guards, archive discovery and destination publication. Recording checks are retained as limited evidence; real filesystem/crash durability is not established.
- Diff: presentation and displayed-row ownership, fold/geometry invalidation, wrapped keyboard navigation and the concurrent #199 text/Git-gutter selection behavior are combined. Settings pages, push-preflight stages, app/protocol/feature owners and regression suites are decomposed; Windows fixture grouping retains coverage but does not establish a root-cause fix for Windows CI instability.

## Measurements and equality

All tracked Rust paths are counted, including tests and build scripts. Historical baseline: **1,456 files / 355 above 500**. Accepted integration: **2,581 / 0**. This source: **2,582 / 0**, maximum **500**. The extra file owns the shared pointer-test fixture extracted from #199's 523-line test module.

The unchanged-base whole-tree equality condition does not apply: dev advanced. The final source differs from the accepted integration only in four diff paths, listed in `source-proof.json` and captured byte-for-byte by `accepted-to-final-source.patch`. The changes preserve upstream's fixed unified horizontal gutter, upstream comments, its horizontal-marker regression, and extract the common fixture while retaining test selectors. The accepted presentation owner, fold invalidation and viewport geometry remain. Everything outside `src/repo/diff` is byte-identical to the accepted integration (`git diff --exit-code`, exit 0). Both parents' source was preserved through manual resolution of four merge conflicts; full combined source must be rebuilt.

## Gate evidence and limitations

Newly executed on the source commit above: `cargo fmt --check`; all **19** Python tests via `python3 -m unittest discover -s ci -p 'test_*.py' -v`; Rust size checker working tree, index and revision modes. JSON records and compressed logs bind the relevant checks to that immutable source. Intermediate history retains inherited cap violations until the decompositions finish; the final full-tree gates pass.

Earlier build/clippy/test-compilation, compiled test inventory (5,191 tests, six binaries, 132 migration mappings) and recovery recording logs are retained under `accepted-*` with their original manifest. They were **not rerun** here and are **not** full-candidate validation: application source changed in the merge. Earlier recovery evidence was originally rebound after a documentation-only change; that narrow original assertion remains recorded without extending it to the combined application.

No full builds, application runtime tests, owned-child/process-control tests, Windows execution, independent adversarial review, push, PR, or CI activity occurred during assembly. The parent must validate containment, synthetic/recording process gates, exact-candidate runtime and independent reviews before publishing; CI follows implementation/review. Windows validation and durability remain unestablished. Keep issues open for undelivered requirements.

The final delivery commit adds only this evidence directory. `delivery-proof.json` strips these additions from the staged tree and verifies exact equality with the immutable source tree above, rebinding only the newly run lightweight gates across the documentation-only addition. No new application test execution is claimed.
