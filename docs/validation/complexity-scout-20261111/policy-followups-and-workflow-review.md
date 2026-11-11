# Configuration policy follow-ups

The validated combined policy source is
`c189ca125965e81e44305d6f2a024f6a3abf9210`, CI tree
`8f64786c9b45c1ea8332ea060e04ab669c92bbee`. Its CI files were verified identical
to the tested working files. `symlink-policy-tests.log.gz` records all 48 Python
test methods passing on Linux, without skipped cases. The command was:

```sh
TMPDIR=/mnt/e/private/gitcraft-build/python-fixtures \
PATH=/home/jonas/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  python3 -m unittest discover -s ci -p 'test_*.py' -v
```

All new compiler fixture source, scripts, Cargo targets, and temporary files were
under `E:/private/gitcraft-build`. No application build was run for these follow-ups.

- `8e97b81112d640cda903b2154abb778d0701637b` rejects Cargo environment values
  that redirect `CLIPPY_CONF_DIR`. Twenty-four static variants cover filename/case
  and plain/relative/forced values. Real compiler fixtures prove both config
  filenames can redirect to threshold 100 despite threshold 25 and manifest
  `forbid`, and require policy rejection. The earlier 44-method passing run is
  retained as `clippy-redirect-policy-tests.log.gz`.
- Reader contribution `9fac6a25c51781f013369c10b48484bf6deb0a5c`, integrated as
  equivalent `0f8d6d31fe36a57b97df130cf8cce872affdfb04`, adds opt-in symlink
  rejection. Gate_scout independently reviewed the reader's Git-mode checks,
  actual working/dangling link checks, and preserved default LOC behavior; no
  blocking findings were found in that contribution.
- `c189ca12` enables strict reads for compiler policy, including a tracked `.cargo`
  directory symlink. Seven path/case variants cover working, index, revision, and
  restored-working-file cases. Real compiler controls prove linked legacy config,
  config.toml, and `.cargo` directory can activate lint-capping target files,
  require policy rejection, and restore compiler failure after removing the link.
  Default physical file-size behavior still counts Git link contents.

Lifecycle_scout independently reviewed exact `8e97b811` and `c189ca12`, excluding
their own reader contribution, and reported no blocking source findings. That
review checked configuration value/key forms, strict snapshot path selection,
working/index separation, and the real compiler baseline/restored controls. The
reviewer ran no additional compiler builds; the 48-method runtime evidence above
belongs to gate_scout's separate test execution.

Working/staged physical-size checks passed with 2,615 Rust files and zero errors;
staged compiler policy passed. These changes modify CI tooling, not application
source. This document adds evidence only and binds those existing checks; it does
not claim a new application runtime execution.

# Independent workflow test review

Reviewed exact lifecycle-scout revision
`42da1475e951ed2bb5457631a57fa0419c5cfac7` relative to `ac03bfcf`, limited to the
new required-command/job-step validator and its mutations. The gate_scout's older
workflow dependency implementation is excluded from this independent review.

No blocking findings remain in the reviewed additions. Exact command checks
protect policy, compiler fixtures, and all-target Clippy in CI/release/Windows;
job/step skips, failure softening, checkout retargeting, altered commands, and
unsupported shell/defaults are rejected. Review identified ancestor environment
and container environment/options gaps; the final revision rejects those surfaces
and includes generalized mutations for them.

An independent archived-snapshot run of all 15 workflow regression methods passed,
recorded in `workflow-42da1475-tests.log.gz`. Source, script, and output were under
`E:/private/gitcraft-build/workflow-42da-review`. This is test-only review evidence,
not approval of a later final application candidate.
