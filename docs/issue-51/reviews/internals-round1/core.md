# Advanced internals core review — ROUND 1

**FAIL** at exact `3e5b685a17d9351ae59dd89cd4c148358c9644bd`, base `a850de26b0a0778ea96fe34cd1498e6e78928492`, checkout `/Users/jonas/.t3/worktrees/gitcraft/t3-3a66e6f0`. Reviewed original issue #49, handoff `/tmp/gitcraft-advanced-repository-tools-report.md`, validation manifest and scoped implementation. Scope: separate pull/push routes, remote URL/refspec editor/prune preview, multi-host publication/failed-only retry and their transport/guard helpers. Other reviewers own refs/object/submodule/borrowed objects.

No tracked edits, CI, full suite, live external writes, native Windows/Linux/SSH claims, agents or unsafe process selector tests. Review stopped at parent priority boundary after the established findings below; it does not certify every remaining refspec/partial-save corner case. Fork dev083 push-destination changes are outside this exact candidate and require separate integration validation.

## B1 — P1: already-expanded push URLs are rewritten again, so a prepared destination can be wrong even with unchanged configuration

Source: `git/repository_tools/publication.rs:104–131` captures `remote get-url --push --all`, which returns effective rewrite-expanded URLs; `:155–172` passes each captured string to `git push URL`. Git applies push URL rewriting to that positional URL again. `routes.rs:391–437` uses the same pattern for force push: advertise/lease-check captured URL, then submit it to push. A stable two-step rewrite configuration can therefore lease/preview host A while publishing host B. Snapshot checks pass because configuration never changes.

**Confirmed by copied production test:** a fixture-only Git wrapper adds stable chained `pushInsteadOf` mappings before prepare. Remote a has a literal alias which expands to owned bare host A; a second mapping expands A's URL to owned bare host B when it is resubmitted to push. Production prepares A, records success for A, and B receives the exact pinned commit. A's refs/heads/main remains absent; the unchanged publication test fails trying to verify A. No external endpoint or timing race was needed.

Evidence `/private/tmp/gitcraft-internals-review-core-r1/chained-rewrite-test.log`, `chained-rewrite.jsonl`, `bin/git`. Recorded prepared A `/private/tmp/gitcraft tëst-publication-host-a ü-34512/host.git`; actual B `/private/tmp/gitcraft tëst-publication-source ü-34512/.git/review-actual-target.git`; actual received commit `64f340a9a1b3f6d7c7f2a9a784929dea1117d0a1`. This is an actual production publication run, not only a rewrite algorithm model. Force-path impact is source-derived: if B has the same advertised old OID, the lease does not prove it is the inspected host.

Correction: bind the executed transport to the once-resolved endpoint under the captured routing context; do not feed an expanded endpoint through a second unconstrained rewrite pass. Preserve credential behavior explicitly. Cover stable chained insteadOf/pushInsteadOf, literal URLs that match another remote name, changed rewrite/profile config, multiple push URLs, actual-advertisement leases and per-host outcome attribution. Also inspect ordinary branch `push_branch`/`rewrite_plain_push`: they still execute by mutable remote name after their reads/preflight, so destination changes in that window are not prevented by URL pinning. No runtime race claim is made for those source-only paths.

## B2 — P1: multi-host execution forwards a single unscoped tab credential to every host

Source: `repo/repository_tools/mod.rs:695–714` captures `self.credentials` once and gives it to publication's Runner for all destinations. `git/repository_tools/guard.rs:87–90` adds a helper returning `GITCRAFT_GIT_USERNAME/PASSWORD` for any credential `get`, without reading/validating requested protocol, host or path. Shared `Credentials` (`git/command.rs:675–678`) contains only username/password, so this caller cannot establish a destination/account binding. Existing `extra_for` installs the same generic helper outside the alias as well.

**Dummy-only credential-protocol probe:** exact Runner alias and helper, isolated empty owned repository/config, `git credential fill` requested `selected-a.invalid`, then `unrelated-b.invalid`. Both returned `DUMMY_HOST_A_USER` and `DUMMY_HOST_A_TOKEN`. No network operation, real token or provider was used. This demonstrates the helper behavior; the per-destination forwarding comes directly from the production call chain above. Existing configured helpers may answer first, but an otherwise unknown second host falls through to the unrelated tab token.

Evidence `credential_scope_probe.py`, `credential-scope.json` in the artifact directory.

Correction: resolve/capture credentials per actual destination authority/account/path as appropriate, and guard that identity along with route/profile context. A transient credential entered for one host must not be offered to arbitrary group members, redirects or changed push URLs. If existing transient state lacks scope, do not reuse it across authorities. Add recording-only helper request tests for both matching and mismatched protocol/host/path, then local/fake-transport integration with dummy values. Do not use real secrets or live hosts for the regression.

## B3 — P2: scoped read guards sanitize Git redirection but three decisions fall back to unisolated run_captured

`git/repository_tools/mod.rs:23–60` strips inherited Git repository/index/config redirection for advanced reads; Runner strips it for writes. In contrast these scoped reads call ordinary `git::run_captured`, whose `command.rs:409–434` delegates to unsanitized ordinary execution:

- `remote_config.rs:121`: native-boolean reads, including mirror policy used to allow/refuse multi-host preparation.
- `remote_config.rs:267`: local config existence check before unset, potentially skipping the selected repository's value or attempting an incorrect unset after earlier fields were written.
- `routes.rs:60`: tracking ref resolution used for route counts, while the surrounding branch/graph reads use sanitized plumbing.

**Confirmed production mirror-policy mismatch:** inject inherited `GIT_DIR` and `GIT_WORK_TREE` to another owned dummy repository at only the raw mirror-boolean child boundary. Actual selected repository has `remote.a.mirror=true`; isolated Snapshot still captures the selected repository correctly. Boolean reads unset/default=false from the other repository, and `prepare` succeeds. The unchanged publication test fails its required mirror-refusal assertion (`publication.rs:192`). This is decision inconsistency, not a claim that this particular ordinary explicit-ref push itself mirrored all refs.

Evidence `redirected-boolean-test.log`, `redirected-boolean.jsonl`, wrapper. Source derivation covers local-existence/count effects; they were not separately reproduced.

Correction: provide a scoped captured-result helper which preserves exit-code handling while applying the same redirection removal/profile context/bounds as every other advanced read, and migrate these decision points. Test misleading other-repository mirror booleans, existing config keys and tracking refs. Keep inherited environmental configuration distinct from intentionally selected profile configuration.

## Verified nonfinding and narrower positive results

The outer Runner alias was separately challenged with inherited GIT_DIR plus GIT_WORK_TREE at the actual production mutation child boundary during the remote editor test. The test **passed**, selected repository writes remained correct, and the unrelated dummy repository's local config stayed unchanged. Thus this report does **not** claim the outer alias itself redirected mutations on the checked macOS/Git path. Artifacts `redirected-write-test.log`, `redirected-write.jsonl`. This narrower nonfinding does not repair raw read inconsistency B3.

Original focused production tests all passed independently before injection: publication1, routes2 and remote_config2. They establish ordinary independent counts, stale-save refusal, selected push branch rewrite, one-URL force lease against concurrent remote movement, negative fetch exclusion/prune example, inherited remote-setting refusal and partial-publication/failed-only retry behavior. The retry fixture makes an already successful endpoint unavailable; retry preserves its success and only contacts the failed host. UI explicitly states non-atomic publication. Remote editor saves require an exact preview and unchanged snapshot, report possible earlier partial field writes on failure, and perform no actual fetch/prune/push during save. Force preparation refuses multiple push URLs; group preparation enumerates every configured push endpoint. These are substantive implementations, but B1/B2 block accepting destination/account guarantees.

No additional prune-mapping overlap, ref deletion, external routing-race, partial-route-save or interactive UI test was run before the priority switch. No broad PASS is inferred for those cases.

## Independent artifacts and limits

Copied the candidate worktree artifact identified by its validation log, `target/debug/deps/gitcraft-46b831b93877728d`, to `/private/tmp/gitcraft-internals-review-core-r1/review-tests`. SHA256 `9bcfa1ae6c5a7ba895358301221725c8532fcb29102edf6c62317cc666f0bb6a`. This reviewer did not freshly build it. Static evidence is bound to exact source3e5b685a; runtime claims additionally bind to that executable hash. Environment TMPDIR=/private/tmp, SHELL=/tmp/gitcraft-integration-tools/validation-shell, integration tools and `/opt/homebrew/bin/git` on PATH. Injection prepends only the fixture wrapper; it modifies explicitly owned `gitcraft tëst-*` repositories and dummy artifacts, with no process discovery/signals/live endpoints.

| Check | Result | Artifact |
| --- | --- | --- |
| `review-tests repository_tools::publication --nocapture` |1 pass|`publication-baseline.log`|
| `review-tests repository_tools::routes --nocapture` |2 pass|`routes-baseline.log`|
| `review-tests repository_tools::remote_config --nocapture` |2 pass|`remote-config-baseline.log`|
| publication with REVIEW_CHAIN_REWRITE=1 |1 failure; advertised A absent, actual B receives commit despite successful result|`chained-rewrite-test.log`, `chained-rewrite.jsonl`|
| publication with REVIEW_REDIRECT_BOOLEAN=1 |1 failure; mirror refusal bypassed|`redirected-boolean-test.log`, `redirected-boolean.jsonl`|
| exact negative_mapping_pruning_and_guarded_url_edits with REVIEW_REDIRECT_MUTATION=1 |1 pass; unrelated dummy config unchanged|`redirected-write-test.log`, `redirected-write.jsonl`|
| exact helper/alias dummy credential-fill |same A dummy credential returned to A and unrelated B; no network|`credential-scope.json`|

Artifacts reside under `/private/tmp/gitcraft-internals-review-core-r1/`. No fresh build, native Windows/Linux runtime, real SSH/WSL, real credentials/provider writes, process timeout/cancel suite, unsafe legacy selector tests, full suite, CI or PR. All unexecuted platform/process/combined gates remain pending. This report is separate from the next static-only process-safety prerequisite review.
