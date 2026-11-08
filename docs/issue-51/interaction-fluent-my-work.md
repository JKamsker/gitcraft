# Fluent My Work and across-repository prose

This incremental unit adds 55 paired English/German messages for My Work's
results, progress, machine discovery captions, push controls and complete work
explanations. Counts and whole singular/plural sentences use Fluent numeric
arguments. Work::describe/why_no_push are presentation-only methods; their enum
selection, push eligibility, raw branch/upstream/ref/path/stash values and
actual push implementation remain unchanged. The obsolete count-fragment helper
and its implementation-only test are replaced by whole UI sentence coverage.

Machine::display_label is used only at paint. The existing machine labels and
machine_of/group keys remain stable. Raw short_reason classification still
controls forget/retry behavior. The finite across::display_error adapter
translates known app-owned queue/timeout/push/search envelopes at paint and
preserves unknown external diagnostics. Workspace full-error tooltips use the
same adapter. Raw producer error strings, queue state, argv and cancellation
paths are unchanged. Branch-bearing envelopes reject whitespace/colon-prefixed
lookalikes rather than consuming arbitrary external prose.

New headless tests draw actual My Work results and push controls in German and
English. Clicks return original Open/Push requests without dispatch; no worker
is started. Pure cases cover raw tracking/forget classification, opaque data,
known error envelopes, unknown diagnostics, and locale numbers. Existing pure
row-description/stale-answer and workspace headless regressions, catalog parity
and all-target compilation are recorded at the immutable commit in
`/tmp/gitcraft-fluent-my-work-validation.json`. No discovery, Git, push, helper,
network/native fixture, full suite or CI runs. Remaining app forms and other
interaction auxiliary surfaces remain migration WIP.
