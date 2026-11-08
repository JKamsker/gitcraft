# Fluent core settings rendering

This incremental unit adds 237 paired English/German messages. Core General,
Appearance, Graph, Git, Terminal and identity controls/notes, portable settings,
agent instructions, diff preferences and file-manager integration instructions
use Fluent. A finite settings-local label renderer covers known page/section/row
names. Their original names stay the identities for search and preview state;
unknown/user-owned names remain opaque. Searches match both visible German names
and the existing English labels/technical keywords. Navigation captions and
semantic names resolve the visible language without changing the page enum.

The actual layout headless regression clicks a German checkbox, switches back to
English and verifies the same control ID and value. Quantity fields use locale
formatting together with a matching parser: otherwise German `2.000` would be
parsed by egui as two. A pure regression checks German/English grouping, decimal
round trips and malformed inputs. Settings payload field names, enum identities,
profiles, commands, argv and native worker policies are unchanged. The app-button
placement combo now keys by its stable button enum, independent of its label.

`settings::compact_tip()` provides translated graph-layout prose; the legacy raw
`COMPACT_TIP` remains for source compatibility. Repository owners must use the
function at their paint boundary. No repository source is edited by this unit.
Specialized account/network/signing/theme/AI/custom-command pages, preview
content and shared helper display labels still need their subsequent migrations.
The full manual and other interaction modules also remain pending.

Selected pure/headless checks, catalog checks and all-target compilation are
bound externally to the immutable application commit in
`/tmp/gitcraft-fluent-settings-core-validation.json`. No full settings-page test
that might discover shells/tools, native picker/action, credential action,
background worker, application Git/process/network fixture, full suite or CI is
claimed. Logs and generated adapters remain outside the repository.
