# Confirmed stopped tests may have no log

Root's original macOS suite at `19b39cae1e39fd7c84048e719e2eb9f0379958b4` failed linear/merge searches at parallelism eight, the fewer-rounds comparison, and the both-ends cleanup assertion. Original evidence remains read-only under `/tmp/gitcraft-51-local-suite-19b39cae`; the JSON record hashes its first 1,990 lines, including the original failures.

Retained local ledgers contain authenticated stopped dispatch receipts with code 125 / incomplete output. Corresponding cancelled slots have no log because cancellation can precede wrapper dispatch. `runner::run` confirmed recovery, then unconditionally ran `tail` on the missing path. Its stderr was discarded, producing `Err("")`; the caller invalidated the workspace and retained worktrees.

The production result path still requires authenticated owner recovery before examining stopped results or reading a log. For a confirmed stopped/timed-out test, reading the log now verifies its parent can be entered, accepts genuine basename absence as empty output, rejects links/nonregular entries, and reports read errors. Non-stopped completion uses delivered output. Unknown recovery and other process failures remain errors. An otherwise empty bisect-shell failure now includes its exit status.

Five pure recording cases cover stopped/timed-out absent logs after recovery, ordering, unknown recovery without log access, nonregular/read failures, retained partial output, and normal delivered output. All-target compilation and the application build pass (existing warnings remain). Actual runtime is held pending root suite completion and preservation/remote verification of this correction; the source record contains exact inert commands and log hashes.

The crashed-bisect fixture now creates worktrees and generation refs through real Workspace operations, verifies durable v3 owner evidence, proves an active claim refuses takeover, and releases only the completed fixture's lease to exercise subsequent recovery. It fabricates no process ownership. A separate legacy fixture proves ancient plaintext evidence is refused and retained; fixture-only completed artifacts are explicitly removed after those assertions.

AI ask and Git stream failures have a separate cause: inherited-open output deliberately reports an incomplete result after bounded drain/owned cleanup. Their old success expectations are renamed and replaced with explicit failure/bounded-return assertions; stream still verifies its delivered line. No production success, cancellation, or uncertainty policy is changed for those callers. AI's error API does not expose its partial answer bytes.

This unit does not add a supervisor framework, signal selector, watchdog, transport, or root-only fixture port. Unix anchor production already matches root19. Integration must retain root19's existing runner connection-scope inheritance. Native Windows/Linux/SSH execution and full-suite acceptance are not claimed.
