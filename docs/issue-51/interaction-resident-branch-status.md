# Resident watcher: branch checks and status delta

This completes the original tray source checklist's current-branch check and meaningful
status paths. It does not release any native, process or network runtime hold.

- The checked-out branch's CI plan uses the existing #172 push-destination router, with at
  most four projects for its validated HEAD. Push destination precedes a distinct tracking
  upstream. Unsupported forges/detached or invalid HEAD remain explicit absent observations.
- A first observation is silent. A known branch changing to failing, or a new failing commit
  on that same branch, produces “Checks failed on <branch>”. Missing results preserve the
  previous baseline. A branch switch establishes its own baseline rather than reporting an
  unrelated previous branch's transition.
- The actual disabled tray status row receives the latest repository transition, including
  when desktop notifications are disabled. The notification preference controls OS delivery.
- Root-authorized single-file CLI correction 3dfe5cbc8f2b14665a2c08007a1a9236e1f8ed49 was
  adopted exactly as c8c65ef555a3fb58c55049fb4563455158b8d774. Its parent blob matched this
  branch, and the imported blob is identical to the owner's. The current branch compiles
  against that existing Service API. Independent bounded STATIC PASS is recorded in
  /tmp/gitcraft-51-forge-cli-bounded-review.md. Preparation still precedes the Service timeout;
  end-to-end tray timing and all native/runtime limits remain separate. No acceptance of the
  supervisor implementation or native null-sink behavior is inferred.

The new pure regression calls the actual branch-plan adapter with a fork push destination
and distinct upstream, then routes a recorded failing completion through Watcher::tick into
the actual tray menu adapter. It asserts both destination order and visible status while OS
notifications are disabled. The nine tray pure/recording tests run no Git, provider, helper,
D-Bus or native icon. Immutable logs/source hashes are external in
`/tmp/gitcraft-interaction-resident-branch-status-validation.json`.
