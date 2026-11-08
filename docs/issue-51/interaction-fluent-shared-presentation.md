# Fluent shared presentation helpers

This incremental unit adds 15 paired English/German messages for app buttons,
button placement, lane-colour choices, update-channel captions and the default
menu refusal explanation. App-button tooltips read the effective key registry;
menu rows use the shared icon/shortcut renderer. An unbound command shows no
shortcut. Stable button/place enums, widget IDs, serialization and event dispatch
remain unchanged.

The account-check result paints known errors through the exact imported backend
`git::display_error` adapter. Stored errors remain raw. Unknown provider/Git text
is not rewritten. An inert headless test seeds the actual result state, verifies
German then English paint, and asserts unchanged raw cancellation text. Another
headless test configures a real chord and an unbound manual key, activates the
actual app menu and checks the original button event identity. Existing app-button
layout/drop geometry cases and the actual menu shortcut-paint case are pure or
headless. No account request, process, native action, network fixture, full suite
or CI execution is claimed.

Immutable selected checks and all-target compilation are recorded outside the
repository in `/tmp/gitcraft-fluent-shared-presentation-validation.json`.
Preview content, the full manual and remaining app/auxiliary surfaces are still
pending; this unit is writer-validated migration WIP.
