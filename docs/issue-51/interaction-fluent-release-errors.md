# Fluent release validation display dependency

This finite follow-up adds 12 paired English/German messages and two render-only
APIs: `release::display_problem(raw, tag, taken)` validates the raw message
against the current actual tag validator inputs before translating it;
`release::display_error(raw)` handles known manifest notes and release-worker
envelopes. Raw tag_problem, draft metadata, labels, execution, command arguments,
release notes and diagnostics remain unchanged. Unknown external messages stay
opaque; manifest-bearing envelopes are restricted to the actual known files.

The workflow-owned release UI must use these APIs only at actual error/note
paint, alongside the previously preserved display_label/display_preview methods.
This dependency does not edit the other writer's consumers or claim their
integration. A pure regression exercises actual tag_problem outputs in German,
including unchanged control eligibility and raw values, rejects mismatched
lookalike errors, and checks worker diagnostics and opaque nested reasons.
Catalog/all-target checks are bound to the immutable commit at
`/tmp/gitcraft-fluent-release-errors-validation.json`. No tag/release/Git/process,
network/native fixture, full suite or CI is invoked. Consumer assembly and other
remaining interaction surfaces remain pending.
