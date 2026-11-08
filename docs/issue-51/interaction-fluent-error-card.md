# Fluent error explanations and recovery controls

This interaction-local delta adds 86 paired English/German messages to the
complete migration base `771cbd9924fbdda106a36718c551b95305a9a269`, after the
manual shortcut/tip unit `60808da81520a78a9419d4a952e1fb05c0aabbb8`.

All application-owned ErrorCard titles, explanations, observed-fact prose,
confirmation/outcome text, internal failure messages, visible controls and
semantic close labels use Fluent. Paths, configuration keys/values, Git output,
attribute values, suggested literal CLI commands and the raw `label()` retry
identity remain untouched. The actual retry button is translated; no header was
invented for the non-rendered `Reload` dispatch label. Complete message patterns
use named interpolation, including file lists and count selectors. Duration
formatting preserves the existing unit thresholds without trimming English
suffixes from another API's localized result.

Two added pure/headless groups check real German retry-button dispatch from an
inert card, unchanged raw label/output, paths/configuration text, German file
counts, durations and all known problem explanations. They construct the state
directly and do not start the background discovery worker. Existing pure text,
confirmation and parsing checks are selected individually; real repository,
process-list, lock removal, config mutation and native fixtures remain excluded.
The operation and inspection policies are unchanged.

Already produced asynchronous fact/outcome strings keep their delivery-time
language, like other historical results; current titles, explanations and controls
resolve the current language. Full application/manual migration remains pending.
No native OS, Git/process/network fixture, full-suite or CI execution is claimed.
Immutable source hashes and exact command logs are external in
`/tmp/gitcraft-fluent-error-card-validation.json`.
