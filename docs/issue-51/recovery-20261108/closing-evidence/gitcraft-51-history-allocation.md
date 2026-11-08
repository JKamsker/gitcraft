# Final history allocation draft

**Read-only preparation complete; final assembly and acceptance remain pending.** This preserves all93 original feature IDs plus strictly process-only F000, in the existing94-unit order. No commits, checkouts, fetches, builds, tests, native/application fixtures or CI ran.

Inputs: dev `cea7dfb5ddcc290352958b63566cfd3e515a0d9f`, immutable combined partial-Fluent root `9cd719ad11dd3b205fa17118837ae0e2e6132de7`, and pre-Fluent checkpoint `4c7d23b43152809acb0fccf72ac4709f713497ae`. All158 original-unit commit objects resolved. The parent must reverify fresh dev and freeze the final accepted Fluent tree before assembly.

Exact `--no-renames` accounting is **1334 paths**, not1333: the previous rename-aware draft omitted the deleted `src/forge/gitlab.rs` side of the GitLab module migration (#43.3). Current path proposals: **1041 unique**, **283 shared**, **10 unassigned general provenance documents**. Unique means a proposal about core ownership, not permission to stage a final whole file across later contributions.

The ledger partitions the exact dev→root diff into **5445 net hunks**. **4250** have one proposed edit owner; **1195** require feature/translation separation, with old/new line ownership runs and chronological evidence. The24-stage ledger contains8462 chronological hunks (successive edits can touch the same line), of which899 remain without one proposed owner. Chronological counts are evidence, not additional final edits.

An inert line-content replay of all24 stage patches reconstructs final source lines for **1334/1334 paths**, with zero mismatches. Root/base/checkpoint Git blob IDs and file modes remain in the path ledger. Replay compares split source lines; exact line terminators and modes remain authoritative in the recorded Git blobs. This proves patch-input completeness and line lineage, not byte-for-byte94-commit write-tree equivalence or implementation acceptance.

## Concrete assembly inputs

- `gitcraft-51-history-allocation.json`: full path records,158 original-unit ownership bindings, original same-path edit-line votes,24 exact chronological patches,5445 net hunk records and12339 wiring-line proposals. Proposals never silently resolve ambiguous shared source.
- `gitcraft-51-history-allocation-paths.tsv` and `-hunks.tsv`: compact flat indices. Each net hunk has source coordinates and edit hash; full JSON adds line-owner runs.
- `gitcraft-51-history-allocation-components.json`: all94 ordered units with proposed paths/hunks,28 source-bound component split instructions, direct Cargo ownership and exact final lock package candidates. Calendar statistics46.6 and file matrix50.5 remain distinct even where original commit provenance is shared.
- `gitcraft-51-history-allocation-seams.json` and `-unresolved.json`: finite remaining surfaces and explicit ambiguous records.
- `gitcraft-51-history-allocation.py`, `-components.py`, `-report.py`: reproducible local Git-object parsing only. All outputs are plain text/JSON under/tmp, with SHA256 manifest; no evidence archives or repository mutations.

## Remaining shared surfaces

| Surface | Exact split instruction |
|---|---|
| Generic registries and lifecycle | Feature-owned declaration/import/field/enum/match branch travels with its implementation. Use 12,339 module-line proposals as evidence; ambiguous syntax and common helper blocks remain explicit. Manual descriptions follow actual feature; Fluent conversion47.15 last. |
| Commands, accounts and owned execution | 43.5 owns captured environment/account/credential and effective endpoint policy; F000 only owner/lifecycle transfers; 35.5 owns frozen CommandSource and echo bridge; 49.1 adds metadata captured-status reads;32.8 owns reviewed APPROVED/EXPECT preflight. bounded.rs captured metadata is NOT assigned F000. |
| Push / routes / DCO / consumers | 32.8 owns reviewed publication/private aliases/guards and final DCO retention-join corrections, including APPROVED recognition in running.rs.35.5 adds source binding;49.8/49.9 durable consumer adapters;49.1/49.4/49.7 route/group/dependency adapters. Do not overwrite latest guards from initial units. |
| Native publisher | 32.14 owns authoritative publisher/capture/restore;33.14 changelist,33.15 virtual and48.1 turn adapters remain distinct. File-comparison index lens50.1 and standalone44.1 own their wrappers; lower primitive fixes remain32.14 where shared. |
| Activity calendar versus contributor statistics | 46.3 introduces history/date/mailmap, common immutable snapshot/cancellation and calendar.46.6 introduces Contributor/AuthorTotals/summary.contributors, authors aggregation/sort, Contributors variant, selector and report view. Exact anchors and 10 split instructions in components JSON. Shared fd080582 provenance is not permission to squash both. |
| Matrix / migration / lens / storyboard | 50.5 owns common Entry/Version/readers and Column/Matrix/matrix;40.8 migration + comparison_move;50.1 lens/transaction + comparison_pick/normal_patch/boundary buttons;35.7 owns ReadOnlyComparison storyboard adapter. Split Page/Data/load/selector branches. Original479f9655 spans three features. |
| Graph / accessibility / windows | 36.4 projection;33.5 dependencies;46.1 bands;46.2 minimap/shared Arcs;39.4 narration/audio;38.14 motion;46.7 celebrations;46.8 postcard workers;39.1 focus;39.3 labels/roles;38.12 resolver;38.5 effective hints;38.8 keyboard staging;38.13 window-owned forms/focus/retention. |
| Vendored winit | 47.9 introduced the198-file vendored tree/Dock hook.38.13 later changes these five files for pointer/source exit. Retain47.9 baseline vendor blobs plus38.13 exact later hunks, rather than assigning final whole files solely by creation provenance. |
| Cargo / lock / platform | Direct dependency ownership +85 new/changed package candidates and2 removals in components JSON. Tree-sitter36.7;resvg/roxmltree40.10;syn/quote35.4;rodio39.4;mailparse42.8;url43.1;same-file44.1;tempfile49.10;tray/KSNI/Zbus47.10;Dock winit47.9 +tearoff38.13;Fluent47.15;OS cc/group anchor buildF000. Shared rustix/exacl/windows feature arrays require per-feature additions. Keep exact final lock bytes. |
| Evidence bundles and frozen requirements | Dedicated evidence files have feature-family candidates; multi-feature reports remain explicit groups. Ten general provenance files have no proposed owner. Parent must record an explicit provenance attachment decision within the94 commits; no hidden F000/source feature catchall and no invented95th feature. |
| Translation and later corrections | Six named Fluent stages own all accepted translation/infrastructure/catalog deltas at47.15. Other17 correction/docs stages retain owning original features/F000. Current partial Fluent is not final acceptance; manually commute remaining shared corrections before final47.15 commit. 771 alone is never treated as final pre-Fluent. |

## Nonlocalization corrections after the checkpoint

These remain feature-owned even though they follow the first Fluent integration:

| Commit | Preserved correction |
|---|---|
| `1ab37567` | Integrate reviewed strict lint corrections for issue 51 |
| `6a7076d6` | Resolve literal operation URLs from ordered Git rewrite metadata |
| `9a57c7e6` | Keep unresolved implicit network routes fail closed |
| `c6a8977e` | Add reviewed isolated native HTTPS credential fixture |
| `5a96acb1` | Validate refs before reftable migration and repair isolated fixtures |
| `6e138bf7` | Prepare bisect logs within owned mutation before dispatch |
| `c908287a` | Correct checked Darwin deferred dispatch and isolated fixture setup |
| `799db3c5` | Align fixture expectations with supported forge and redaction |
| `0ea15c1c` | Test process-list ownership without ambient host assumptions |
| `9230e064` | Exercise reviewed approval in history confirmation fixtures |
| `ba6b0d82` | Use explicit local fetches for passive-download fixture controls |
| `7c85bdb5` | Target painted graph controls in interaction fixtures |
| `e8bef07e` | Add owned-local subtree import conflict and split gates |
| `cc337f2f` | Give subtree fixture repositories independent root histories |
| `4b768a95` | Record immutable owned-local subtree lifecycle results |
| `5b57cfeb` | Preserve contained bundle assertions without lint panics |
| `935d305c` | Integrate reviewed push, storyboard, and DCO completion fixes |

The six translation layers are `771cbd99`, `cde548c3`, `44b3c722`, `710830f0`, `60094d90`, `9cd719ad`. Source changes from these partial integrations are47.15 candidates, subject to final accepted source freeze. Translation touches444 paths; later nontranslation corrections touch105 paths. No virtual corrected pre-Fluent tree is claimed: the chronological layer evidence and net line runs expose exactly where edits must commute.

## Unassigned provenance attachment decisions

- `docs/issue-51/README.md`
- `docs/issue-51/baseline-content.md`
- `docs/issue-51/baseline-core.md`
- `docs/issue-51/baseline-platform.md`
- `docs/issue-51/evidence-policy.md`
- `docs/issue-51/final-validation-plan.md`
- `docs/issue-51/reviews/convergence.md`
- `docs/issue-51/source-issues.json`
- `docs/issue-51/validation/baseline.json`
- `docs/issue-51/validation/dev-0830614f-integration/result.json`

These ten documents cover the entire recovery/input/evidence policy, rather than one implementation feature. Their attachment must be declared explicitly by the assembler; they are not assigned to F000 or localization merely to make counts balance. Multi-feature review/report bundles additionally retain named candidate groups in the ledger.

## Final finite assembly procedure

1. Reverify current fork dev, freeze accepted final combined and final Fluent source, then refresh this draft against those immutable targets. Keep94 ordered IDs, dependencies and exact Jonas author from the reviewed map.
2. Materialize feature-exclusive corrected source and split shared declarations/algorithms using the component/line evidence. Preserve post-4c runtime/DCO/subtree/route corrections with their actual owners. Resolve the listed document attachments explicitly; every final edit needs exactly one owner.
3. Apply the entire accepted translation/catalog migration as47.15 last, including all writers still pending at this snapshot. Never restore stale original-unit files wholesale.
4. Verify93 unique original requirement IDs +F000, exact author on all94 commits, no generated evidence binaries, exact final tracked tree ID and zero full-tree source difference. Rewritten feature commits cite original provenance/reviews without claiming those gates reran.
5. Parent performs required exact final combined local gates and independent adversarial review before final PR/CI. This allocation work neither runs nor substitutes for those gates.
