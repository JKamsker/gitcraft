# PR #224 dev integration after Windows CI

Committed architecture candidate: `c828d39e2521334cea924bf06a054390e54433d9`. Includes a plain merge of dev
`d3a759bd4daba1d4120f61ec8bc246636cf39cd8` (#236, #237, #238 and #241).
This evidence commit adds only this directory; application/policy source identity
is verified against the architecture candidate.

CI run 43672223776 on previous head ae6712a99941e18df4dcd5ebb14e5a07f5ff6705
passed every job, including Windows git-rest, repo-a-c and repo-q-r. That confirms
the Windows cleanup identity fix; the unrelated resolver failure did not recur.
The attached previous-ci.json records the completed run and jobs. This is prior-head
CI evidence, not a claim that the integrated candidate has passed CI yet.

New local checks on committed HEAD:
- Low-priority `cargo +stable run --release --locked --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD --json <owned-target>/architecture-final.json`: 0 violations.
- `python3 ci/arch_baseline.py --base origin/dev --revision HEAD`: passed.
- `python3 ci/arch_baseline.py --base ae6712a99941e18df4dcd5ebb14e5a07f5ff6705 --revision HEAD`: passed.
- All declarations published on dev preserved unchanged; three new PR224 entries.
- `cargo +stable fmt --check` and staged Rust file-size check: passed, 0 size errors.

Merged measurements: git exported 985 (within dev's corrected 1019 budget), settings
symbols 181, repo-core symbols 610, sidebar exported 37. The latter exposes the
WorktreeCleanup state contract through branch_cleanup for RepoTab. The three actual
budget increases have scoped reasons, ownership and remediation in increases.json.
The old git/exported=385 ceiling is obsolete with the corrected visibility checker;
no git budget growth beyond current dev is necessary. Dev's published git/exported=1019
declaration is preserved and covers the prior-push comparison.

No local application tests or application Clippy rerun is claimed for this integration;
latest work is a plain dev merge plus budget/declaration/evidence updates. Earlier
source-bound local checks remain in ../pr224-windows-path; final-head CI remains required.
All target/temp files used the owned /home/jonas/work/gitcraft-targets/224 directory,
removed after delivery. No full/Windows/forge-server suite ran locally.
