# Local Actions Act R2 — PASS for the four corrections; runtime HOLD remains

Exact candidate: **12e7641c917d92482788f5f8a3449ffca425f83f**, branch identity supplied as `t3/issue51-batch-b`. Original R1 candidate: `052721ed3c36853ad97c34485dbef65950f7ea38`.

**Verdict: all four R1 P2 findings are closed at the static/pure/recording evidence level. No new actionable Act correction defect found.** This is a narrow correction review of the local Actions feature in [#43](https://github.com/JKamsker/gitcraft/issues/43), indexed by [#51](https://github.com/JKamsker/gitcraft/issues/51). It is not whole-batch approval, real act/Docker validation, process-supervisor clearance, credential-helper certification, or final combined acceptance.

## Provenance and evidence classes

The full original R1 report and evidence manifest were read, as were the full correction response, structured validation record and all three candidate logs. The parent's `docs/issue-51/reviews/act-round1` copies were checked against parent commit `cec166d429365a5e6404457425b59f5127d69194`: the report and decompressed manifest equal the supplied originals, and all eleven preserved archive payload digests match their index.

Candidate source came from Git objects and a fresh `git archive` extraction at `/tmp/gitcraft-act-r2-12e7641c-t2c19ce3`. Every application source file listed in the evidence manifest was compared byte-for-byte with `git show <candidate>:<path>`. No application source was read from the writer's live worktree. A moving branch or a later writer commit does not alter this verdict. The commit's author and committer are the required Jonas Kamsker identity. No claim of a newly verified remote branch SHA is made by this reviewer.

- **S — reviewer static inspection:** exact candidate source, candidate correction diff, adjacent unchanged-source comparisons, and exact process-owner port comparison. `/tmp/gitcraft-51-act-r2-static-checks.log` records mechanical checks.
- **P — reviewer-executed pure model:** archived model/context code compiled in an external harness, with `yaml-rust2 = 0.13.0`.
- **R — reviewer-executed recording only:** archived `run.rs` executes against stub Git and process endpoints. `Command` is inspected, never spawned. Synthetic regular files in an owned scratch directory supply workflows. This does not test the actual Git service, process service, shell-path integration, or production safe-read implementation.
- **M — reviewer manual callbacks:** the exact archived options builder installs stdout/stderr callbacks, which the harness calls directly. A recording endpoint also returns identical buffered stdout to detect final duplication. This does not establish actual pipe arrival or GUI repaint timing.
- **W — supplied writer evidence inspected, not rerun:** the exact candidate-labelled logs report 12 focused tests passed / 1 ignored; qualified lint failed; further-qualified lint passed. These labels are supplied provenance, not an independently attested build chain.
- **O — official protocol source:** act source pinned to `2540ba6c49dfbafe167d655e4044921cd2ed19e7`. Six locally preserved payloads match the prior provenance manifest. The relevant official raw sources were also opened read-only during R2. Compatibility conclusions below are source inferences, not installed-act execution results.

Independent harness command: `cargo +1.95.0 run --offline --manifest-path /tmp/gitcraft-act-r2-harness/Cargo.toml`. **Exit 0**. Log: `/tmp/gitcraft-act-r2-harness.log`. Git, process service, egui, shell path and safe-read are explicitly stubbed. Compilation and the pure harness program ran; no actual Git/act/HTTP/process-control execution fixture ran.

## Per-finding verdicts

### R1-1 — CLOSED: isolate unmatched matrix includes

Candidate `src/repo/local_ci/model.rs:160–203` collects unmatched additions in `extras` and appends them only after processing every include. Later includes therefore visit only surviving original combinations. The combined 256-entry bound still counts extras.

The independent harness reran the adversarial excluded-Linux/reintroduced-Linux/later-`bar` case: Windows receives `bar: two`; the extra Linux row retains `foo: one` without `bar`. The exact selected snapshot round-trips that row unchanged. The official six-entry include example also passes. This matches the deferred-extra structure of [act's pinned matrix expansion](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/model/workflow.go#L409). Candidate tests at `tests.rs:317` cover both cases. Evidence: **S + P + W + O**. This closes the reported mismatch without claiming all YAML/act matrix semantics were exhaustively tested.

### R1-2 — CLOSED: validate, display, bind and revalidate repository context

Candidate `context.rs:11–43` requires a cached configured remote and accepts only direct GitHub.com HTTPS, `git@github.com:` or `ssh://git@github.com/` forms with a plain owner/repository. Credentials, parameters, custom ports, aliases and enterprise/other hosts are refused with an explicit supported-scope explanation. `context.rs:45–49` displays the accepted fetch URL, host and repository. This is lexical configuration validation, not an online repository-existence check.

`mod.rs:339–352` invalidates trust when displayed context changes. `mod.rs:455–471` rejects a changed cached identity at Run, and `mod.rs:495–510` retains the context in provenance and the worker selection. `run.rs:262–264` then calls the existing unattended Git API with a 20-second timeout. Its exact query is `git config --local --no-includes --get-all remote.<name>.url`; changed, missing/error and multiple nonempty URLs refuse the act endpoint. The read is intentionally conservative for include-only, rewritten or otherwise nonliteral configuration. Raw Git error text is replaced by a generic error. The API passes no typed credentials; shared credential transport is not certified here.

`run.rs:133–145` supplies `--remote-name <name>`, `--github-instance github.com`, `--env GITHUB_REPOSITORY=<owner/repo>` and `--env GITHUB_REPOSITORY_OWNER=<owner>`. Official source confirms these are registered flags and explicit assignments enter runner configuration. [CLI parsing and environment loading](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/cmd/root.go#L77) supports this argv. [Runner context construction](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/runner/run_context.go#L950) seeds the repository from configuration and selects host URLs. [Repository fallback](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/model/github_context.go#L168) only consults the remote when the explicit repository is empty; it recomputes owner from that repository. The owner flag is consistent but redundant for that method. Thus, by source inspection, the pin prevents the original silent `nektos/act` fallback for these repository fields.

The independent recording harness confirms accepted/rejected URL forms, exact query and timeout, refusal before the act endpoint, preserved argv, and pins remaining unchanged after a synthetic post-validation remote edit. Writer tests add headless stale-display refusal before worker submission. Evidence: **S + P + R + W + O**.

The revalidation is not atomic with launch. Git config, files, refs, actrc or the executable may change afterward, and event JSON may intentionally carry separately supplied repository fields. The bound argv protects the reported repository/owner/host correction; it does not freeze the worktree, lock Git config, prove every action's network destination, or guarantee arbitrary installed act versions. No real Git revalidation or act context was executed under this hold.

### R1-3 — CLOSED: shared default for hosted and local entry paths

`ci_view/mod.rs:124` calls `local_ci_context_default` from the shared `open_ci` entry point. `local_ci/mod.rs:405–430` chooses current-branch push remote, then origin/first-remote fallback, and returns early for an explicit override or pending operation. The text editor marks an explicit override at `mod.rs:334–337`. `local_ci_ui` also refreshes late-loaded defaults and clears trust when a default changes.

Hosted `Action::OpenCi` and local `Act::Open` both reach `open_ci`. The candidate headless test at `tests.rs:431` checks hosted-first default, override retention, and local entry. That supplied test was inspected, not independently rerun; this reviewer independently traced both paths and the late-render update. Evidence: **S + W**. Hosted project routing remains independent.

### R1-4 — CLOSED: bounded live stdout callback, no duplicate final output

The process delta is exactly the claimed narrow port: `Options.stdout: Option<Chunk>`, default `None`, `consume`, and stdout dispatch in the normal-ready, post-ready-marker and stopped-drain paths (`processes.rs:31,54,480,497,627,685`). The helper is byte-identical to process-owner commit `fb56988503377bd8cfcd36cc35c0cfc63c36c931`; field/default and all three dispatch calls match it. Mechanically removing just those changes restores the entire parent `processes.rs` byte-for-byte. No other supervisor protocol changes were imported.

`run.rs:314–334` installs both streams into the bounded shared log and requests repaint. `run.rs:297–309` no longer appends completed stdout a second time. Normal act job logging targets stdout in the [official logger](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/runner/logger.go#L95).

Independent callbacks update the log before the recording endpoint returns; identical buffered stdout is not duplicated after return. Repeated 8 KiB callback chunks keep text within 2 MiB and rows within 32768; options retain the 8 MiB stream-capture limit. Candidate `tests.rs:467` supplies additional direct callback coverage. Existing consumers default to no stdout callback, bounded capture remains, and Act uses neither stdout-file redirection nor a remote-ready marker. Evidence: **S + R + M + W + O**. Real scheduling, buffering, drain completion, cancellation, output-overflow termination and platform timing remain untested.

## Adjacent regression and supplied-gate assessment

Eleven neighboring routing/lifecycle/task files have the same blobs as R1: `src/ci/route.rs`, `src/repo/ci_status.rs`, `src/ci/mod.rs`, `src/repo/ci_view/page.rs`, `src/repo/apply.rs`, `src/repo/palette/mod.rs`, `src/repo/lifecycle.rs`, `src/repo/actions.rs`, `src/repo/messages.rs`, `src/tasks.rs`, and `src/connections.rs`. The hosted view's only correction is default initialization. Static review and the recording harness preserve explicit worktree/snapshot selection, stale-workflow refusal, token-variable removals and local-only result identity. This is focused regression evidence, not a full application regression pass.

The three supplied logs are represented exactly:

1. `...-0.log`: `cargo +1.95.0 test --lib repo::local_ci:: -- --test-threads=2` — **12 passed, 1 ignored**, with pure/model/recording/headless/manual-callback scope. The actual owned fake-act subprocess fixture remains ignored.
2. `...-1.log`: `cargo +1.95.0 clippy --lib -- -D warnings -A dead_code` — **FAIL, exit 101**, setup-imported `src/app/clone.rs:224` `collapsible_if`.
3. `...-2.log`: same lint with additional `-A clippy::collapsible_if` — **PASS**, only as this further-qualified lint. No strict or original qualified-lint pass is claimed.

The known shared credential-helper redirect/LFS/path/helper-precedence FAIL remains setup-owned and open. This review neither certifies it nor relabels it as a new Act-specific finding. Earlier credential-fill and fake-child execution described in the response is historical supplied evidence; none was rerun or used to clear the current hold.

## Remaining holds

Actual subprocess/Git/act/Docker/HTTP/credential-fill execution fixtures, kill/cancel/timeout fixtures and native process service tests were not run. No full suite, CI, PR action, external write, commit, push, subagent or parent/writer file edit occurred. Network activity was read-only retrieval of issue/protocol sources, not HTTP execution fixtures.

Genuine unresolved gates remain: the independently held process supervisor; the separate credential transport blocker; actual installed-act compatibility and repository context; streaming arrival/pipe behavior; real Docker images, lifecycle and retained resources; Linux/Windows/native platform behavior; full combined checks and final-candidate independent acceptance. The cancellation UI continues to disclose that containers/services may remain. **R2 closes the four implementation findings only.**
