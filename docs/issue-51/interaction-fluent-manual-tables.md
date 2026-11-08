# Fluent manual shortcut tables and daily tips

This incremental interaction migration builds on complete source base
`771cbd9924fbdda106a36718c551b95305a9a269` and watcher presentation unit
`0cf8d93732ef23b02fe0882b2556a9cf67918004`.

All 21 daily tips, manual shortcut group titles, local shortcut descriptions,
mouse gesture captions and the shortcut heading use 61 additional paired English
and German messages. Each language has its own bounded static tips cache, so a
language change does not keep the first language forever. The existing static
slice interface and tip order are preserved; consumers that cache tip collections
must also key their caches by language. Registry shortcut keys, numbered tabs,
modifier names and mouse gesture semantics are preserved. Shortcut grid identities
use their stable group index rather than translated names.

The new headless regression exercises the actual shortcut tables, checks German
visible names and a return to English, verifies all tip lengths, and compares the
registry key strings across languages. It installs only bundled fonts, without
constructing the application or native services. Existing registry collision
checks use the stable manual group IDs.

This unit does not complete Fluent migration: the long feature descriptions in
`manual.rs` and other owned interaction surfaces remain pending. No application
Git, subprocess, network, native OS, full-suite or CI execution is claimed.
Immutable-commit command logs and source hashes are kept outside the repository
in `/tmp/gitcraft-fluent-manual-tables-validation.json`.
