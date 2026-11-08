# Native protocol budgets — WIP checkpoint

Native stop uses one five-second absolute budget across connect, nonblocking partial writes, reads and flushes. The helper admits at most four nonblocking control clients, each with a 100 ms absolute frame deadline, and keeps inspecting its owned execution ticket while clients are incomplete. Final acknowledgement writes are bounded separately after cleanup.

Windows resume now checks the retained process HANDLE with zero-time `WaitForSingleObject`, in addition to querying the opened thread HANDLE's owner. A process that exited with code 259 cannot be mistaken for a live owner.

Recording-only gates: `CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked --lib recording -- --nocapture` passed 16 application and 6 OS tests. The new injected reader/writer gate covers drip reads, partial writes and a blocked flush under one budget. `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked --all-targets` passed with existing dead-code warnings. `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked -p gitcraft-os --target x86_64-pc-windows-gnu` passed. `git diff --check` passed.

No process fixture, native helper, SSH/WSL request, Windows runtime, full suite or CI was run. Independent static review is still required; other R2 blockers remain open.
