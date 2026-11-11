# GitHub CI run comparison

Insights → CI path keeps the existing waterfall, table, critical path, and job
details. On GitHub it also compares exact run attempts and displays the observed
job and step outcomes separately from their duration differences.

Choose a run, then an available attempt. The baseline controls offer the last
successful run, previous run, previous attempt of the same run, no comparison,
and the runs returned by the bounded inventory. Once resolved, both sides show
their run number and attempt; refreshing keeps those exact attempts. The ordinary
CI view shortcut opens the latest attempt, as its tooltip states. Use the attempt,
job, and step links for the exact recorded evidence used by this comparison.

Job alignment uses workflow identity and, when the workflow files are available,
the workflow job ID and named matrix bindings. Display renames and matrix row
order do not by themselves change that identity. Ambiguous bindings, duplicate
names, renamed steps, and missing workflow evidence remain unmatched or carry
an explicit inferred/unknown identity. Step names align uniquely; repeated step
names need recorded positions. Timing differences never establish a failure cause.

The selected job and its steps show the actual attempts/runs read at the same
nonempty commit SHA, including links and unknown entries for unavailable or
ambiguous evidence. An observed pass and failure can establish intermittent job
or step outcomes; they do not establish that an individual test is flaky. History
is bounded to eight snapshots, including the current one, and incomplete reads
are labeled. A failed history listing retains the already observed current run.

Individual test reports, cache hits/misses, toolchain versions, and step log
contents are unknown because this view does not collect them. Runner metadata is
shown only when the API records it. GitLab retains its existing CI path behavior;
the new comparison/history controls are GitHub-only. Log/report downloads,
provider parity, broader history, and report export remain follow-up work.

This is a bounded contribution toward [issue #190](https://github.com/JKamsker/gitcraft/issues/190),
not completion of that issue's full acceptance criteria.

The attempt reads use GitHub's documented [run-attempt metadata](https://docs.github.com/en/rest/actions/workflow-runs#get-a-workflow-run-attempt)
and [jobs for an attempt](https://docs.github.com/en/rest/actions/workflow-jobs#list-jobs-for-a-workflow-run-attempt)
endpoints. Revisions, project/workflow identity, request ownership, and selection
guards prevent a reply for an earlier view from replacing the current comparison.
