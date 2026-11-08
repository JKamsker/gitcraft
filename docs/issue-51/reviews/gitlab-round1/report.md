# GitLab native MR feature review R1 — FAIL

Candidate: **652e63345057aee614480c656edf3b959e8bb56b**. Tree: `5660b43324ac9cff2ebc7199a8841763f4371728`; immediate parent: `2d93de3b9c521af48b6dda0264ac9bfea75505d5`. Reviewed immutable archive `/tmp/gitcraft-gitlab-r1-652e6334`, not the moving writer worktree. Manifest: `/tmp/gitcraft-51-gitlab-review-r1-manifest.json` (24 source files independently matched to their candidate Git blob IDs, plus artifact/log SHA-256 hashes).

Scope is precisely issue #43's requirement: “Full GitLab merge requests: only lists/creates currently; open MR in big area like GitHub PR, with changes, line comments, resolving discussions, approval and merging; show merge blockers.” Unsupported pending reviews, request-changes verdicts, suggestion application and rebase operations are outside this requirement. This is one original-requirement review, not a whole-branch, shared credential API, process supervision or native platform acceptance.

**One P2 blocker: an outdated line discussion cannot be replied to or resolved from the app.** Other requested paths have substantive reachable implementations and no additional concrete blocker was established in this bounded review.

## GL-R1-1 — P2: outdated line discussions lose every in-app reply/resolution entry point

Principal location: `src/repo/pr_review/native_discussions.rs:16`. The native discussion controls admit only threads whose `path` is empty. File discussions remain outside that UI even when they can no longer appear under any current diff line.

Source proof at the candidate:

- `src/forge/gitlab/details.rs:352` parses positioned discussions, retaining their file path and replies. At lines 378–385 it marks the discussion outdated if any original base/start/head SHA differs from current MR `diff_refs`, while independently retaining `can_resolve` and `can_reply`.
- `src/repo/pr_review/native_discussions.rs:13–16` excludes every such nonempty-path discussion from native reply/resolve cards.
- `src/repo/pr_review/line_panel.rs:174` selects only `thread_is_on(...)`; `src/repo/pr_review/lines.rs:70–71` unconditionally rejects outdated threads. Opening the corresponding current file/line manually does not restore these controls.
- `src/pull_request/mod.rs:123` keeps the comments in timeline entries, but `src/repo/pr_view/timeline.rs:155` suppresses “Show it in the diff” for outdated entries. The timeline header/body has no native reply or resolve action. GitLab line comment URLs are also empty (`details.rs:393`), so the individual entry does not even acquire the generic external-link button.
- The backend can resolve a displayed old discussion: `src/forge/gitlab/post.rs:185` checks displayed discussion membership/current MR ownership and calls the native resolution endpoint. The missing piece is reachable UI, not a nonexistent server operation.

Concrete reproduction scenario (specified, **not executed against Git/GitLab**):

1. Open an MR with an unresolved, resolvable discussion positioned on `a.rs:2`, originally at head A, as a user allowed to resolve it.
2. The MR source advances to head D; reload. The discussions API still returns that thread's original A position, while the current MR `diff_refs.head_sha` is D. Set/observe `detailed_merge_status=discussions_not_resolved` when GitLab requires all discussions resolved.
3. The page displays the discussion as outdated in its timeline and displays the resolve-discussions merge blocker. There is no reply, resolve or reopen control for that file discussion anywhere in the page/current diff. Thus this required workflow cannot clear its own displayed blocker.

Independent inert reproduction: `/tmp/gitcraft-51-gitlab-r1-evidence/reachability.rs`, log `reachability.log`. It extracts the candidate's actual `native_discussions::show` and `thread_is_on`, widens their visibility, and uses a deliberately inert recording UI plus a minimal Thread fixture. It executes no application constructors, Git, shell discovery, API adapter or process-control code. Two checks passed:

- Fixture `path=a.rs`, `outdated=true`, `can_resolve=true`, `can_reply=true`: **zero native discussion buttons; line-panel predicate false**.
- Positive controls: the current-line predicate is true for the same current file thread; a general discussion renders `Resolve discussion` and `Reply to discussion`.

Compile/run commands:

```text
/Users/jonas/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc --edition=2024 --test /tmp/gitcraft-51-gitlab-r1-evidence/reachability.rs -o /tmp/gitcraft-51-gitlab-r1-evidence/reachability-tests
/tmp/gitcraft-51-gitlab-r1-evidence/reachability-tests --nocapture --test-threads=1
```

Minimal correction: offer a native discussion card for outdated/unmapped file discussions, preserve its original path/coordinate and outdated label, and route reply/resolve/reopen by the shown thread ID through the existing guarded target. Do not attach an old thread to a current line as if its coordinates were current. Add a real headless review UI regression with an unresolved outdated file discussion and a recording resolution/reply regression preserving the current MR/account/revision guard.

## Original requirement disposition

| Requirement | Scoped verdict | Concrete source evidence |
| --- | --- | --- |
| Preserve existing list/create/find GitLab operations | PASS, source preservation | GitLab module rename is 95% identical. The complete rename diff against the immediate parent changes imports/module declarations, adds `Provider::pages`, and appends `Pages`; existing list/create/find/merged/issues/inbox functions are unchanged. Proof saved in `list-create-find-preservation.diff`. Existing four GitLab parser/host tests remain present and passed in writer log 0. |
| Open MR in main area | PASS, source reachability | `gitlab/mod.rs:32` returns Pages; its implementation at 477 reads native details and supplies a native Target. `repo/sidebar/pull_requests.rs:146` uses `has_pages`, double-click/Open emits `OpenPr`; `repo/apply.rs:59` opens the view; `repo/pr_view/mod.rs:84,110` asynchronously loads details. |
| Show changes | PASS, source/recording scope | Native details bind exact merge-base/base-start-head OIDs (`gitlab/details.rs:147,286`). Shared PR file loading uses validated OIDs and native `refs/merge-requests/{iid}/head`, fetches missing objects, then creates the local diff (`pull_request/mod.rs:174`). Details→files callbacks are request-tagged (`repo/pr_view/mod.rs:136,185`). The recording read test checks changes/counts. No actual Git fetch/diff was run in this review. |
| Line comments and replies | PARTIAL | Current displayed diff lines route through existing line UI and native `comment_now`/reply; `post.rs:151,177,245` verifies exact current MR, diff membership, old/new paths, base/start/head and context/add/remove coordinates before posting native discussions. Outdated file discussion replies are unreachable (GL-R1-1). |
| Resolve discussions | FAIL | General and current mapped file discussion controls exist; native PUT resolution backend exists. Outdated file discussions are excluded from every control surface (GL-R1-1). |
| Approval | PASS, source/recording scope | Native `user_can_approve` controls capability; unsupported verdicts are hidden. `post.rs:122` rechecks approval rights and current account/revision, sends optional comment and native approval with displayed head SHA. Recording test asserts the approve route/payload. |
| Merge | PASS, source/recording scope | `repo/pr_merge.rs` renders native policy/capabilities and confirmation. `post.rs:213` freshly checks MR/project/approvals, rejects unsupported method/fork-branch deletion, rechecks revision, sends native merge with head SHA/squash/remove-source-branch and requires a confirmed merged response. |
| Show merge blockers | PASS for implemented display; clearing discussion blocker incomplete | `gitlab/details.rs:94` covers draft, permission, unknown policy/approval state, conflicts, required discussions/approvals/CI/rebase/checking and explicit unknown statuses; `pr_merge.rs` displays the native blocker and disables merge. GL-R1-1 prevents clearing one ordinary blocker through the app. |
| Selected Context/account/revision writes | PASS within this consumer's source/recording scope | Snapshot stores immutable Context binding, stable user ID, MR ID/IID and full revision (`details.rs:174`). Target verifies binding/source and pins the displayed default viewer when no explicit GitLab account was selected (`post.rs:18`); `guard` rechecks Context/user ID+name/URL/MR identity/state/head/base/full revision before writes. API execution runs under captured Context, explicitly names HTTPS authority/project, strips ambient token/routing overrides, then applies selected CLI authentication (`api.rs:47,153`). No live authentication acceptance is claimed. |

The implementation uses the documented native API shapes: diff positions include base/start/head plus both paths, added lines use new_line, removed lines old_line, and context lines both coordinates; discussion replies and resolution target discussion IDs. [GitLab Discussions API](https://docs.gitlab.com/api/discussions/). Native approval and merge include the reviewed head SHA, which GitLab documents as a source-head condition; other endpoints have no advertised CAS here, and the implementation rechecks without claiming atomicity. [Approval API](https://docs.gitlab.com/api/merge_request_approvals/), [Merge Requests API](https://docs.gitlab.com/api/merge_requests/).

## Evidence and limits

Writer evidence inspected, **not independently rerun**: `/tmp/gitcraft-51-gitlab-652e6334-0.log` has 12 passing provider tests (8 new pure/recording tests, 4 existing parser/detection tests); log 1 has 1 passing headless supported-verdict UI test; log 2 records successful `cargo check --all-targets`; log 3 successful qualified `clippy --lib -- -D warnings -A dead_code`. Entries in `/tmp/gitcraft-51-batch-b-validation.json` name this exact candidate. The recording adapter returns before production command discovery/account resolution/process execution; the command-builder test supplies a fixed inert `Command`. These are not live GitLab/process/auth gates.

Independent execution in this review was only compilation and two tiny extracted-function/inert-UI checks described above. This does not prove full actual egui rendering or actual native GitLab interoperability. No actual Git/CLI/helper/HTTP/SSH/network/auth/native/process fixtures, app constructors, process control, CI, full suite, PR or external write ran. Official public API docs were retrieved read-only. Existing shared credential/process reviews remain separate integration dependencies and are not recertified or duplicated as GitLab findings here.

**Acceptance: FAIL for the original MR workflow until GL-R1-1 is corrected.** No additional concrete functional/credential/data-loss blocker was established in this bounded pass.
