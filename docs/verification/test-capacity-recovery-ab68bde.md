# One controlled full-suite recovery attempt — failed

On 2026-11-07 the orchestrator explicitly authorized one recovery validation
using the existing accepted default-test binary at immutable application head
`ab68bdedd387236c03f26f839572860d405a2f7f`. No Cargo compilation or CI ran.
The earlier compact smoke/evidence unit was committed, pushed, and remote-SHA
verified as `d7d0816b2c67c7a74994269d5fb9338a7699c86c` before this attempt.

The accepted binary SHA-256 and the existing full/list log SHA-256 values
matched `/workspace/work/integration-ab68bde-gates.json`. Integration was clean
at the accepted head before and after. The new JSON binds the accepted gate,
binary, previous logs, exact command/environment, and new log hashes.

The ordinary supervised shell executed the binary once, saved its exit
status, remained alive for three seconds, and exited with the saved status.
Tini used only `-s -w -v`. SSH/WSL live-test variables and Tini behavior
variables were unset; no Git config overrides were added. Test concurrency
was two and Rayon threads two. The accepted private target was retained.

Result: **failed, exit 134**, after 5.7 seconds. The harness registered all 948 cases for execution, but no full-suite
result summary was produced. Before the abort, the log recorded 470 `ok` and
199 `FAILED` case lines; captured individual failure reasons were not emitted
before termination. An unnamed
notify 8.2.1 thread unwrapped `SendError` in `inotify.rs:584:32` and again in
`inotify.rs:635:51`; a panic in destructor cleanup aborted the binary. The
captured logs contain no EAGAIN resource-error message establishing that as
its cause. This is not evidence of restored full-suite capacity and does not
supersede the prior accepted 945-pass/3-ignored baseline gate.

Containment observation: visible process counts were 31,317 before and after;
visible zombie counts were 31,301 before and after (delta zero). Tini logged
82 adopted-zombie reaps. This included only descendants of the supervised
command. No PID inspection, process selection, signaling, or foreign cleanup
was performed; only count snapshots were read. The test orchestration hold
remains in force and no retry was performed.

The three-second grace only permits normal short-lived descendants to finish.
It cannot guarantee draining arbitrary descendants that remain alive when
Tini's direct shell child exits. Existing PID 1 zombies cannot be recovered
by this process. Diagnose the notify abort separately before releasing broad
fixture execution. Safe bounded compilation, if separately authorized, still
needs private targets and constrained jobs/threads; this attempt did not run
or establish compilation capacity.

Raw evidence: `test-capacity-recovery-ab68bde.json`,
`test-capacity-recovery-ab68bde.test.log`, and
`test-capacity-recovery-ab68bde.tini.log` in this directory.
