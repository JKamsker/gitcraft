# Failed combined candidate run

Candidate `7511b6fc12f5b6184c3a061d9810c0ad7983bd5a` passed size/policy
checks in all three modes, 61 Python tests, formatting, all-target compilation,
strict three-package Linux and Windows cross-target Clippy, test builds, both
repaired zoom/profile fixture preflights, ten
process recordings, 27 recovery recordings, 24 OS tests and all integration suites.
The main library run failed: 5,223 passed, one failed, 27 ignored, four filtered.
The four isolated tests and documentation tests were not reached.

The accessibility graph fixture panicked because its named semibold font was not
bound. It installed fonts inside the first UI callback; queued font definitions
were not yet active when startup data arrived quickly enough to paint that frame.
One contained exact-test diagnostic against the same frozen binaries passed,
establishing timing dependence without retries or replacing the failed full run.
It did not fail a keyboard-focus or author-label assertion. Both previously
repaired zoom/profile tests also passed in this full run. These are historical
failed-candidate results, not validation of a later initialization correction.

The current-dev assembly, stopped stale-base guard and independent source reviews
are preserved. The approved archive helpers were not executed because this run
failed; their source reviews do not claim final application success. The separate
recording-only archive-helper regression has 61 override cases, an in-memory
negative control and restored success. Source reviews do not supersede this
failed application execution result.
