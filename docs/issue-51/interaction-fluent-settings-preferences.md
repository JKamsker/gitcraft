# Fluent theme and custom-command settings

This incremental unit adds 69 paired English/German messages. Theme controls,
gallery filters and groups, native picker captions, whole import/save outcomes,
and painted colour-swatch semantic names resolve the selected locale. Theme
names, IDs, RGB bytes, file extensions, URLs and the persisted copy-name suffix
remain literal. Previously produced import/save messages retain the language in
which the action completed.

Custom-command settings translate controls, shell-variable explanations and
whole validation explanations. Enum/serde values, shell text, prompt variable
names, user names and trusted paths stay unchanged. The raw `BUILT_IN` table,
`variable_error()` and `problem()` remain stable; `variable_description()` and
`display_problem()` are paint-only adapters. Unknown validation strings pass
through unchanged. Existing eligibility still reads the original raw result.
Repository workflow consumers may use the display adapter at their own paint
boundary; this unit does not edit repository-owned sources.

Three new audited pure/headless tests exercise the actual command editor and
painted colour swatch across language changes, and check German validation
rendering alongside unchanged raw errors and command eligibility. Selected
existing pure validation/eligibility/serde cases, catalog checks and all-target
compilation are recorded against the immutable application commit outside the
repository in `/tmp/gitcraft-fluent-settings-preferences-validation.json`.
No command, native picker, file import/export action, network request, process
fixture, full suite or CI execution is claimed. Profiles, connections, network
settings, previews, the full manual and other interaction modules remain pending.
