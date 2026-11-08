# Windows suspended ownership ordering

WIP static/recording checkpoint; no Windows process was launched.

Production Windows launch now executes the same generic ordered driver used by recording tests: spawn suspended, assign the retained child's HANDLE to a kill-on-close Job, then resume. Assignment failure kills/reaps only the retained child. Resume failure first closes its assigned Job, then kills/reaps that child. Spawn failure has no cleanup target. The Job stays held through normal process completion and final drain. Assignment no longer reopens a process by PID; the permitted suspended-primary-thread snapshot is used only to find the resume thread.

`CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked -p gitcraft-os recording -- --nocapture`: 4 recording tests passed, including every driver success/failure boundary. `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked -p gitcraft-os --target x86_64-pc-windows-gnu`: passed. Native Job runtime, Git-for-Windows bisect, controller crash/recovery and the independent candidate review remain pending.
