# Fluent interaction migration: keyboard, semantic labels and native controls

This partial migration uses the accepted combined base
`771cbd9924fbdda106a36718c551b95305a9a269` (identical local import tree at
`0d4ca3bd50a39f2bd30f279d22729e498cbafad3`). It adds 124 genuine English/German
message pairs to the interaction-owned catalog, keeping the accepted Fluent API.

Completed presentation in this unit:

- All registry action titles, keyboard editor controls/instructions, binding validation
  errors and which-key heading. Const registry rows store message IDs; Command::title
  and the manual's registry rows resolve them at presentation.
- Accessibility label adapters for commit/file/branch/status/section/worktree/PR/stash
  names and change counts. Counts use locale formatting and Fluent plural selection;
  repository names, refs, hashes, user messages and paths remain arguments.
- Show/Quit/native notification fallback labels, Windows recent-group title, Linux
  launcher Quit/tooltip and desktop tray tooltip. Menu updates refresh its tooltip.
  Action identities and literal Open/Clone/Quit command arguments remain unchanged.

Intentional literals: persisted command/key IDs, Git ref/status/stash syntax, viewport
and widget IDs, keyboard modifier/key spellings, product name and external payloads.
The shared engine, settings contract, native lifecycle and process ownership are
unchanged. Native watcher result/status/error text, remaining app/settings/manual
content and auxiliary modules are still pending; this is not full #47 completion.

Direct checks include the actual German editor Change → F8 → Save path with unchanged
`new_tab`/`tab_1` identities and numbered-tab keys, real German semantic names/counts,
and the actual tray menu with unchanged unusual path/argv values. Existing English
keyboard and semantic-label checks plus embedded catalog parity/parser validation
remain relevant. Exact immutable commands, source/log hashes and exits are external
at `/tmp/gitcraft-fluent-controls-validation.json` after preservation. Only audited
pure/headless and compilation checks are authorized; no native objects, process/Git/
network fixture, full suite or CI acceptance is claimed.
