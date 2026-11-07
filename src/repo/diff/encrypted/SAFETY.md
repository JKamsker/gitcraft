# SOPS recovery and local-only safety contract

Independent static review passed the exact local-only deletion/safety checkpoint
`ee6e8482a09c27a2c40286996433bd1bd996b3d0`; evidence is
`/workspace/work/review-sops-static-ee6e848.json`. Root accepted that review and
explicitly authorized a separately preserved guard-removal unit. Both compile-time
guards were removed in the separately pushed checkpoint `bd851ed75b092b447a4981c713d0d4b134b6624c`.
The reviewed containment fix is integrated through `ff901aad4346ee409509b60f9c6e7b12689fde44`.
Local-only owned-child execution was explicitly authorized after those checkpoints.
Static safety acceptance does not replace independent feature review.

Do not run a host watchdog, live process-table selector or actual remote fixtures.
Unset `GITCRAFT_TEST_SSH` and `GITCRAFT_TEST_WSL`. The baseline
`remote::session::tests::stopping_ends_a_whole_tree` now uses only a synthetic table
and recording command functions; default full-suite gates need no safety skip.
Only one assigned Cargo slot may be used, with a private target and
`CARGO_INCREMENTAL=0`. Independent exact-head feature review is still required.

## Removed architecture

The unsafe host watchdog and its executing fixture are deleted. There is no SOPS
host process-table query, PID-list signal loop, private SSH/WSL plaintext runner,
or remote plaintext framing. The remote transport sources contain only reviewed
green code, including the separate baseline test-containment change.

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
Plaintext remains in local bounded pipes and the opt-in view.

Cancellation uses only the existing reviewed `bounded_process::run_input` contract,
which owns its child handle and isolated local process group. No remote cancellation
selector is needed by this design.

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

The inert validation remains available without executing application/process fixtures:

```text
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_sops_safety.py -v
```

It uses explicit tables, 216 small ancestry graphs, recording-only intents, source
AST capability checks and reviewed local-only boundary assertions. It does not inspect live
processes or execute the production SOPS request, tool, transport or cleanup code.

## Validation and delivery

Use only dummy inputs, test-owned children and the reviewed local launch containment.
The optional real-tool fixture requires explicit `GITCRAFT_TEST_SOPS` and
`GITCRAFT_TEST_AGE_KEYGEN` executable paths. It generates its own age keys and isolates
HOME, XDG configuration and inherited credential environment. No user key is needed.
Without those variables the test reports that real-tool validation was not run.

Run formatting, strict all-target Clippy and full private-target default tests;
preserve exact-source logs and independently review the immutable feature candidate.
Commit and push every completed unit before another unit or risky test. One final
giant PR remains gated on all required combined tests and independent review.
