# Jira/Linear original-feature review R1

**FAIL on exact `9d8f54cf9c58ad70182a88e961dc40ec2356c02e`.** Two concrete functional defects remain: pointer selection cannot choose earlier tracker rows, and a configured Jira criteria field can disappear from the working card. The remaining original flow is implemented within inspected static/pure scope. No provider, secure-store or native Git acceptance is claimed.

## Binding and scope

Tree `8d73281ffa3233ea4f75e7f427038c3b13e2a35b`; immutable archive `/tmp/gitcraft-trackers-r1-9d8f-source`. Read original issue 43, the full tracker handoff and batch validation receipt; selected only the five receipt entries bound to this candidate. Source came from immutable Git objects, not the moving writer checkout. Evidence is under `/tmp/gitcraft-trackers-r1-9d8f-evidence`; companion manifest binds source, adapters, dependency artifacts, receipts and logs.

Scope is assigned Jira Cloud/Linear issues through Start work from an issue, issue-named branch/worktree, persistent criteria and subsequent PR backlink, plus the new profile/account/request wiring. Jira Cloud email/API-token and Linear personal-key support, the visible first 100 cap and unavailable Data Center/OAuth are the agreed limits. Existing GL-R1-1 and the separately closed shared credential-policy findings are excluded; no adjacent integration inventory was added.

## Findings

### TR-R1-1 — P2: all tracker rows share their interactive widget ID

Source: `src/trackers/mod.rs:375–387`; `src/repo/issue_work/picker.rs:326–348,422–425`.

Every Jira/Linear Issue has number=0. The picker correctly changed selection state to list indices, but issue_row still builds its egui interaction ID from ("issue-row", issue.number). All tracker rows therefore register the same widget ID in the same parent UI. The model test tracker_keys_choose_distinct_rows_without_forge_numbers calls choose directly and never tests pointer interaction.

Independent headless reproduction extracts the **exact issue_row function** and renders two rows, ENG-9 and OPS-9, using cached egui with inert theme/chip helpers. No App/RepoTab constructor or OS window is involved. Actual headless pointer press/release results:

```text
row0 rect: [8,8]–[592,36]; row1 rect: [8,39]–[592,67]
click row0 -> [false, false]
click row1 -> [true, true]
```

Production choose iterates all rows and overwrites clicked with the last activated index. Thus the first row cannot be selected by pointer; clicking the second activates both responses and ultimately selects the last row. With several results, this can select a different issue/branch than the clicked row. Keyboard selection remains a workaround, not fulfillment of the visible picker.

Minimal correction: give every rendered row a distinct identity, preferably the source-qualified stable tracker issue identity while preserving forge identities. Add a two-tracker-row headless pointer test that asserts each click selects only its intended issue; model-only choose(index) coverage is insufficient. No provider or native fixture is needed.

### TR-R1-2 — P2: configured Jira criteria are dropped when the description already has a criteria section

Source: `src/trackers/mod.rs:320–327`; `src/repo/issue_work/criteria.rs:166–180`; `src/repo/issue_work/link.rs:46–55`; `src/repo/issue_work/card.rs:117–132`.

Jira parse converts the description, then appends the selected customfield_N under another same-level "Acceptance criteria" heading. The existing extractor selects the first matching section and stops at the next heading of that level. A perfectly ordinary issue with a criteria heading in its description therefore hides the explicitly configured criteria field from the persisted link and card.

Independent pure reproduction uses the **exact Client::parse, required, adf and criteria-extraction functions** with inert account/issue structs and supplied JSON:

```text
description ADF: ## Acceptance criteria / old description requirement
configured customfield_10001: selected custom-field requirement
parsed body: ## Acceptance criteria\nold description requirement
             \n\n## Acceptance criteria\nselected custom-field requirement
visible/persisted criteria: "old description requirement"
```

The configured field is present in the API result and parsed body but absent from the only criteria string kept by IssueLink::of. Refresh repeats the same loss. This is a direct failure of keeping the issue's acceptance criteria in view, rather than an unsupported rich-document-format case.

Minimal correction: define the selected field's precedence or combine it with extracted description criteria so the configured text is kept. Avoid appending duplicate criteria headings into a first-section-only extractor. Test parse → kept link/card data with both sources populated; existing happy-path customfield tests cover a description without a criteria section and miss this case.

## Per-requirement assessment

| Original requirement / requested boundary | Assessment and evidence |
| --- | --- |
| Reach Start work from an issue and choose a tracker, including repositories without a recognized forge | **Implemented static.** actions.rs:798–804 offers the command unconditionally; issue_work/mod.rs:163–191 creates a picker before the forge error; picker.rs:178–188 exposes Repository forge/Jira/Linear. An unavailable forge does not prevent selecting a tracker. |
| Browse issues assigned to the selected account | **Implemented source contract; provider runtime held.** trackers/mod.rs:159–179 verifies viewer email/stable ID; Jira currentUser unresolved JQL and Linear viewer.assignedIssues exclude finished states; :240–250 verifies each assignee. GraphQL errors reject partial data. Official APIs support these request/auth patterns. |
| Stable source/account/issue identity and cursor handling | **Implemented data model; row interaction FAIL TR-R1-1.** Identity stores kind/origin/account/viewer/stable issue ID/key; refresh checks source and stable viewer/issue IDs (:274–310). Lists use opaque provider cursors, refuse missing/repeated continuations and deduplicate stable IDs. The first 100 cap is disclosed at picker.rs:337–345. |
| Create a branch or worktree named after the selected issue | **Implemented static/pure operation path, blocked by pointer picker defect for ordinary selection.** Issue::branch_name → tracker_branch, issue_work/mod.rs:194–227 → existing issue_plan and start_ops; issue_work/link.rs:223–258 emits checkout/worktree and link writes. Existing base/dirty-change/existing-worktree behavior is reused. No Git command was executed. |
| Acceptance criteria stay visible | **Partial, FAIL TR-R1-2.** Supported ADF task/text/heading/table/link/code and Linear Markdown reach the existing persistent branch link/card. Unknown ADF nodes have visible markers; depth/response bounds error. The existing card explicitly caps long text with an ellipsis and shows its read date. Configured Jira criteria can still disappear in the demonstrated case. |
| Refresh/freshness | **Implemented static for supported criteria path.** issue_work/mod.rs:431–472 captures original link and connection, discards changed-profile/unlinked/replaced-link callbacks; tracker one checks stable identity. Refresh reuses the same parsing defect. No live refresh or external route mutation was exercised. |
| Subsequent PR links back | **Implemented static/pure.** Branch Create PR uses issue_pull_request (sidebar/branches.rs:413–428); external links seed Relates to KEY: URL through link.rs:62–68. Forge links retain Closes #N. No PR was created. |
| Additive nonsecret profile metadata and secure references | **Implemented static.** connections.rs adds defaulted Jira/Linear Account fields; Account serializes origin/email/reference/optional criteria-field only. Token input is transient egui state. Account store/secret and UI Forget target the selected reference; portable-mode store refusal is explicit. Actual keyring behavior is held. |
| Worker and selected-origin credential boundaries | **Implemented static contract.** Settings Save/Forget submit worker jobs (trackers/ui.rs:62–128). assigned/one are invoked through RepoTab::then; task jobs retain initiating Context. live resolves keyring secrets on the worker, uses the selected HTTPS origin only, refuses redirects, sends Jira Basic email/token or Linear personal-key header and returns bounded generic failures (trackers/mod.rs:478–558). No Git helper reuse, token metadata serialization or token request logging was found in the new path. No actual credential/store/provider acceptance is inferred. |
| Existing GitHub/GitLab/Forgejo/Gitea issue behavior | **Preserved in inspected delta/pure gates.** Native Issue construction sets tracker=None; numeric forge keys, naming, legacy branch-link reads and Closes #N remain. Picker state now uses indices for all sources; unique forge numeric widget IDs retain their prior behavior. GL-R1-1 remains separately excluded. |

Public primary documentation checked read-only: [Linear endpoint/authentication and GraphQL errors](https://linear.app/developers/graphql), [assigned-issue filtering](https://linear.app/developers/filtering), [cursor pagination](https://linear.app/developers/pagination), [Jira enhanced JQL search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/#api-rest-api-3-search-jql-post). These were document retrievals, not provider API fixtures.

## Gates and limits

Writer receipts/logs inspected, not rerun: six tracker pure/recording tests, four picker model tests and five link pure tests (15 total), offline all-target check and qualified offline library Clippy -D warnings -A dead_code, all exit0 at exact 9d8f. These existing passes do not cover the two demonstrated compositions. Historical earlier batch entries retain their original execution classifications, including the recorded pre-hold actual Git-fill case; they are not current tracker gates.

Independent work: four rustc +1.95.0 invocations, three successful. One driver compilation initially used the older egui Context::run API; fixed to run_ui without changing extracted production code. The first headless run then failed because the driver did not clear an intentionally unused TexturesDelta; the corrected driver reproduced row selection on its second run, exit0. One pure criteria adapter compiled and ran once, exit0 with defect assertions. Source and execution inventories preserve these failures and corrections rather than treating them as passes.

No actual keyring, provider HTTP, Git/helper/CLI/process fixture, App/RepoTab/discovery constructor, OS/native window/PTY, signal/cancel/timeout fixture, full suite, CI, PR, source edit or subagent occurred. Cached egui dependency code and inert supplied data were the only UI execution. After these two concrete defects are corrected, existing future provider/store/Git/platform acceptance gates remain separate under the unchanged runtime hold; no parity or broader authentication work is requested by this review.
