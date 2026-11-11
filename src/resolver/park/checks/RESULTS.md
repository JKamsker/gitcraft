# Recovery re-review recording validation

Baseline: `90e05f40` (integration checkout). Before editing, both owned source
subtrees (`src/resolver` and `src/repo/resolver`) were byte-for-byte Git tree
equivalent to revised `46a4cae9935c978cc317d9b93a24f9d1ef448489`.
Original `9cd34bd9e08084aa642040d331df1b72dc50c5b0` is incorporated by that
revised implementation, not an ancestor (`merge-base --is-ancestor` exits 1).
The original recovery evidence remains in this file's parent history.

The containing fix commit binds this evidence to immutable source. SHA-256 of
sorted changed Rust paths relative to the baseline, each encoded as path + NUL +
index blob bytes + NUL (14 paths):

`ed450fb51ef81263689d7ffdd223484e6a220ad540196fa9cf9187985fe4e514`

Command (isolated harness only, not the application Cargo project):

```
RUSTFLAGS='--cfg recovery_recording' cargo test --offline \
  --manifest-path src/resolver/park/checks/Cargo.toml -- --nocapture
```

Final candidate result: **12 passed, 0 failed**, in 165.71 seconds; doc tests:
0 passed, 0 failed. Ordinary injection and abrupt stop sweeps each cover 90 park,
107 restore and 26 drop boundaries (223 each). The actual cross-device test ran,
with no unavailable-filesystem skip. Complete output, including expected caught
recording panics: [re-review.log.gz](re-review.log.gz).

Production parking modules compile directly against recording Git boundaries.
All filesystem writes are inside owned temporary directories; no child-process,
signal, actual Git integration test, full application build, CI, push or PR
operation was run. The Linux cross-device test confirmed that archive-to-live
rename fails with `CrossesDevices` between owned `/dev/shm` and `/tmp` locations,
then successfully restored raw bytes using destination-side publication.

Coverage includes independent callers executing production list/bring_back/drop
while the foreground is stopped at Restored and archive-moved boundaries. Locks
remain held across movement, busy callers refuse mutation, repeated restart
listing reaches intact archives, vanished-source receipt writes fail, and
journal-only ghosts cannot shadow retained archives. Checks assert original ref
removal and the exact recovery ref, raw/index/operation bytes after completion.

Required raw/index/metadata source removal fails before any checkout, Restoring
receipt or plan publication; repairing each source permits retry. A symlink source
also fails preflight. An ignored file on a path added by the archived merge blocks
restore and retains binary/CRLF bytes. A preexisting staging directory is untouched;
a new edit at the durable destination-temp barrier refuses publication.

Both ordinary error injection and abrupt recording stops exercise actual
production effect boundaries. Abrupt mode panics past explicit production error
handlers, preventing their cleanup from masking the stop; RAII closes handles.
It is not an actual no-unwind process termination or power-loss test. Restart
uses production list, bring_back, drop_to_branch and (for incomplete reservations
with verified original live bytes) explicit forget. Expected guarded restart
failures are asserted, never silently ignored; uncertain index locks remain
untouched, and Drop retains exact bytes and completes ref cleanup.

The application flow test now deliberately asserts retained completed discovery,
archive contents, agent workspace cleanup, and ref cleanup after application.
The raw-byte integration test explicitly checks no plan/phase mutation on missing
source preflight. Those two application tests were edited but not executed here;
combined application compilation/regressions remain integration gates.

`rustfmt --edition 2024 --config skip_children=true --check` and staged diff
whitespace checks pass for all 14 changed Rust files; maximum is 464 lines.
Both mandatory full-tree size checks pass: 2579 tracked Rust files, 761 advisory
warnings, zero hard-limit violations (including newly staged files).

Status: local recording validation; independent review and combined application
acceptance remain pending. See [RECOVERY.md](../../RECOVERY.md) for operational
and platform limits. OS file locking uses the standard library and no PID-based
ownership. Linux independent-handle contention and real cross-device raw restore
are covered; cross-process execution, Windows locking/rename/directory durability,
filesystem/device power loss and concurrent external Git/editor writers are not.
External writers must be quiescent; byte checks cannot atomically exclude a write
between the final comparison and rename. Interrupted private staging directories
are preserved, never adopted or overwritten on restart.
