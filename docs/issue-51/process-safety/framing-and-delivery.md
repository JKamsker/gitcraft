# R2: frame boundaries, output disposition and fallback reap

WIP static correction, no process runtime gate. Both brokers now read one bounded complete frame and retain the byte-stream remainder, including coalesced retirement ACK + next request/ping. Pure recording cases invoke the actual readers across every split. There is still no dispatch retry after an ambiguous write.

Output cleanup proof is now separate from delivery proof. Terminal receipts include a strict `complete` boolean. The owned target's actual code stays in its receipt; truncated capture, missing pipe EOF, forwarding failure, overflow and pending output after the finite drain set delivery incomplete and force a non-success transport result. Pooled callers inspect that receipt before success. Native/standalone streaming callers authenticate completion through dedicated cleanup capacity; a stopped/reaped target with incomplete or unconfirmed delivery becomes an explicit error. Confirmed cancellation retains its expected partial output. Receipt validation for stopping/recovery still accepts `complete:false`, since delivery does not change whether the target was stopped.

Perl double signal refusal now still attempts its exact owned-child reap and preserves both errors. The recording backend covers the double refusal without real signals.

Rust recording filter: 11 application + 6 OS gates passed on Rust 1.95.0 with CARGO_BUILD_JOBS=2; the final receipt assertion was rerun before preservation. `python3 -B src/remote/supervisor/recording_tests.py`: 7 tests passed. `perl -c src/remote/supervisor/owner.pl`: syntax OK. `perl src/remote/supervisor/owner.pl recording`: 11 gates passed.

Remaining R2 blockers include bounded setup/ping/request/ACK writes and handshake emission, pinned retirement directory identity, absolute native socket budgets, caller uncertainty propagation and exclusive workspace claims/checked mutation cleanup. Static FAIL remains; no owned fixture/full suite/CI was run.
