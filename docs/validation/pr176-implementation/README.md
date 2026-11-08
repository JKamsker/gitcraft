Evidence for PR #176 integration

Application candidate: `76a1eab871b314dad0fe0a2f1a087c92c3dd5034`.
Prior test candidate: `68801c148fd2727f9c50d3dfff3d03c94b1d2584`.
Subsequent changes remove three redundant production borrows in the atomic replacement helper and propagate regression fixture errors instead of using unwrap. The final standalone atomic helper test was rerun and passed; prior logs retain their original commit binding. All other production source is unchanged.
The manifest binds the compressed logs to this immutable source commit. This evidence commit changes documentation only; it does not claim a new application test run.

The candidate merges #176 with `--no-ff`, retaining the base localization and owned-process service. It fixes process-service execution/cancellation, file-over-ID completion selection, temporary-file collisions and long names, raw recovery archives, raw-byte park freshness, hidden-index restoration guards, and restored-run selection without deleting the other parked run's durable workspace. Existing published source branches were not rewritten.

`cargo clippy --locked --tests -- -D warnings` and formatting passed on the final application candidate. On the prior test candidate, `cargo check --locked --tests`, 18 recording-only OS ownership tests, four resolver process tests, done-file selection, and run-record roundtrip passed. Recording-only process checks and the standalone bounded-line test passed before real owned-child tests; the candidate was pushed and its remote SHA verified first. Standalone tests compiled the actual production `Lines` and atomic replacement functions copied verbatim from the candidate, avoiding GUI dependencies; the full application also compiled afterward. No host-wide PID selection or signal command was used.

The parking group compiled and ran, but all 27 cases failed during fixture setup because the host's Git 2.43.0 rejects the base application's `--no-lazy-fetch` option. These are environment-blocked checks, not parking behavioral passes. The parent must run parking/workspace/restore-selection regressions with a compatible Git, plus final combined integration checks and independent review.

Parking now backs up all tracked regular files, including files Git filters make appear clean. This increases archive storage and I/O. Dropping a merge preserves the complete raw/index/state archive under `gitcraft/parked-recovery/<id>/parked` inside the repository's Git metadata; the recovery branch description records its absolute location.
