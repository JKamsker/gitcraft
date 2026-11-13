# gitcraft agent notes

## Forge

- Forge: **github** (`api_base`: `https://api.github.com`).
- Canonical checkout aliases: `origin` = upstream `Vancete/gitcraft` (read-only for us); `fork` = `JKamsker/gitcraft` (admin) — issues and PRs go to the fork.
- Tooling: `gh` (pass `-R JKamsker/gitcraft`).
- Worktree aliases can differ: verify configured URLs before use. All writes and PRs target `JKamsker/gitcraft`.

# Work preservation

- Commit **and push** every completed unit before starting the next unit or any risky test. Verify the remote branch SHA equals the local commit, and report whether the unit is WIP, reviewed, or validated.
- Keep each agent's work on its assigned branch/worktree. Do not modify another agent's WIP.
- **Never rebase or force-push a pushed branch.** To bring a branch up to date with `dev` (or resolve PR conflicts), merge `dev` into it (`git merge origin/dev`) and push normally. Published history is never rewritten.

# Commit authorship

- Use exactly `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`, matching commit `028242cd4bc3c82ced848ea0fa73639981cd71d7`, for this Codex recovery and PR #164 follow-up task. For unrelated work, use the contributor's configured author identity. Verify the commit author before pushing. Do not use the earlier session email.
- Preserve published WIP/recovery branches. Build the final PR branch from `dev` with one commit per feature under this author identity; keep provenance and verify the final source matches the accepted combined implementation.

# CI failures

- Before pushing, run `python3 ci/check.py` (add `--test <filter>` for what you changed). It runs CI's exact gates.
- When CI fails, inspect the exact failed job and test first: `gh run view <id> --log-failed`, the `CI result` summary, and the `test-report-*` artifacts. Reproduce it locally, make a scoped fix, rerun the check, then push once. Never edit on a guess, and never raise timeouts or skip tests to get green. A passing re-run does not prove a failure harmless. See `docs/ci.md`.

# CI timing

- Do not start, dispatch, re-run, or wait on GitHub CI during implementation. Keep work-branch pushes outside CI triggers. Finish all implementation, final history assembly, local checks and independent adversarial reviews first; only then use CI for the final candidate.

# Final delivery

- Deliver one final consolidated PR. Do not open it until full combined checks/tests and an independent adversarial review pass on the exact final candidate.
- Preserve test and review evidence bound to immutable commits. For documentation-only changes, bind the prior application gates after verifying application files are unchanged; do not claim new test execution.

# Local builds

- Run every local Rust toolchain command (`cargo build`/`check`/`test`/`clippy`, `rustc`) at the lowest OS priority so it never competes with the user's work. Windows: `cmd /c start "" /low /b /wait cargo build --release` (child `rustc`/linker processes inherit the idle class). Unix: `nice -n 19 cargo …`.
- On Windows, antivirus may block freshly compiled build-script executables under `target/debug/build`. When `target/release` already holds build scripts that ran, prefer the `release` profile: cargo reuses them instead of compiling new ones. This is a workaround only — after `cargo clean` or a dependency change they are compiled again and may be blocked too.

# Process-test safety

- Never derive signal targets from host-wide PID searches/selectors or send host-wide signals. Use owned child handles/process groups with verified ownership, or isolated process namespaces.
- For new process-control logic, first validate synthetic process snapshots and recording-only signal/command checks. Run real owned-child tests only after those checks pass and containment is established.
- Preserve the current unit with a commit, push, and verified remote SHA before any risky process test. Do not use watchdogs or cleanup commands that can target unrelated processes.

# Rust source size and maintainability

**Rust source-file size:** Try to keep each Rust source file below **300 physical
lines**. **Never exceed 500 physical lines in any tracked `.rs` file**, including
tests, examples, build scripts, and any generated or vendored Rust files committed
to this repository. Count comments and blank lines. If a change would exceed the
cap, refactor before completing it. Prefer composition, clear ownership, and
reusable helpers. Do not evade the cap with minified code, giant inline
expressions, arbitrary file splits, partial types solely to reduce line counts,
or moving the same oversized responsibility into another hierarchy level.
Run `python3 ci/rust_file_size.py` before committing (and `--staged` to check the
complete index). CI checks the complete candidate with `--revision HEAD` and must
error whenever any tracked Rust file exceeds 500 lines. There are no per-file
exemptions or grandfathered oversized Rust files. Lexical complexity metrics
remain advisory; this physical file-size policy is mandatory.
