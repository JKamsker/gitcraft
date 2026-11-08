# Fluent settings previews

This incremental unit adds 80 paired English/German messages. All preview headings,
explanations and choices resolve the selected language. Painted inert history
summaries and ages are catalog messages; civil dates and quantity measurements use
the existing locale formatters. Whole named patterns cover graph width/kept-name
parts, pull behavior, lane spacing and editor-theme colour guidance.

Example hashes, refs, author names, code, shell commands, file paths and diagram
identities remain literal. Only the presentation of the made-up history changes.
Preview enums and the underlying settings values are unchanged. App-button
settings rows retain English search/row identities and resolve their captions via
the existing finite settings renderer, even though the button display helpers now
translate.

An inert actual-paint regression draws ten preview types with editor-theme lookup
disabled. It inspects rendered text for German labels, example ages/dates and
unchanged refs/shell commands; checks grouped widths and English fallback; and
verifies the settings payload stays byte-equivalent as JSON. The remaining
settings-row test and catalog checks are pure/headless. Exact checks and all-target
compilation are recorded outside the repository in
`/tmp/gitcraft-fluent-settings-previews-validation.json`. No real repository,
tool/shell discovery, native action, process/network fixture, full suite or CI is
claimed. The separately assigned manual table is untouched. Remaining app and
auxiliary surfaces still need migration; full Fluent localization remains WIP.
