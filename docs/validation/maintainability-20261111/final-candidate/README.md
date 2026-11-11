# Final candidate local validation

Application candidate: `0e6a8e52d1f16e19122a674b314eef70abeb58b9`. Complete source tree: `1ae8537f6c9d734e1d9194472d897c09b0dd5654`.
This tree equals accepted combined source `83f25dc09327b0e0db9436ccdb2c815dc17cea68`.
The assembled history contains 21 feature units from dev
`57d7395bce9e187ff9f2a3ce9b96dc0d65715600`, with preserved source provenance.

All 33 local gates passed. Full Python discovery ran 61 tests successfully.
Working-tree, complete-index and committed-revision size and complexity checks,
formatting, all-target compilation, and strict Clippy for all three tracked Cargo
packages passed. Linux execution and Windows GNU cross-target Clippy are distinct:
the cross check did not execute native Windows tests.

| Runtime gate | Passed | Failed | Ignored | Filtered |
| --- | ---: | ---: | ---: | ---: |
| frame-font-readiness | 1 | 0 | 0 | 5255 |
| frame-accessibility | 1 | 0 | 0 | 5255 |
| frame-layout-profile | 1 | 0 | 0 | 5255 |
| frame-settings-profile | 1 | 0 | 0 | 5255 |
| recording-pipes | 6 | 0 | 0 | 5250 |
| recording-outcomes | 3 | 0 | 0 | 5253 |
| recording-resolver | 1 | 0 | 0 | 5255 |
| recovery-recording | 27 | 0 | 0 | 0 |
| runtime-gitcraft_os-lib | 24 | 0 | 0 | 0 |
| runtime-standalone_cli-standalone_cli | 3 | 0 | 0 | 0 |
| runtime-mcp_stdio-mcp_stdio | 6 | 0 | 0 | 0 |
| runtime-checkpoint_cli-checkpoint_cli | 1 | 0 | 0 | 0 |
| runtime-gitcraft-main | 0 | 0 | 0 | 0 |
| runtime-gitcraft-lib | 5225 | 0 | 27 | 4 |
| bisect-isolated | 1 | 0 | 0 | 5255 |
| acl-save | 1 | 0 | 0 | 5255 |
| acl-index | 1 | 0 | 0 | 5255 |
| palette-isolated | 1 | 0 | 0 | 5255 |
| doc-tests | 0 | 0 | 0 | 0 |
| doc-tests | 0 | 0 | 0 | 0 |

The four specially isolated library tests are excluded from the combined library
invocation and each executed separately. Ignored tests remain visible in raw logs;
this local run does not establish live SSH/WSL/forge parity or real power-loss
durability. Synthetic recording checks precede owned-child execution. Real
process suites run in a probed PID/proc/dev namespace without host PID selectors
or host signals. Existing owned ACL fixtures run in private native Linux folders.
No Docker was used. Build scripts, compiler targets and tooling scratch are on E:;
private Linux runtime scratch preserves native filesystem semantics.

Prior candidate `9e57224a` failed two newly added headless zoom/profile fixtures:
egui frame outputs contained unapplied texture deltas. The four corrected output
disposals preserve all frame executions, assertions and loop axes. Both exact
tests now run early and again in the complete library suite. Prior failed logs,
source reviews and shell-checkout repairs remain in `../intermediate-regressions`.
Dev advanced during validation with PR #207; the final history replays onto its
merge commit and preserves its direct-push behavior. The only parser conflict
retains extracted ownership plus submodule-check support and a generalized
supported-mode/option-boundary regression. Unit 18 explicitly materializes exact
LF shell bytes and records before/after hashes without changing index blobs.
An earlier replay helper correctly refused the stale dev base before mutation;
that stopped helper and log are preserved separately from the successful replay.

Prior candidate `7511b6fc` passed those texture-disposal probes but its full suite
exposed first-frame font readiness in the accessibility fixture. The corrected
eframe creation callback installs fonts before its first pass. Both analogous
resolver fixtures defer actual paints until a subsequent font-ready pass. A
ready/pending regression exercises both strategies; this and the original
accessibility test run early and again in the complete suite. The failed suite
and one contained timing diagnostic remain preserved, without treating the
isolated pass as a replacement for that failure.

The first run of this unchanged application candidate stopped in the validation
helper after 61 tooling regressions and a successful formatting command: native
certutil rejected the zero-byte formatting log. No application runtime ran in
that stopped attempt. The corrected runner streams empty files through hashlib,
retains fresh native hashes for nonempty E: files, and fails on native hash errors.
Eleven recording tests cover 26 scenarios including an empty-to-nonempty transition,
stat failures and a removed-guard negative control. Five actual digest checks cover
owned empty/nonempty fixtures and the original zero-byte log. Both attempts and
helper versions remain archived; only the completed attempt supplies the 33-gate
pass claim above.

Three independent source reviews bind to the exact application candidate and
state their authorship exclusions. Source review is separate from test execution.
Raw results bind every gate to unchanged HEAD/tree, binary and log hashes, cleared
compiler overrides, tool versions, commands, environment and shell checkout bytes.
Gzip archives preserve original bytes; manifest hashes verify both representations.
The original runner and assembly helper are preserved in `../candidate-helpers`.
The archive helper's inherited Git-environment correction has recording-only
coverage: 61 varied override cases pass, fail with sanitation removed in memory,
and pass after restoration. This is separate from application test execution.

This evidence is added in a documentation-only follow-up. Application and gate
files must be verified unchanged before binding these prior executions to that
follow-up. No new application test execution is claimed for documentation changes.
Final GitHub CI is intentionally deferred until local checks and independent
review pass; its exact commit and results belong in the consolidated PR.

Publication-tool safety evidence is a separate supplement under `publication-safety/`.
Review found that Git fetch and push destinations can differ. The corrected helper
checks every expanded effective push URL against the canonical fork allowlist
before staging, and retains the fetch check. Five recording methods cover 57
configurations, including mixed/foreign/missing/malformed destinations and failed
queries. Removing the actual guard causes the expected rejection failures,
including three foreign/mixed configurations reaching recorded staging; restoring
it passes. No actual Git command runs in those tests. Exact original/fixed helper
sources, blocked/fixed/independent review records, all regression output, and the
initial 79-artifact manifest/README are preserved. These are publication-tool
regressions, not additional application test execution. The final documentation
commit must still be independently checked for unchanged application/gate files.
