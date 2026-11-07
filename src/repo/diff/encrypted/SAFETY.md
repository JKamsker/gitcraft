# SOPS recovery safety checkpoint — not runtime validated

Both `mod.rs` and `tool.rs` retain their `compile_error!` recovery quarantine. Do not
remove either guard or run Cargo, SOPS, watchdog, process-kill, or transport fixtures
until an independent reviewer has approved this static unit and the next isolated
execution step is explicitly authorized.

## Removed architecture

The unsafe host watchdog and its executing fixture are deleted. There is no SOPS
host process-table query, PID-list signal loop, private SSH/WSL plaintext runner,
or remote plaintext framing. `src/remote/mod.rs` and `src/remote/session.rs` are
restored to reviewed green `4fcb0dfe226f30785dd2758a2959ff4379daa8ed`.

The outage bug was not just an incomplete membership filter: AWK reads of
`selected[parent[pid]]` created zero-valued entries, and iterating all keys emitted
unselected ancestors and potentially PID 0/1. Merely changing that final filter
would still leave live host PID reuse, reparenting and ownership races. Do not
reintroduce that architecture or connect a process-table parser to signal calls.

## Simpler decryption boundary

The proposed feature reads ciphertext using the existing reviewed, Practice-aware
Git transport. SOPS and its keys are local to the UI computer even for SSH/WSL
repositories. The tool boundary accepts a format and ciphertext, never a repository
host/path; it builds a local `sops` command with fixed arguments and a neutral local
working directory. Local key-file environment overrides should use absolute paths.
Plaintext would remain in local bounded pipes and the opt-in view.

Cancellation uses only the existing reviewed `bounded_process::run_input` contract,
which owns its child handle and isolated local process group. This static checkpoint
does not execute or newly validate that code. No remote cancellation selector is
needed by this design.

## Inert selector regression model

`tools/sops_safety.py` is a pure synthetic model, not imported by the application.
It takes a fixture launch receipt and an immutable registry of opaque owned process
handles. A live root, a distinct newly-created session/group, ownership, unambiguous
identity and registered ancestry are required. PID-label 0/1, ancestors, unrelated
branches, foreign ownership, containment escapes and cyclic/missing ancestry cannot
be selected. Duplicate handles/PID labels and ambiguous root data fail closed.
Only opaque handles leave the selector. A recording sink stores stop intents and
has no OS adapter, signal callback, process lookup, subprocess or I/O capability.

The receipt is a synthetic premise, not self-authenticating production authority.
Passing these tests never authorizes converting handles to host PIDs or attaching
this model to live signals. Actual authority must come from the launcher's retained
OS child/session/job containment capability, independently reviewed for the platform.

Run only the inert validation at this stage:

```text
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_sops_safety.py -v
```

It uses explicit tables, 216 small ancestry graphs, recording-only intents, source
AST capability checks and quarantine/locality assertions. It does not inspect live
processes or execute the production SOPS request, tool, transport or cleanup code.

## Next gates

1. Commit and push this completed static unit; verify its remote SHA.
2. Obtain independent static review of the synthetic/recording behavior and deletions.
3. With separate authorization, validate local execution using explicitly owned children
   and fixture tools only. No global host PID table or process selector may be signaled.
4. Remove quarantine only when authorized, then run formatting, strict Clippy and the
   full private-target suite; independently review the immutable feature candidate.
5. Commit and push every completed unit before another unit or risky test. The final
   giant PR remains gated on all required tests and independent review.
