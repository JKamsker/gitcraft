**Request changes — two P2 findings** on `2743133d690874512716c2037b6ab0f9c11e6a5a`, against `cea7dfb5ddcc290352958b63566cfd3e515a0d9f`.

1. **Successful-push undo can still remove an unrelated commit.**
   [operations.rs:681](/Users/jonas/.t3/worktrees/gitcraft/t3-cfbc5b5f/src/repo/operations.rs:681) fills undo’s local-ref expectation from disk after pushing, while only the remote lease is bound to the actual pushed OID.

   Reproduction: rebase H→R; pause the push with a blocking pre-push hook; commit C externally on the same branch; release the hook. Git correctly pushes pinned R, but undo now expects local C. Ctrl+Z passes its guards and resets C to H. Bind the local undo expectation to the recorded rewrite/push result, preserving rejection when the branch moves afterward. Redo uses the same late filling at line 693.

2. **Fork checkout redo bypasses the destination protections.**
   [forks/mod.rs:258](/Users/jonas/.t3/worktrees/gitcraft/t3-cfbc5b5f/src/repo/forks/mod.rs:258) stores checkout’s plain commands as undo/redo; its captured scope is cleared after success. [timeline/tab.rs:457](/Users/jonas/.t3/worktrees/gitcraft/t3-cfbc5b5f/src/repo/timeline/tab.rs:457) replays them without recreating that scope.

   Reproduction: successfully check out Ann’s fork, undo, configure an `insteadOf` rewrite from Ann’s advertised HTTPS prefix to another repository containing `fix`, then redo. Redo adds, marks, fetches and checks out that other repository without canonical destination validation. Persist or reconstruct checkout’s destination checks for replay.

Both findings are established by source tracing; I did not execute these application reproductions.

**Verification:** independently ran **61 targeted tests; all passed**, using the SHA-256-verified immutable binary. Relevant fork, operation and surrounding tested sources match this candidate. Parent full gates remained incomplete at last inspection.

Checkout remains clean. No edits, builds, delegation, CI, PR, pushes or live forge writes.
