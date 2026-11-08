# Operation-label presentation contract

`crate::operation_labels::display(raw: &str) -> String` formats a finite set of
known application operation labels and templates for painting. Unknown/custom
labels pass through unchanged. Template values (refs, remotes, paths, patterns)
are opaque, and nested Undo/Redo/Preparing wrappers are translated only when the
inner label is also recognized. English preserves existing labels.

Never store this result in `run_ops`, `OpKind`, a busy-operation identity,
retry dispatch, timeline entries or rewrite comparison state. Raw prefixes and
values control cancellation, continuation, LFS refresh/locking, celebrations and
undo matching. Translate only at the final rendering boundary. Raw status strings
that are also compared for equality likewise remain raw until painting; this API
formats a label, not an arbitrary Git/status/error sentence.

This is a small shared presentation dependency, with 59 paired messages, for the
coordinated repository-core/workflow render migrations. Those owners retain their
raw call-site values and own their production rendering integration/classifier
regressions. This unit does not claim those integrations complete. The pure
adapter regression checks German rendering, unchanged borrowed raw labels,
ref/path arguments, nested recognized labels, opaque unknown labels and English
round trips. Actual repository operation/classifier execution is not claimed.

All prior interaction deltas were source-audited: no raw run_ops/OpKind/timeline
operation-label values changed. ErrorCard `label()` remains a raw retry identity;
its own fix control/confirmation/result prose has no operation classifier role.
Immutable source/command receipts stay outside the repository at
`/tmp/gitcraft-fluent-operation-labels-validation.json`. No native, process/Git,
network, full-suite or CI fixtures are authorized by this unit.
