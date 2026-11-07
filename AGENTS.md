# gitcraft agent notes

## Forge

- Forge: **github** (`api_base`: `https://api.github.com`).
- Canonical checkout aliases: `origin` = upstream `Vancete/gitcraft` (read-only for us); `fork` = `JKamsker/gitcraft` (admin) — issues and PRs go to the fork.
- Tooling: `gh` (pass `-R JKamsker/gitcraft`).
- Worktree aliases can differ: verify configured URLs before use. All writes and PRs target `JKamsker/gitcraft`.

# Work preservation

- Commit **and push** every completed unit before starting the next unit or any risky test. Verify the remote branch SHA equals the local commit, and report whether the unit is WIP, reviewed, or validated.
- Keep each agent's work on its assigned branch/worktree. Do not modify another agent's WIP.

# Commit authorship

- Use exactly `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`, matching commit `028242cd4bc3c82ced848ea0fa73639981cd71d7`, for this Codex recovery and PR #164 follow-up task. For unrelated work, use the contributor's configured author identity. Verify the commit author before pushing. Do not use the earlier session email.
- Preserve published WIP/recovery branches. Build the final PR branch from `dev` with one commit per feature under this author identity; keep provenance and verify the final source matches the accepted combined implementation.

# CI timing

- Do not start, dispatch, re-run, or wait on GitHub CI during implementation. Keep work-branch pushes outside CI triggers. Finish all implementation, final history assembly, local checks and independent adversarial reviews first; only then use CI for the final candidate.

# Final delivery

- Deliver one final consolidated PR. Do not open it until full combined checks/tests and an independent adversarial review pass on the exact final candidate.
- Preserve test and review evidence bound to immutable commits. For documentation-only changes, bind the prior application gates after verifying application files are unchanged; do not claim new test execution.

# Process-test safety

- Never derive signal targets from host-wide PID searches/selectors or send host-wide signals. Use owned child handles/process groups with verified ownership, or isolated process namespaces.
- For new process-control logic, first validate synthetic process snapshots and recording-only signal/command checks. Run real owned-child tests only after those checks pass and containment is established.
- Preserve the current unit with a commit, push, and verified remote SHA before any risky process test. Do not use watchdogs or cleanup commands that can target unrelated processes.
