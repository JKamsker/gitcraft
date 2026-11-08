# Fluent profiles and tab strip

This incremental migration adds 37 paired English/German messages for the
profile switcher, unfinished-work explanations, tab titles, context menus,
worktree badges, native-window destination names and tab-close accessibility.
Known unfinished-work identifiers are matched unchanged and translated only to
prose. User profile names, author identities, paths, worktree names, raw work
classifiers and generated profile default names remain opaque/stable.

Tab counts use locale numbers and whole singular/plural messages. The plus
button and overflow menu show the configured New Tab shortcut through the
existing registry instead of the former literal Ctrl+T hint. Command variants,
numbered-tab defaults, action indices, viewport identities and widget IDs are
unchanged. Native move/teleport lifecycle code is untouched.

Headless regressions exercise the actual painted profile row and tab-close
control in German and English, including profile activation response, stable
row identity, and unchanged author/profile data. Pure tests cover known and
unknown unfinished-work labels and existing tab geometry. Catalog and all-target
build checks are recorded at the immutable commit in
`/tmp/gitcraft-fluent-profiles-tabs-validation.json`. No Git, process, native
window, profile activation worker, full-suite or CI fixture is run. This unit
remains part of the unfinished full interaction localization migration.
