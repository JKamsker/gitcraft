# Fluent release preview dependency

This interaction-owned render dependency adds `Step::display_label()` and
`Plan::display_preview(forge)` in `src/release/display.rs`, with 14 paired
English/German messages. Workflow-owned `repo/release/view.rs` must call these
methods at actual paint; this isolated dependency does not edit that writer's
consumer. Raw `Step::label()`, `Plan::preview()`, plan/step values and release
execution stay byte-for-byte unchanged, apart from the new module declaration.

The new preview derives whole explanatory sentences from the plan's existing
fields. It preserves tag/version/ref/remote/asset names and literal command
examples, including `chore(release):`, `git tag -s/-a`, and `git push --atomic`.
The changelog file's appended explanation is translated separately. Release-note
payload generation and persisted markdown remain unchanged.

An audited pure test compares complete English display previews with original
previews, verifies signed/version and annotated/direct-commit German variants,
checks raw plan equality and raw labels after locale change, and retains opaque
refs, names, notes and command examples. Catalog and all-target build gates are
bound to the immutable commit in
`/tmp/gitcraft-fluent-release-display-validation.json`. No release/Git/process,
forge/network, native, full-suite or CI fixture is run. Consumer integration and
remaining release validation-error presentation belong to subsequent units;
this dependency alone is not a full release localization claim.
