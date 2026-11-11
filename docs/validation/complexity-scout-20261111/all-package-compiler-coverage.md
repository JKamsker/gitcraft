# Complete tracked-package compiler coverage

Runner implementation: `c3d88ea358521fa6566950a376a34d3240c8a672`, based on
`baf6cc14efc6a4974562de3e1bddc555b18ac2dd`. Recording-profile strict-lint
followup: `3e510e5d8e5e0f4da6741f4944246fff3c297ddd`.

The former root command compiled only the app and OS package. The independent
recovery workspace was absent; a new independent package, or a workspace member
omitted from defaults, could have valid static forbid policy without ever being
compiled by CI. The runner discovers every tracked package manifest, validates
the complete inventory before compilation, and explicitly selects each manifest
and package. Virtual workspace manifests have no target of their own. Missing,
malformed, unnamed, or linked manifests fail closed. No package is exempt.

On the runner commit, recording-only discovery/command tests passed **3 tests**,
and workflow regression tests passed **16 tests**. Subsequent real compiler
coverage tests passed **4 tests** (including those three) with no skipped cases:

```
TMPDIR=/mnt/e/private/gitcraft-build/python-fixtures PATH=/home/jonas/.cargo/bin:... \
PYTHONPATH=ci python3 -m unittest test_complexity_packages -v
```

The compiler negative controls demonstrate the old root command succeeding while
an independent or nondefault package exceeds the threshold, the runner rejecting
both, and restored simple source passing. A fixture requiring the recording cfg
also passes through the runner. Compiler fixture sources are in the immutable
test file; every generated fixture and Cargo target was under E:.

The actual isolated harness initially exposed three strict-lint failures:
recording-hook type complexity, a fake Call missing the production `long` field,
and checking a Result variant before unwrapping its error. The followup names the
hook type, aligns the fake API, and matches the Result while retaining assertions.
Its exact committed source passed this command, exit 0 in 4.15 seconds:

```
TMPDIR=/mnt/e/private/gitcraft-build/python-fixtures \
CARGO_TARGET_DIR=/mnt/e/private/gitcraft-build/recording-clippy-target \
cargo clippy --locked --manifest-path src/resolver/park/checks/Cargo.toml \
  --package gitcraft-recovery-recording --all-targets -- -D warnings --cfg recovery_recording
```

The tracked root and harness lockfiles were used with `--locked`. Only the
dependency-free fixtures and isolated harness were compiled; no application
build, runtime process test, or GitHub CI was launched by this scout. Independent
source review by lifecycle_scout passed both the runner commit and the exact
recording-profile followup, tracing the unchanged alias, fake API contract, and
retained Result-branch assertions. Combined application
and Windows checks remain separate evidence owned by the root agent.

Clippy covers code active for the selected platform, features, and cfg. The known
recording harness explicitly selects its intended profile; this does not claim
analysis of every inactive branch or every macro-generated body.

Archived command output: `all-package-recording-tests.log.gz`,
`all-package-workflow-tests.log.gz`, `all-package-compiler-tests.log.gz`,
`recording-clippy-c3d88ea3.log.gz` (initial failures), and
`recording-clippy-3e510e5d.log.gz` (passing followup).
