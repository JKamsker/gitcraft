**Request changes — one new P2 blocker** on `4cc6a9f970fd8b13aacb059409b9473cc6a147d6`, against base `cea7dfb5ddcc290352958b63566cfd3e515a0d9f`. Reviewed only the delta after `2743133d`.

**P2 — Reset recheck rejects valid timeline restores after an intentional checkout.**
[operations.rs:970](/Users/jonas/.t3/worktrees/gitcraft/t3-1a34bd60/src/repo/operations.rs:970) rechecks every original symbolic-ref expectation immediately before `reset --keep`. However, [snapshot.rs:183](/Users/jonas/.t3/worktrees/gitcraft/t3-1a34bd60/src/repo/timeline/snapshot.rs:183) captures the starting branch, then intentionally checks out the destination branch before resetting it.

Minimal reproduction:

1. Make commits F1 and F2 on `feat`, recorded in gitcraft’s timeline.
2. Check out `main`.
3. Jump to the timeline entry after F1.

Restoration generates expectations for `HEAD → main` and `feat → F2`, followed by `checkout feat` and `reset --keep F1`. The new recheck rejects the reset because HEAD now points to `feat`, reporting “feat is checked out now, not main.” The restore fails after already changing the checkout.

This regression comes directly from the new shared reset check. Restricting it to the guarded paths requiring it, or tracking intentional checkout changes, would preserve the protection without rejecting this restore.

**Both original review20 blockers are resolved:** local undo expectations retain worker-owned R, and checkout replay retains canonical identity and effective destination pins.

**Verification:** independently ran **64 targeted tests; all passed**, including all nine review20 regressions and 20 drafts tests. Binary SHA-256 hashes and relevant candidate source blobs were verified. The new finding is established by source tracing; I did not execute its application reproduction.

Limits: parent full gates remain parent-owned and incomplete at last inspection. No edits, builds, delegation, CI, PR, pushes, or live forge writes. Checkout remains clean.
