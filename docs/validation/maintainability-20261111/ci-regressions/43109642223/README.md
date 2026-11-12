# Preserved Linux CI regression

CI run [43109642223](https://github.com/JKamsker/gitcraft/actions/runs/43109642223),
completed Linux job `130822169435`, tested head `0622a2ac1237311e29f6dcc94788b30f5b094bb4`.
Compilation, formatting and Linux lint passed. The combined library invocation
reported 5,226 passed, one failed, 27 ignored and two filtered; subsequent isolated
palette/bisect and real SSH steps did not run after that failure. Native Windows
execution is not established by this Linux job record.

`repo::navigation::tests::back_and_forward_through_a_repository` failed at
`tests_repository.rs:80`: Back yielded no diff where the fixture expected its
manually opened commit diff. The source trace identifies readiness-sensitive
setup: redundant user selection auto-reveals the diff when detail is ready/cached,
then explicit same-key open intentionally toggles it closed. Pending detail takes
the fixture's passing path. The link-navigation fixture has an analogous graph-only
assumption. Follow-up coverage must distinguish passive manual-history setup from
real Select/SelectHash reveal and same-file toggle behavior for ready, cached and
delayed detail, retaining the navigation invariants.

The prior local 33-gate pass remains genuine for application candidate `0e6a8e52`
and its documentation-only follow-up `0622a2ac`; it does not supersede this failed
CI result. Its immutable evidence remains under `../../final-candidate/`.
This archive preserves the downloaded job log and completed-job API metadata with
raw/compressed hashes. It does not claim a rerun, a fixed candidate, new application
test execution, or completion of other CI jobs. All scripts and tooling are on E:.
