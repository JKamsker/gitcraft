# Fluent application dialogs

This incremental unit adds 38 paired English/German messages for agent approval
cards, SSH prompt controls, local diagnostics review and update-banner states.
Whole named patterns cover agent/repository names, remaining approval time,
platform credential-store guidance and update versions/states. Credential help
uses the configured palette shortcut. The ghost-preview header has an explicit
stable ID to retain its collapse state across language changes.

External SSH prompts/fingerprints, literal SSH yes/no answers, secrets, agent
wire replies, agent-supplied request content, commands, hashes, diagnostics and
release URLs retain their original values. Known update/account errors use the
imported backend display adapter only when painted. No worker, approval policy,
arming threshold, credential-store or cancellation behavior changes. The actual
SSH modal now accepts its existing state directly, so its controls can be tested
without entering the OS question/answer queue; the App wrapper is unchanged.

Three new audited headless cases drive the actual SSH trust controls (asserting
literal yes/no), approval controls (asserting decision identities and opaque
commands), and local diagnostics review. The existing local-review no-browser
regression is retained. No network account, SSH/OS prompt queue, agent execution,
update install/relaunch, actual browser/native action, process fixture, full suite
or CI is invoked. Immutable selected checks and all-target compilation are stored
outside the repository in `/tmp/gitcraft-fluent-app-dialogs-validation.json`.
The separately assigned manual table is untouched. Remaining app/auxiliary
surfaces and integration of exact shared process corrections are still pending.
