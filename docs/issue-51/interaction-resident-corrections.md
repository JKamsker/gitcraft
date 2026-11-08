# Resident tray: bounded R1 corrections

This unit addresses exactly TRAY-R1-1 and TRAY-R1-2 from the independent review of
`da2e421d60907e0d3ac374439074f3893f96037e`.

- Parsed `--quit` handoffs reach the existing exit path even when Git is unavailable.
  Git-dependent launch commands retain their prerequisite. The headless regression
  uses the actual handoff and resident close reducer with a recorded missing Git.
- Each subscribed path retains an existing `tasks::Scope` for notification delivery.
  Profile/immutable Context changes, removed paths, opt-out, disabled notifications
  and watcher destruction retire the relevant scopes. Queued tasks share those tokens
  and recheck cancellation immediately before the platform notification callback.
  The recording adapter executes that same final delivery closure: an unchanged owner
  delivers once; all six invalidations suppress the queued private title.

Validation is limited to ten tray pure/recording tests, the missing-Git headless
regression, and a macOS all-target compile check. Exact commands, immutable source
hashes, exits and log hashes are recorded outside the repository at
`/tmp/gitcraft-interaction-resident-corrections-validation.json` after preservation.
No native tray, OS notification, Git/helper/network runtime, full suite or CI ran.
The existing shared-supervisor and end-to-end timing limits remain unchanged.
