# Bitbucket BB-R1-1 delta review R2 — scoped PASS

Exact candidate `c42bfe7106ab0e26696872457f22f2c8dab45624`, tree `2ca6a30bf17b95e05586b1ab3201dcd749fa4046`, parent `fe2d56e38ee913cf2f8febf71ee9f7f1012b4312`. Immutable Git-object inspection only. Scope is the single BB-R1-1 correction; preceding mailparse work and unrelated providers are not reviewed here.

**BB-R1-1 CLOSED.** The new helper in `src/forge/bitbucket/mod.rs:93–108` validates full base/head OIDs and calls the JSON repository diffstat endpoint with `SOURCE..COMMON?topic=false&renames=true&pagelen=100`. Bitbucket documents source-first/comparison-second ordering, opposite to `git diff`; `topic=false` compares those two commits directly. This is the correct orientation for the displayed common-to-source changes. [Official diffstat semantics](https://developer.atlassian.com/cloud/bitbucket/rest/api-group-commits/#api-repositories-workspace-repo-slug-diffstat-spec-get).

Both prior failing routes use the helper: page counts pass validated common ancestor and source SHA (`details.rs:56`), and positioned lookup passes displayed common/source OIDs (`post.rs:96`). Neither calls the PR convenience endpoint. The old 302 no longer needs following; redirects remain disabled. The existing left-side rename content validation and line-comment payload are preserved after the helper substitution.

Preservation checks against R1 `eed8eca6`: the cloud transport and `pull_request/mod.rs` are byte-identical, and the native pagination implementation is identical. Thus origin/byte/deadline refusal, same-collection next-page checks, page/item limits, foreign-fork fetch isolation and destination-credential handling are unchanged. The only production changes within the Bitbucket provider are the new helper and its two call sites; repository/snapshot/account/current-revision guards remain intact. A source-only assertion receipt records these comparisons at `/tmp/gitcraft-bitbucket-r2-source-checks.json`; it is not an application test receipt.

Exact writer evidence was source/log inspected:

- New regression (`bitbucket/tests.rs:298–350`) records the old PR diffstat route as a refusal and asserts it is never called; checks native counts `(1,2,1)` and two identical source/common repository comparisons; a renamed-left request records exactly one comment POST with the intended old-side line and current file path.
- Foreign and repeated repository-diffstat cursors still refuse; non-OID base refuses. Prior source/target/repository/account/profile and discussion checks remain among six recording tests.
- `/tmp/gitcraft-51-bitbucket-r1-fix-c42bfe71-0.log`: **6 passed**, zero ignored. `-1.log`: all-target check PASS. `-2.log`: qualified all-target clippy `-D warnings -A dead_code` PASS. Commands name exact candidate in `/tmp/gitcraft-51-batch-b-validation.json`. These are supplied writer executions, not independently rerun application tests; strict dead-code-clean lint is not claimed.

No application/discovery constructors, actual Git, helper/CLI/keyring, HTTP/provider runtime, native UI, process/cancel/timeout fixture, full suite, CI, PR or tracked edits. Only source Git reads, public primary documentation and static comparisons were performed independently. Reports and JSON references remain under `/tmp`; no generated binary or archive evidence was committed. The original Cloud-only/native-completion-handoff scope and held native/runtime limits are unchanged.

Verdict: **scoped PASS — BB-R1-1 closed at exact c42bfe71.** This does not certify native service/platform behavior or unrelated inherited changes.
