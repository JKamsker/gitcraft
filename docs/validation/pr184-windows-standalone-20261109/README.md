# Windows standalone test isolation

The failed core shard on `590bc4b4fe6bc6a312d3373e779220935b97e553`
reported eight standalone test failures. Six reported a subprocess timeout;
two UI assertions subsequently exposed the same timeout as their load error.
Their failures clustered while native PowerShell metadata helpers ran concurrently.
This supports resource contention as the working diagnosis; it does not establish
a deterministic production defect. The other nine Windows shards passed.
The two live forge matrices also passed on the unchanged application source.

Source candidate `33dfd019002d5b97c151826fc297f87bea31fe91` changes only
`ci/test-shards.py`. It excludes `standalone::` from the parallel core invocation,
then runs that entire feature group with `--test-threads=1`, followed by the
existing exact isolated tests. Nonzero serial exits fail the shard. Partition
validation includes serial tests and refuses overlaps and empty serial groups.
No test is skipped and production deadlines remain unchanged.

Four recording tests passed for serial placement, complete synthetic partition,
failure propagation, empty groups and overlap rejection. The real precompiled
Linux library inventory also remains a complete disjoint partition of 5,024
registered tests. This was `--list` inspection, not runtime test execution or
native Windows validation. The archived recording harness references the original
workspace and completed artifact record; those paths are historical inputs.

The source-map comparison confirms every path/mode/blob except the CI script is
identical to the base. The previous full application gates and live matrix evidence
remain applicable to those unchanged application/test inputs; no new Cargo build
or runtime pass is claimed here. A new Windows CI run must establish whether
serial execution resolves the observed contention.

`validation.json` binds the source candidate, verified preservation branch SHA,
script, recording harness and output. `manifest.json` inventories the captured
files and binds the raw failure log. Historical archives are preserved unchanged.
