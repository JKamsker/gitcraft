# Prompt & session — independent adversarial R1

**FAIL / incomplete original requirement: five concrete findings.** Review only, exact candidate `e64aaa786576a4cbedaf6a0b9a97b2ba03973113`, tree `79a340592a05190006d9d3c87521584df8527c77`. No writer/root source was edited. This is not certification or reassessment of the unrelated Batch C features, credential policy, history recovery or process supervisor.

Source references below mean files at this candidate, frozen under `/tmp/gitcraft-prompt-review-e64aaa78.HLP9M0/source`. Archive SHA-256: `71896bdd6a589e5ce4142221fcf6fbdd98b188b0483897ac14b2374012cf967e`. All 1,364 archived blobs were independently checked against their immutable Git blob IDs. Original #48 was read from the supplied issue JSON; only **The prompt behind a commit** is in scope. The later user instruction to converge and avoid speculative hardening governs this review.

## Findings

### PS-1 — P1: normal Entire hook publication receives no checkpoint disclosure warning

**Candidate:** `src/push_preflight.rs:662–687`; `src/git/prompt_session/mod.rs:164–183`. Reachability: `src/git/command.rs:1603–1615`; `src/git/push_hooks.rs:38–46`.

The new warning recognizes named checkpoint sources/destinations or an exact OID alias of a checkpoint ref. It does not recognize checkpoint publication initiated by the original pre-push hook. That original hook is deliberately retained and invoked by the existing push wrapper unless hooks were explicitly skipped.

**Trigger:** an Entire-enabled repository using automatic checkpoint synchronization, a normal code branch push to the elected checkpoint remote, and checkpoint changes waiting to synchronize. The code branch and its destination are ordinary `refs/heads/main`; its OID differs from the checkpoints branch OID. The new predicate returns false. Entire's pre-push hook can then push `entire/checkpoints/v1` (or per-checkpoint refs) before the code push completes, without this dialog warning about transcripts. A dedicated checkpoint remote can also be the recipient. This is an ordinary integration route, not an arbitrary malicious hook scenario.

**Primary evidence:** Entire's [`PrePush` implementation](https://github.com/entireio/cli/blob/17108c4550e4e39887b679b6f0b3424dec057556/cmd/entire/cli/strategy/manual_commit_push.go#L43) documents and implements synchronization from the Git hook, including the `push_sessions` setting and separate checkpoint destination. The candidate wrapper passes through the original hook. Independent pure test `exact_checkpoint_ref_predicate_misses_normal_entire_hook_publication` confirms the missing warning condition; it does **not** execute either hook or a push.

**Correction:** include possible Entire automatic checkpoint publication in the push dialog when its hook/integration and settings can cause it. A clearly conditional warning is sufficient where the precise checkpoint set cannot be determined without execution. Retain exact-route/OID warnings. This does not require executing arbitrary hooks during preview or implementing a general hook sandbox.

### PS-2 — P2: supported trailerless Entire attachments are completely invisible

**Candidate:** `src/git/prompt_session/entire.rs:59–67,103–125,220`; caller `src/git/prompt_session/mod.rs:300`.

The Entire reader starts exclusively from `trailers(message)`. Without an `Entire-Checkpoint` trailer it never looks at the local checkpoint store, and does not explain that a supported attachment lookup was omitted.

**Trigger:** attach an Entire session to a commit already held by a remote. Entire preserves that commit and writes a root `linked_commits` entry naming the exact SHA. Selecting the commit here shows no Entire session despite its available checkpoint. This defeats a practical way to obtain the prompt behind an existing commit; it is not a request to guess authorship from timestamps or import anchors.

**Primary evidence:** Entire's [`WriteOptions.LinkedCommits` contract](https://github.com/entireio/cli/blob/17108c4550e4e39887b679b6f0b3424dec057556/api/checkpoint/metadata.go#L154) defines this exact commit link; root `CheckpointSummary.linked_commits` is at lines 615–616. The writer explicitly acknowledges omitting it. Static control flow proves the omission: no trailer means zero iterations, independently of any adapter or native runtime behavior.

**Correction:** add bounded local discovery of exact-SHA `linked_commits`, displaying them as **unverified recorded links**, consistent with Entire's CLI. Report any bounded lookup omission. Import-only `commit_sha` anchors have weaker meaning: do not promote those to authored-commit evidence. This review does not require every imported checkpoint to acquire attribution.

### PS-3 — P2: the claimed compact fallback silently loses supported prompt/edit evidence

**Candidate:** `src/git/prompt_session/entire.rs:185–209`; `src/git/prompt_session/parse.rs:82–92,143–150,258–275,287–291,320–329`.

When raw `full.jsonl` is unavailable or over 4 MiB, the reader tries Entire's compact transcript through the same parser. That handles some top-level user/assistant records, but does not fully read the compact format it claims to fall back to:

- Compact user content may be an array of `{id, text}` blocks **without a `type` field**. `text()` requires `text`/`input_text`/`output_text` as the block type, so that recorded prompt disappears without an unsupported-format notice.
- Compact timestamps use `ts`, ignored here.
- Compact tool blocks named `apply_patch` go through `tool()`, whose edit allowlist is only `Edit`, `MultiEdit`, `Write`; the stored patch is never parsed for hunk navigation.
- Compact output carries no recorded `cwd`. A Claude absolute edit path from the original worktree cannot be made relative to another clone/worktree's current root. Its content is shown but its hunk link is unavailable, without distinguishing absent origin context from a nonmatching edit.

**Primary evidence:** Entire's pinned [`compact.go`](https://github.com/entireio/cli/blob/17108c4550e4e39887b679b6f0b3424dec057556/transcript/compact.go#L27) defines the output fields and user block shape; its [format specification](https://github.com/entireio/cli/blob/17108c4550e4e39887b679b6f0b3424dec057556/transcript/README.md#L60) documents the typeless user text blocks and tool-use representation. Three independent pure cases reproduce the omitted prompt and unavailable compact edit paths; the timestamp loss is asserted in the compact Codex case. No conversion program or actual transcript was run.

**Correction:** explicitly parse the supported compact schema (including typeless prompt blocks and `ts`), route supported recorded patch tool input to the patch reader, and distinguish missing origin context from content mismatches. Preserve a visible unsupported/unmapped tool step when exact path mapping cannot be justified. The 4 MiB raw-file limit may remain; the bug is losing usable bounded fallback evidence, not refusing unlimited transcript/chunk reads.

### PS-4 — P2: unchanged interior lines are presented as hunk edit candidates

**Candidate:** `src/git/prompt_session/mod.rs:203–224,237–242,355–361`; `src/repo/prompt_session.rs:126–130`.

`changed_matches()` removes a common prefix and suffix, then treats everything between them as changed. It does not exclude unchanged lines inside that interval. Parent-side matching independently succeeds even when a later step changed those interior lines in the selected commit.

**Trigger:** an `Edit` replaces a multi-line string, changing its first and last lines while retaining 38 middle lines. Another step changes a middle line in a separate commit hunk. The first edit's entire before-string still occurs uniquely in the parent. Its computed old-side span is lines 1–40, so the middle hunk's old line 20 matches that first edit too. Clicking the middle hunk can jump first to a tool step that did not change it. The label “content match” does not cure this: the step did not change the candidate line, and the issue specifically asks for the step that wrote the hunk.

**Independent evidence:** `changed_span_includes_unchanged_interior_lines` produces `Span { start: 1, end: 10, old: true }`; `unchanged_interior_yields_candidate_for_another_steps_separate_hunk` passes that span through the exact `hunk_steps()` function and gets the unrelated endpoint-only edit for `(20,20)` in a 40-line fixture (sufficient separation for ordinary three-context-line diff hunks). These are in-memory exact-source checks, not an application/runtime fixture.

**Correction:** compute actual changed runs inside the recorded before/after strings and match hunk changed lines against those runs. Preserve ambiguity labels and refusal for repeated/redacted snippets. An honest unknown is acceptable where a recorded edit cannot establish the association; an unchanged interior must not become an attributed changed run.

### PS-5 — P2: valid Codex file-deletion calls disappear instead of remaining visible as unmapped steps

**Candidate:** `src/git/prompt_session/parse.rs:158–180,194,353–360`.

`patch()` recognizes `*** Delete File: ...` but only flushes a step if `before` or `after` has content. A normal deletion patch supplies the filename without embedding the removed file bytes. Both buffers stay empty, so the entire tool invocation is discarded, with no notice.

**Trigger:** the selected commit deletes a file through a recorded Codex `apply_patch` deletion. The prompt/session panel contains no deletion tool step, and its deletion hunk cannot reach that step. This occurs even for a tiny, unredacted raw transcript in the exact worktree/time window; it does not depend on chunk limits or uncertain shell-write attribution.

**Independent evidence:** `codex_delete_patch_call_is_dropped_entirely` supplies session metadata and the standard `*** Delete File: file` tool call to the exact parser: zero steps, zero notices. The existing parser explicitly recognizes this patch header, so an unsupported-version theory is unnecessary.

**Correction:** preserve a deletion tool step with its validated path. If available context cannot establish exact removed content, label the step unmapped instead of inventing line attribution. A visible tool step with an honest unavailable hunk match meets the evidence requirement better than silently dropping it.

## Per-requirement coverage

| Original requirement | Exact candidate coverage | Assessment |
| --- | --- | --- |
| Selected commit's Prompt & session tab | Real tab in `src/repo/detail/commit.rs:32–44`; explicit read action; result pinned to immutable OID; request generation and worktree/profile callback checks in `src/repo/prompt_session.rs:68–114`. | Implemented at source/headless level; no independent native UI run. |
| Prompts, agent summary, tool calls | Commit-message text preserved with honest GitButler ambiguity; raw Claude and legacy Codex message/tool records parsed; Entire prompts and structured summary read. Commands/tool output/reasoning omitted. | Substantive, partial: compact prompts/edits and deletion calls lost (PS-3/5). Assistant text is labelled summary; no generated summary is claimed. |
| Entire checkpoints branch source | Pins per-checkpoint ref or legacy local v1 branch to immutable OID; checks root/session checkpoint IDs; supports numbered session paths and raw/compact offsets. | Partial: PS-2/3. No demand for exhaustive agent/version support. Import anchors must remain distinct from attribution. |
| GitButler prompts in messages | Splits first blank line and preserves the full remaining message. GitButler simple mode emits precisely prompt + blank line + change summary. | Reasonable practical coverage. No reliable marker exists in that layout; ordinary messages remain explicitly unverified. Primary: [GitButler simple.rs](https://github.com/gitbutlerapp/gitbutler/blob/3d879f8c25efb7042e88d38f5cd4173544d0a9d1/crates/but-action/src/simple.rs#L93). |
| Local Claude/Codex, matched worktree + time | Opt-in roots; canonical current worktree and 1–168-hour-before/10-minute-after window; separate candidate sessions; bounded JSONL discovery and safe reads. | Implemented for parsed raw shapes; correlation is correctly labelled unauthenticated. This is not proof of every upstream version. |
| Clicking a hunk jumps to its writing step | Diff header Prompt step button, old/new ranges, candidate links/highlighting and scroll-to-step; ambiguous/absent evidence yields no asserted attribution. | Partial: PS-3/4/5. Conservative unknowns for shell writes, repeated or redacted snippets are acceptable. |
| Transcripts shown locally only | Local reader/state; remote repo reading refused; no transcript upload/MCP/export/agent call introduced by this module. Redaction and read/display limits present. | Source-supported viewing policy. No fresh native filesystem/process/network or comprehensive confidentiality-runtime claim. |
| Push dialog warns when checkpoint history would be pushed | Dedicated transcript finding and acknowledgment UI for named checkpoint routes and exact tip aliases. | Fails normal Entire automatic push route (PS-1). |

## Evidence precision, accepted limits and inherited dependencies

The feature is real, not a scaffold. Explicit limits and conservative unknown attribution are generally appropriate. The omitted >4 MiB raw/chunk payloads are reported; this review does not require unlimited reads, shell-write attribution, reconstruction of redacted content or exact authorship from time/path alone. Bounds on source bytes are not a claim that total process memory is capped at 32 MiB. Failed object reads during hunk mapping are currently converted to unavailable matches (`mod.rs:342–352`); no stronger object-availability guarantee is accepted.

An additional pure probe shows that the purported final-trailer parser accepts a prose paragraph ending in an `Entire-Checkpoint:` line. Entire's own final-trailer grammar rejects that paragraph unless its trailer-block conditions hold. Treat the “exact trailer” wording with that limitation. This is retained evidence, not a separate gating demand or a call for exhaustive trailer/version compatibility. Probe: `final_paragraph_with_prose_is_incorrectly_accepted_as_exact_trailer`; primary pinned `cmd/entire/cli/trailers/trailers.go:574–602`.

Both prior Batch C reviews and their manifests were read/copied. Their rejected working-transaction, deferred-cwd, credential/account and process prerequisites are **not duplicated as prompt-specific findings**. This reader reaches the shared `forecast_bytes → draft_bytes → local_command/processes` service (`src/git/command.rs:1373–1453`), so the new module's allowlisted read arguments do not independently validate that supervisor or native PATH discovery. Likewise PS-1 uses the existing hook wrapper only as source reachability evidence; it does not certify the surrounding push/credential implementation. No unrelated batch is accepted.

## Independent checks versus supplied evidence

**Independent:** an isolated `/tmp` Cargo harness compiled only the five prompt-session production modules, with Git/file/remote adapters replaced by panic-only inert stubs. All five module files are byte-identical to the archive except a declared test-module footer appended to the harness copy of `mod.rs`. No root/writer source was changed. The harness cannot call production Git/process/app constructors through those adapters. It ran 10 existing pure tests plus 8 independent cases: **18 passed, 0 failed**, final log `independent-pure-final-corrected-fixture.log`. It excludes the repo UI module and is not an application compile/test gate. Some independent assertions deliberately confirm existing defective behavior; “18 passed” means the observations reproduced, not feature acceptance.

An initial `cargo test --offline` attempt used Cargo 1.84 and stopped at manifest parsing (edition2024 unsupported), before tests. Re-running with the installed explicit Rust/Cargo 1.95 paths compiled and ran 16 checks; after adding two focused evidence cases, an 18-check run passed. The separate-hunk fixture was then lengthened from 10 to 40 lines so its middle change is separated from both endpoint changes under ordinary three-line diff context; the corrected final 18-check run passed. All earlier logs are retained; the short fixture alone is not evidence of a separate UI hunk. No network dependency resolution occurred (`--offline`).

**Supplied, separately preserved:** `/private/tmp/gitcraft-c-e64aaa78-prompt-pure.log` reports the author's 11 focused tests passed, including a headless panel test; `/private/tmp/gitcraft-c-prompt-check6.log` reports compilation success with warnings. The user identifies the first as immutable exact-candidate evidence. They are copied and hashed under `supplied/`; they are not relabelled as independent executions. The logs themselves do not include a source-OID/executable receipt, so this review does not infer stronger executable provenance from their filenames. No actual runtime was claimed or added.

**Held / not executed:** actual Git-under-test repositories or fixtures; helper/CLI/HTTP/SSH/PTY/agent/native fixtures; live push/pre-push; cancellation/timeout/signal tests; App/git_cmd/local_command construction; real transcript discovery; full suite; CI; PR creation; commits/pushes; subagents. No host PID selectors or signals were used. Git was used only to read the given commit/tree/archive/diff. Public HTTP GETs fetched primary docs/source for research, never a transport fixture. macOS/Windows/WSL/remote UI, process containment and native filesystem behavior remain unexecuted here.

Manifest: `/tmp/gitcraft-51-prompt-session-review-r1-manifest.json`. Detailed file hashes, harness sources/lockfile/logs and command inventory remain in `/tmp/gitcraft-prompt-review-e64aaa78.HLP9M0`. Required correction is bounded to PS-1–5; this review requests no implementation changes, new review rounds or further runtime activity under the hold.
