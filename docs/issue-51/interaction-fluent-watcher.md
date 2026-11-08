# Fluent interaction migration: watcher and network presentation

This unit appends 28 English/German message pairs (152 interaction pairs in total)
using the unchanged accepted API. It migrates all application-owned presentation in
`tray/watch.rs`, the resident pause caption, Linux tray-host explanation and the
network Pause label/explanation adapters. Watcher transition sentences use named
arguments and actual Fluent commit-count plurals. PR numbers remain literal reference
identifiers; counts receive locale grouping. Provider titles/errors remain arguments.

The transient status caption refreshes on a locale revision without restarting jobs,
discarding observations or retiring queued notification owners. Historical messages
already delivered to the OS retain their delivery-time language. Process, fetch,
provider, cancellation, profile and delivery ownership protocols remain unchanged.

The new German recording check reaches actual transitions, tick/status, observation
argv and queued delivery. It verifies localized count/review/check text, unchanged
event keys, exact quiet-fetch and remote-HEAD argv, retained delivery ownership across
a language switch, and the actual final notification closure with an inert sink.
All eleven tray recording/pure tests and five catalog tests pass, with an all-target
macOS compilation check. Immutable source/log hashes and exact commands are external
at `/tmp/gitcraft-fluent-watcher-validation.json` after preservation.

No native tray/notification/locale call, Git/network/helper fixture, Linux target,
full suite, lint green or CI acceptance is claimed. Other interaction app/settings/
manual/error/auxiliary strings remain pending; this is partial #47 migration.
