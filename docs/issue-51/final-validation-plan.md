# Issue51 final validation plan

Planning snapshot: root `d75aab48a46386f71871367b7e803281bd7962a4`. **No builds, tests, application runtime, CI or checkout edits were performed for this plan.** Commands below are future gates, not permission to bypass the current supervisor/runtime hold. Reconcile manifests once against the accepted combined candidate if they change. Do not reopen accepted feature findings to execute this plan.

## 1. Freeze prerequisites before running runtime gates

Finish the finite tracked implementation/correction checklist, including the queued workflow B3/B4 fixes. Preserve the combined unit: required author, commit, push to a non-triggering fork work branch, and verify its remote SHA. Retain immutable source/review/log manifests. Obtain supervisor clearance for the exact integrated source and fixture containment before any Git/helper/process/SSH/Docker/native or broad-suite execution. Synthetic/recording evidence precedes narrowly owned runtime fixtures; no host-wide signal target selection or watchdog cleanup.

Supervisor inventory is not finalized. Parent identifies preserved `a820bb311e9155fbdfdaeaa7596586e0a2098188`; workspace wiring and12 recording schedules remain pending. After its accepted merge, refresh the exact runtime inventory **once**, using its preserved `docs/issue-51/process-safety/*`, `crates/gitcraft-os/src/{unix_anchor,owned_child,process_job,suspended,private_directory,workspace_lock}.rs`, `src/processes.rs`, `src/remote/{live_tests,supervisor,session,workspace_claim,workspace_ledger}.rs`, and bisect test/workspace modules. Run the approved narrow owned fixtures first; do not infer Unix/Windows/SSH/WSL runtime success from recordings/cross-compilation. This plan does not supply missing supervisor acceptance.

## 2. Run CI-equivalent local gates on the final assembled SHA

Use the same default Cargo features as CI: `native-highlighting` and `native-git`. Workspace default members include both `gitcraft` and `gitcraft-os`. CI installs Rust **stable**; Cargo requires at least1.95. Record `rustc --version`, Cargo version, OS/architecture, Git executable/version, feature set, clean tree and exact commit for every platform. Install rustfmt/clippy; native highlighting needs a C compiler/linker. A pinned1.95 local result should be labeled separately from CI's moving stable toolchain.

**Ubuntu-latest equivalent, native Linux:** install the workflow's packages `libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgtk-3-dev libdbus-1-dev libasound2-dev pkg-config openssh-server`; Git, Python3, OpenSSH client/keygen/daemon and ordinary shell tools must be available. Then run these exact workflow commands, preserving exit status and logs:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --no-run
RUST_BACKTRACE=1 cargo test --locked -- --nocapture --skip parallel_bisect::tests::a_test_past_its_limit_is_stopped_and_skipped
RUST_BACKTRACE=1 cargo test --locked -p gitcraft --lib parallel_bisect::tests::a_test_past_its_limit_is_stopped_and_skipped -- --exact --nocapture
sudo mkdir -p /run/sshd
python3 ci/test-ssh-transport.py
```

The timed-out bisect test is isolated in CI, not waived. Isolation alone does not establish safe signal targeting: run it only after the accepted supervision/fixture gate. The SSH script creates an owned loopback sshd, keys/config and test repositories; it is runtime, not a recording test. CI gives the main test step30min and the isolated bisect/SSH steps5min each; do not add an unsafe host-wide watchdog locally.

**Windows-latest equivalent, native Windows:** Rust stable MSVC and normal Visual Studio C/link tools, Git for Windows, Python, PowerShell and the Windows SDK are needed. In the exact final checkout, run the first three commands as separate checked steps and stop on any nonzero exit code:

```powershell
python ci/windows-test-build.py
python ci/test-shards.py --check-only
cargo test --locked --doc
$env:RUST_BACKTRACE = '1'
$env:GITCRAFT_TEST_PROCESS_TRACE = '1'
foreach ($shard in @('git-a-m','git-rest','repo-a-c','repo-d-f','repo-g-m','repo-n-p','repo-q-r','repo-s','repo-rest','core')) {
    python ci/test-shards.py $shard
    if ($LASTEXITCODE -ne 0) { throw "Windows shard failed: $shard" }
}
```

The builder runs `cargo test --locked --no-run --message-format=json` once and writes `ci/test-binaries.json`. Reuse those exact binaries/DLLs for all ten shards. The checker verifies complete/disjoint inventory; the script also runs its four designated tests separately. Doctests are additional. No Windows pass can be established on macOS or by a cross-check build; maintain explicit unavailable evidence if there is no native runner.

**macOS local combined gate:** same formatting/strict all-target clippy, followed by `cargo test --locked --no-run` and `RUST_BACKTRACE=1 cargo test --locked` once containment is cleared, plus the approved explicitly ignored owned fixtures below. These are native macOS results. Main CI has **no macOS job**, and release's macOS matrix entry is commented out. Linux/Windows results and Linux xattrs/Windows Job/security behavior cannot be claimed from macOS. Native audio, native multi-window/source-exit capture and unavailable SSH/WSL paths retain their actual evidence limits.

Historical `cargo clippy --locked --lib -- -D warnings -A dead_code` (or other allowed lints) is **qualified evidence**, not CI lint. CI uses `--all-targets -D warnings` with no allowance. If strict macOS lint encounters its recorded baseline warnings, report that result accurately; it does not replace the native Linux strict gate. Do not silently weaken final CI arguments.

CI does not enable `--all-features` or `--no-default-features`. Preserve earlier fallback-backend evidence; rerun `cargo check --locked --all-targets --no-default-features` only if final integration changes that feature wiring or its previous evidence cannot be bound. Release builds/packages are not PR-check prerequisites: do not run the release workflow merely to validate this PR.

## 3. Explicitly held, already-authored fixtures

Default suites skip `#[ignore]` tests. Run named fixtures explicitly only after their prerequisite is cleared, with clean owned repositories/temp directories and exact test inventory verified against the final artifact. Template:

```sh
cargo test --locked -p gitcraft --lib FULL_TEST_NAME -- --ignored --exact --nocapture
```

| Preserved group | Already-authored names / route | Prerequisite and meaning |
| --- | --- | --- |
| Activity lifecycle (rootd75) | `git::activity::tests::cancelling_a_running_analysis_stops_process_descendants`; `git::activity::tests::running_analysis_enforces_deadlines_and_output_limits` | Approved owned-process cancellation/deadline fixtures. Their previous ignored status is not a pass. |
| Raw reproducer source (workflowfe88, pending corrected merge) | `repo::review_workflow::reproducer::materialize::tests::r2_runtime_raw_archive_does_not_execute_checkout_or_filter_sentinels` | Owned Git sentinel fixture; establish that host hooks/filters do not execute. |
| Frozen transfer/queued publication (workflowfe88) | `repo::review_workflow::sharing::tests::r2_runtime_frozen_endpoint_survives_name_rewrite_and_profile_configuration_changes`; `repo::review_workflow::echo_push::tests::r2_runtime_queued_echo_publishes_only_captured_oid_and_preserves_upstream` | Owned local Git route/ref/profile fixtures after B3/B4 corrections and shared route/credential integration; no live external publication. |
| Historical contract retention (workflowfe88) | `repo::review_workflow::contracts::tests::r2_runtime_backport_roots_survive_gc_after_source_rewrites` | Owned Git/GC/source-rewrite fixture. |
| Reproducer containment (workflowfe88) | `repo::review_workflow::reproducer::tests::r2_runtime_same_integration_test_observes_base_failure_head_success_without_host_checkout` | Docker installed, explicitly preloaded image via `GITCRAFT_REPRODUCER_TEST_IMAGE`; image needs `/bin/sh`, GNU-compatible timeout, tar, cargo, mkdir/cp and offline dependencies. No image pull or host fallback. This single fixture does not prove the entire cancel/crash matrix. |
| Local Actions (rootd75) | `repo::local_ci::tests::owned_fake_act_records_selection_and_stale_source_refuses_execution` | Owned fake-tool fixture first; it is not actual act/Docker workflow execution. |
| Windows output security (rootd75) | `standalone::save::tests::native_windows_label_and_audit_only_drift_are_refused_and_full_copy_is_equal` | Native owned Windows fixture session with `SeSecurityPrivilege`; static/cross-compile checks do not substitute. |
| OpenSSH transport (enabled CI) | `python3 ci/test-ssh-transport.py` explicitly runs `remote::transport::unix::tests::real_openssh_reuses_concurrently_and_recovers` | Disposable loopback sshd; no personal SSH config. |

Confirm full module names after merge from exact source/artifact before invoking; require a nonzero expected selected count. The workflow names above are bound to preservedfe88, not falsely claimed present in rootd75. Existing enabled owned-Git/history/setup/internals/working-state fixtures are covered by the final platform suites; do not rerun all historical filters on top without a remaining evidence gap. Author-supplied pure/recording logs remain separate from actual helper/route/runtime validation.

Ignored benchmarks and the system-credential-store test are not enabled CI gates. Do not blanket-run `--ignored`: it would mix benchmarks, real keychain mutation, containers and live forges. Retain genuinely missing/unavailable native/runtime evidence explicitly.

## 4. Conditional live forge matrix; avoid duplicate CI

`ci.yml` enables forge jobs only for `workflow_dispatch`, not PR/push. If required by the accepted changed forge scope, run the local disposable Linux matrix once after runtime clearance: `(FORGEJO_VERSION,GITEA_VERSION)` = `(17.0.1,28.3.0)` and `(15.0.11,1.26.4)`. Use a fresh owned `FORGE_WORK`, separate ports3000/3001, `ci/forge-server.sh forgejo 3000 ENV_FILE`, `ci/forge-server.sh gitea 3001 ENV_FILE`, load those generated environment values privately, then:

```sh
cargo test --locked --lib --no-run
cargo test --locked --lib forge::gitea::live -- --ignored --nocapture
```

Require all six lifecycle/assigned-issue/inbox tests to pass for each pair, preserving the test exit status (CI additionally checks `test result: ok. 6 passed`). The launcher downloads/checks Linux binaries and starts disposable services; this is a live-service runtime gate. Do not run it now or treat it as ordinary source inspection. No live provider account is needed for this disposable matrix.

CI triggers: pushes to `master`, PRs, or manual CI dispatch. Release triggers: pushes to `dev`, `v*` tags, or manual release dispatch; it can publish a prerelease. Keep all implementation/final-preservation pushes on a non-triggering work branch. **No CI/release dispatch now.** After final local checks and exact-candidate independent review pass, open the one consolidated PR; its automatic CI can be the first final CI run. Avoid a preliminary manual CI dispatch followed by the same unchanged PR checks, and do not push to `dev` to test. Windows required result aggregates build plus all shards. Any later application change invalidates affected final evidence and must receive proportionate local checks/review before a new final CI run.

## 5. Fresh-dev per-feature final history and equivalence

1. Finish/preserve the accepted combined implementation first. Freeze its source SHA and the idea→feature→published-source-commit map. Do not squash all93 baseline incomplete ideas into batch commits or discard recovery branches. Shared prerequisite infrastructure needs an explicit mapped unit; include original feature coverage and accepted corrections in the owning feature commit.
2. Verify actual remote URLs, select the fork remote for `JKamsker/gitcraft`, fetch its `dev`, record that remote dev SHA, and create a **new** final branch/worktree from that SHA. The stale local `dev` and upstream read-only `Vancete/gitcraft` are not the assembly base. Commands use the verified remote name, not an assumed `origin` alias: `git fetch "$task_fork_remote" dev`; `git worktree add -b issue51-final "$task_final_worktree" "$task_fork_remote/dev"`.
3. Apply accepted feature units in dependency order, splitting multi-feature WIP commits as necessary. Commit each feature under exactly `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`. Retain a provenance table and verify the required author rather than assuming cherry-pick retained it. Commit/push each completed unit on the safe work branch and verify its remote SHA; do not modify other worktrees or rewrite published recovery branches.
4. If freshdev introduced changes absent from the accepted integration, integrate those into the accepted combined target first and review the concrete integration delta. Then compare the final assembled tree with that accepted target: prefer `git diff --exit-code "$task_accepted_tip" HEAD --`. If final evidence/ledger documentation intentionally differs, enumerate/review exactly those documentation paths; require every application, dependency, build, asset, test, packaging and CI file to match, including modes/symlinks/deletions. Save tree/path hashes and the empty application diff. A similar-looking feature history or clean compile is not source equivalence.
5. On the final assembly SHA, perform the applicable native local gates and explicit held fixture inventory above, record failures/ignored/unavailable counts honestly, then obtain the independent adversarial combined review bound to that exact candidate. Documentation-only evidence commits may reuse prior application gates only after proving application files unchanged. Verify required author and pushed remote SHA again. Only after these gates pass, publish the single fork PR and use final CI. Do not mark unexecuted platforms/features passed.

## Immutable planning inputs

The following files were read from rootd75 Git objects; SHA256 binds this plan's CI interpretation. Supervisor/workflow references above are preserved pending candidates and are not root acceptance claims.

- `AGENTS.md`: `a6b99ef334f51715e97ee6fd0d24fd4bd6920999d277657edc87ef6d570ffb1d`
- `.github/workflows/ci.yml`: `71cdfee62b0d469416d19628d955e94ab64e381591a2b27d7763e98fa5e5ddc2`
- `.github/workflows/windows-tests.yml`: `100334f8ddfe31a88172ef717f22a08b37e26d1badad0a519b1c4191ef5933e9`
- `.github/workflows/release.yml`: `2b6c7ad0bdd317e838d39fa1a96bd379a9dfcff71099e29da2ac17f6f602a86f`
- `ci/windows-test-build.py`: `1ef9bbdb80941124a4be8451e33911dd1d986966a008ca3502d95c44d0e6c6ca`
- `ci/test-shards.py`: `91a11f2b12a08bce91b69ebd53a66557ff450e08516c252bdfe527e27bdb7516`
- `ci/test-ssh-transport.py`: `345febd573861a54db6fce5e36cedf7aa4310bc5728265f535e2894003b0749d`
- `Cargo.toml`: `8f536c8f48138bc2a79f96988af53b95d46367e5b494463537c485589ceb01ca`
