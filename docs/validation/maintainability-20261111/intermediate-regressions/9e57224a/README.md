# Failed combined candidate run

Candidate `9e57224a584fcebfeeb6f8d9b80a6d5160dd82e6` passed size/policy
checks in all three modes, 61 Python tests, formatting, all-target compilation,
strict three-package Linux and Windows cross-target Clippy, test builds, ten
process recordings, 27 recovery recordings, 24 OS tests and all integration suites.
The main library run failed: 5,205 passed, two failed, 27 ignored, four filtered.
The four isolated tests and documentation tests were not reached.

Both newly added zoom/profile fixtures dropped egui frames with unapplied texture
deltas. Epaint panicked before behavior assertions. The correction explicitly
disposes those headless frame outputs, retaining the zoom precondition, post-action
zoom/profile assertions, recorded profile-application count and generalized loops.
This is historical failed-candidate evidence, not validation of the later fix.

The initial shell preflight and assembly continuation records preserve the
separate checkout/Git-configuration repairs without source normalization. The
successful shell repair report verifies already canonical bytes on its second
invocation; it does not record the original CRLF byte hashes. Source reviews found
no production blocker but do not supersede these failed execution results.
