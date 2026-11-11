# Candidate assembly and validation helpers

These gzip archives preserve exact helper bytes, including line endings. Their
original and compressed hashes are in the manifest. They are stored on E: and
were independently reviewed as source before execution. Archiving is not execution.

`assemble-validated.py` accepts the complete accepted source SHA, the fixture-lint
source SHA, the UI source SHA, then the ordered CI comparison source unit SHAs.
It starts a new branch from the pinned dev base, preserves published WIP branches,
checks size before each commit, verifies authorship and each pushed SHA, and
requires the final source tree to exactly match the accepted combined source.
The saved `assembly.json` is its prior immutable baf6 candidate provenance input.

`run-final-candidate.py` requires an explicit immutable candidate SHA and a clean
checkout. It binds shell bytes, source tree, application/test hashes and commands.
It runs full Python discovery, size/policy snapshots, strict all-package Linux and
Windows cross-target Clippy, default test builds, synthetic recording preflights,
the isolated recording harness, full runtime suites and isolated special cases.
Real app-process tests run in a probed PID/proc/dev namespace. The two existing
owned ACL metadata fixtures run natively in private Linux directories. Compiler,
build and tooling scratch is on E:. Windows cross checks do not execute Windows.

Neither helper dispatches CI or creates a PR. Actual final results and their
candidate binding must be recorded separately after these checks finish.
