# Local Actions R1 adversarial review — FAIL

Candidate: **052721ed3c36853ad97c34485dbef65950f7ea38** (`t3/issue51-batch-b`). Base: **270a8a3b33402847f6d04cd8b8e70d625f33f4cb**. Scope: the local Actions feature from [issue #43](https://github.com/JKamsker/gitcraft/issues/43) only. Rich-diff regressions, GitLab WIP and the six setup issues are outside this verdict.

**Verdict: FAIL. Four P2 implementation findings remain.** This is an actual implementation, not a stub: it builds a selected workflow snapshot, constructs an act invocation, submits it through the process service and presents local results beside hosted CI. That does not establish real act/Docker execution or clear the separately held process-supervisor review.

## Immutable source and evidence

The review used `git show` / `git diff` for the exact candidate and a `git archive` extraction at:

`/var/folders/0z/snd05v_n21jd6pkhvg2h0vlc0000gp/T/gitcraft-act-r1-052721ed-ygjmojz0`

All 11 changed files in that archive were compared byte-for-byte with `git show 052721ed…:<path>`. Source blob IDs, SHA-256 digests and evidence hashes are in `/tmp/gitcraft-51-act-review-r1-evidence.json`.

Process exception: one final reference-collection command accidentally read source paths from the writer's live worktree. Its source output was discarded; the references were recollected from the archive and the archive was checked against the commit. No tracked files were edited. The writer's newer implementation is not reviewed here.

Evidence classes:

- **S — static:** exact archived application source and diff; current official act source pinned to `2540ba6c49dfbafe167d655e4044921cd2ed19e7` in `/tmp/gitcraft-act-r1-upstream/provenance.json`.
- **P — reviewer-executed pure model:** an external harness imports the exact archived `model.rs`, with `yaml-rust2 = 0.13.0`. Command: `cargo +1.95.0 run --offline --manifest-path /tmp/gitcraft-act-r1-model-harness/Cargo.toml`. Log: `/tmp/gitcraft-act-r1-model-harness.log`. Exit 0; confirms the ordinary official six-entry include example and the adversarial mismatch below.
- **R — reviewer-executed recording only:** an external harness imports exact archived `model.rs` and `run.rs`. Its process endpoint records `std::process::Command`; it never spawns a child. egui, shell-path and process-service dependencies are stubs; safe-read uses only owned synthetic regular files. Command: `cargo +1.95.0 run --offline --manifest-path /tmp/gitcraft-act-r1-recording-harness/Cargo.toml`. Log: `/tmp/gitcraft-act-r1-recording-harness.log`. Exit 0. This is not process-service, full application, symlink-safety or act-runtime evidence.
- **W — supplied writer evidence inspected, not rerun:** `/tmp/gitcraft-51-batch-b-validation.json` and its candidate-labelled logs show 7 local-CI tests passed / 1 ignored; the separately invoked owned fake-act test passed; qualified clippy passed with `-D warnings -A dead_code`. The fake test records arguments/stdout/stderr and refuses an edited workflow. It does not execute act or Docker, exercise cancellation, or prove containers ran. The log labels identify the candidate; they are supplied evidence, not an independently attested build provenance chain.

No actual act, Docker, workflow, native process supervisor, process cancellation/kill test, repository CI, full suite, subagent, forge write, commit or push was invoked by this reviewer. Network reads were limited to the original issue and official protocol/documentation sources.

## Findings

### R1-1 — P2: later includes mutate an extra matrix row that act leaves unchanged

Source: `src/repo/local_ci/model.rs:168–194`, especially the immediate `entries.push(addition.clone())` and subsequent scans of all entries against the original Cartesian product.

Safe repro, executed against the immutable model:

```yaml
strategy:
  matrix:
    os: [linux, windows]
    exclude: [{os: linux}]
    include:
      - {os: linux, foo: one}
      - {bar: two}
```

The selector produces Windows with `bar: two` and Linux with both `foo: one` **and `bar: two`**. act expands this as Windows with `bar: two`, followed by the extra Linux row with `foo: one` **without `bar`**: it retains unmatched include rows separately until all includes have been applied. See [act's pinned matrix expansion](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/model/workflow.go#L409). The GitHub explanation also distinguishes original combinations from added combinations in its [include example](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations#expanding-or-adding-matrix-configurations).

The pure harness also confirms that the altered Linux row survives into the single-entry snapshot, so this changes the executed matrix context rather than only a label. A step conditioned on `matrix.bar` can take a different path. Keep extra include rows separate until expansion finishes, then add a regression covering exclusion/reintroduction followed by another include. Evidence: **S + P**.

### R1-2 — P2: an unchecked remote can silently produce a fallback repository or the wrong GitHub host

Source: `src/repo/local_ci/run.rs:123–148` and `:249–287`; `src/repo/local_ci/mod.rs:317–321`, `:439–454`.

The remote is a free-text name checked only for empty/leading-dash/control characters. No configured-remote lookup or URL/host validation occurs before invocation. The recording harness accepts `missing-context` on an owned worktree with no Git repository, reaches the command endpoint, and confirms that no `--github-instance` is supplied.

Two concrete protocol consequences:

1. A nonexistent/renamed remote is not a preflight failure. act's [repository-context code](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/model/github_context.go#L168) substitutes `nektos/act` after a remote lookup error, logging the explanation at debug level. A credential-free echo job could complete under that unrelated context.
2. A valid enterprise remote still gets act's default GitHub.com server/API context. act requires an explicit [`--github-instance` hostname](https://nektosact.com/usage/github_enterprise.html); its [server/API construction](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/runner/run_context.go#L1030) is independent of the remote name. The UI does not disclose this restriction or refuse unsupported hosts.

The fetch-URL/pushURL distinction is disclosed and is not itself a defect. However, neither resolved URL/host nor a confirmed repository slug is shown or retained in run provenance, and a remote-name binding can change while queued. Generic “GitHub context differs” text does not make fallback to an unrelated repository understandable. Validate and show a sanitized effective context, bind/revalidate it at execution, and either support the explicit enterprise host or refuse it clearly. Preserve the intentional separation from hosted CI routing. No selected-account token disclosure was demonstrated. Evidence: **S + R + official protocol source**; no real act reproduction claimed.

### R1-3 — P2: opening the hosted CI pane bypasses the push-remote default

Source: `src/repo/local_ci/mod.rs:45–59`, `:375–390`; `src/repo/ci_view/page.rs:19–21`; `src/repo/apply.rs:81–91`; `src/repo/palette/mod.rs:259–271`.

Safe static/UI-path repro: use a fresh tab whose current branch pushes to `fork` while `origin` is upstream. Open “Show CI for HEAD” (or a hosted run) first. `Action::OpenCi` calls `open_ci` directly; the local controls are then rendered, but `State::default()` still contains `origin`. Only the separate “Run Actions locally…” action initializes the local remote from `refs.push_remotes`.

Consequently, the local pane beside hosted fork CI initially runs with upstream context despite the claimed push-remote default. Initialize the default consistently on both entry paths, while preserving an intentional user override. This is a local-context defect; the hosted #172 routing code itself remains unchanged. Evidence: **S**; no newly executed full UI test claimed.

### R1-4 — P2: normal job output is unavailable throughout a running act job

Source: `src/repo/local_ci/run.rs:290–308`; `src/processes.rs:477`, `:507` in the immutable candidate.

Only the stderr callback appends output and repaints while the command runs. Stdout is captured by the process service and appended after its synchronous `run` returns. act's normal [job logger writes to stdout](https://github.com/nektos/act/blob/2540ba6c49dfbafe167d655e4044921cd2ed19e7/pkg/runner/logger.go#L95).

Thus a long-running job can show startup diagnostics but no step logs until completion, timeout or cancellation. The default timeout is 30 minutes. The writer's fake test asserts output only after completion and cannot detect this. Provide a bounded stdout stream/callback into the local log, retaining bounded capture and honest termination behavior. Validate with recording callbacks or an isolated, owned fake writer only after the separate process-safety gate permits it. Evidence: **S + official logger source**; no runtime test invoked.

## Requirement coverage at this SHA

| Requirement | Assessment and evidence |
|---|---|
| Pick workflow/event/job/matrix | Substantially implemented. Catalog reads local workflow files, enumerates events/jobs, and snapshots a selected entry. S/W; R1-1 blocks full matrix compatibility. |
| Static matrix, include/exclude, prerequisites | Cartesian/static cases and ordinary include/exclude covered by source and writer tests; official six-entry include example passed in the pure harness. Needs closure retains prerequisite jobs and all their matrix entries; unrelated jobs removed; missing dependencies and cycles rejected. R1-1 remains. |
| Dynamic matrices and aliases | Explicit errors name expression matrices/axes/values/includes and YAML aliases. Conservative unsupported selection is disclosed, not claimed as executed. Dynamic prerequisite matrices remain in prerequisites for act to evaluate; no runtime success claim. S/W. |
| Current worktree and exact command | Separate argv, absolute worktree/snapshot paths, explicit event/job/remote, payload, images and optional architecture; no shell command concatenation. One-entry private snapshot avoids loose matrix filtering. R confirms spaces, argv, private files and timeout clamp. Actual CLI compatibility remains untested. |
| Stale workflow/source/selection | Exact workflow source is reread before snapshot/command creation; event/job/matrix membership is revalidated; reload resets selection/trust. R/W confirm changed source refuses before launch. This does not freeze every worktree file; current application files are intentionally used when act runs. Remote-context binding is deficient under R1-2. |
| Environment, secrets and account context | Explicit empty env/secret/var/input files; no automatic selected-account token retrieval; named forge-token variables removed. R inspects those removals. Workers inherit connection/cancellation context, and State rejects a result after a profile change (W). These facts do not constitute a security sandbox or guarantee workflow/container options cannot access host resources. |
| actrc refusal | Private invocation directory avoids repository `.actrc`; nonempty ambient HOME/XDG/macOS/APPDATA paths are inspected and refused. Synthetic HOME `.actrc` refusal passed R. No exhaustive cross-platform runtime gate. |
| Remote defaults and understandable host/repository | Palette path uses push remote and clearly distinguishes fetch URL from pushURL. Fails R1-2 and R1-3. |
| Reachable local runs beside hosted CI | Palette entry exists without a recognized forge; shared pane displays separate local identity/output; writer headless test passed. No hosted run ID/check is fabricated. S/W. Streaming deficiency R1-4 remains. |
| Hosted #172 destination routing | Static preservation: `src/ci/route.rs`, `src/repo/ci_status.rs`, `src/repo/ci_view/mod.rs`, `src/ci/mod.rs` unchanged from base; local state does not replace hosted project ownership. No new hosted regression-test execution claimed. |
| Output/time bounds | 2 MiB/32768-row display cap, bounded line wrapping, 8 MiB per-stream process capture cap and 1–120 minute timeout configured. Unicode single-chunk bound test passed W. The 8 MiB cap stops execution; this is not unlimited logging. S/W/R, not runtime timing proof. |
| Cancellation/container honesty | Cancel signals the owned task scope; profile/tab lifetime cancels through existing machinery. UI and stopped-result text explicitly say containers/services may remain and request inspection; no automatic global Docker cleanup is promised. W checks scope/result behavior only. Native cancellation correctness is not cleared. |
| Unsupported act behavior / execution claims | UI names macOS/Windows runner differences, permissions/OIDC/environment/concurrency/timeouts/continue-on-error/context/event limitations and missing secrets/private/reusable-workflow problems. Success says “Local act run completed,” explicitly separate from hosted CI; no claim Docker/Linux/Windows passed. S. |

## Genuine remaining runtime gates

1. **Existing global process-review FAIL remains held.** This review neither reruns nor clears it. Cancellation, timeout, output overflow, profile switch during a running child, tab closure and pipe draining still require the authorized synthetic/recording-first process review and later isolated owned-child checks. Do not run host-wide PID selection or cleanup.
2. After that gate is cleared and the implementation fixes are preserved, validate an explicit supported installed act version with a throwaway owned fixture: one event/job/matrix plus prerequisites, current edited files/local actions, exact remote/host context and streamed output. No real repository CI workflow should be used for that test.
3. Actual Docker execution, runner image behavior, socket isolation, normal cleanup and retained containers after cancellation are **not tested here or by the supplied fake-act gate**. Cancellation must continue to disclose possible retained resources; complete container cleanup is not an acceptance promise invented by this review.
4. No Linux, Windows or native cross-platform/container success is claimed. No fresh full combined suite, final-candidate CI, final-history assembly or later GitLab/rich-feature review is supplied by this local-only R1.

Route R1-1 through R1-4 to the writer, preserve fixes and evidence on new immutable commits, and request a fresh independent review of that candidate. Do not treat the seven unit checks or fake tool as a real act/Docker pass.
