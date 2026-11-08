# Receipt-gated artifact retirement

WIP static/recording checkpoint. No process runtime fixture has run; independent review remains required.

Pooled requests acknowledge only a fully validated terminal receipt and the complete frozen binary output. The broker retires the preceding private endpoint on that exact-nonce acknowledgement over the same connection; passive reads never reconnect for retirement. Standalone ephemeral streams attempt fixed authenticated retirement after their local transport has finished and cleanup is confirmed. Durable bisect records retain their endpoints until workspace cleanup succeeds. Retirement validates the atomic receipt, unlinks only the fixed private input/control/terminal names, and removes only that exact private directory. No retirement operation signals a process. Failed/unknown cleanup retains evidence; failed artifact retirement leaves private temporary files rather than inventing signal authority.

The cleanup Service now has an explicit shutdown after remote shutdown. This process's packaged Unix anchor image is unlinked by its exact stored path only after all owners shut down. Native helper exit follows the same ordered shutdown. This is private artifact cleanup, not a host scan or watchdog. Abrupt supervisor/app death or failed authentication can intentionally retain artifacts for recovery; no global temporary-directory sweeper is introduced.

Gates (CARGO_BUILD_JOBS=2, Rust 1.95.0):

- `cargo +1.95.0 test --locked --lib recording -- --nocapture`: 8 application + 4 OS tests passed.
- `cargo +1.95.0 check --locked --all-targets`: passed with baseline macOS dead-code warnings.
- `python3 -B src/remote/supervisor/recording_tests.py`: 5 tests passed, including zero deletions without receipt and exact fixed artifact targets.
- `perl -c src/remote/supervisor/owner.pl`: syntax OK.
- `perl src/remote/supervisor/owner.pl recording`: 8 recording gates passed, including retirement ordering.

Native Windows/Linux/SSH/WSL, canary, EOF/crash recovery, saturated output, inherited focused tests and full-suite validation remain pending. This checkpoint adds a defaulted `Options::retire_remote` field and two app-shutdown calls; existing Service call sites remain source compatible.
