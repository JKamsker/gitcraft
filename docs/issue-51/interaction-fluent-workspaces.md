# Fluent workspace cards

This incremental unit adds 54 paired English/German messages for workspace
creation, rename/delete confirmation, repository rows and keyboard removal,
fetch/look controls, status, names and validation diagnostics. Numeric quantities
use locale formatting and whole count patterns. Refs, paths, workspace names,
remote names, persisted data and action requests stay unchanged.

`across::short_reason` remains the raw classifier used by retry/forget logic.
The added `across::display_reason` translates its finite output only for paint.
`across::workspaces::display_error` translates the known input-validation messages
without changing their producers; unknown diagnostics pass to the existing
finite Git display adapter and otherwise remain opaque. External Git output in
row tooltips is retained. The actual card and naming UI use these adapters.

A headless regression drives the actual German workspace row by Tab/Enter,
asserts its translated accessible tooltip/remove label and unchanged external
error, then switches the same harness to English. A pure regression checks
German grouped quantities and relative age, raw error classification and
unchanged workspace validation state. Existing selected pure row helpers and
catalog/all-target checks are recorded against the immutable commit at
`/tmp/gitcraft-fluent-workspaces-validation.json`. No fetch/look/open worker,
Git/process/network/native fixture, full suite or CI is run. My Work and the
remaining interaction auxiliary surfaces are separate pending migration units.
