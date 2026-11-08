# Fluent tab appearance menu

This unit migrates all icon descriptions, color names, headings, reset text and
custom-painted accessible labels of the tab appearance menu: 30 paired
English/German messages. Actual icon names and serialized TabColor variants
remain unchanged. ICONS' second tuple element is now a stable message ID resolved
only when the menu paints; its first element remains the original asset key.

The actual headless menu test picks the German Library icon and Red highlight,
asserts the same `package` / `Red` persisted JSON, switches to English and checks
both original menu controls and unchanged serialization. Catalog and all-target
checks are recorded against the immutable commit in
`/tmp/gitcraft-fluent-tab-look-validation.json`. No native menu/window, process,
Git/network fixture, full suite or CI is run. Full interaction migration remains
WIP; this unit covers only the tab appearance menu.
