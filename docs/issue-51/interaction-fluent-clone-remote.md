# Fluent clone and remote forms

This incremental unit adds 47 paired English/German messages for clone/remote
dialogs, options, credentials controls, progress and known app-owned status and
validation envelopes. Stored errors, auth decisions, paths/URLs, host/distro
values, credentials, clone options, window ownership and worker handoffs remain
unchanged. The clone-options header uses its original English identity as an
explicit salt so its expanded state survives language changes.

Finite `clone::display_message` and `remote_open::display_message` adapters
recognize only these app-owned envelopes and return no match for unknown text.
The form error UI and start-page/toast paint use the adapters before the existing
Git display fallback. They never replace raw errors before classification or
rewrite external diagnostics. Backend-owned CloneOptions validation and progress
labels retain their existing API; their canonical backend migration is a
separate assembly dependency, and is not rerun or claimed here.

New actual-form headless tests toggle blobless/sparse options and SSH/WSL kinds,
verify unchanged URL/path/distro/host/credential values, and switch the same
harnesses to English. Pure tests cover known/unknown envelopes; existing remote
target-validation cases are also audited pure. No form submit, machine discovery,
folder browse, clone, cancel, connection, Git/helper/process/network/native
fixture, full suite or CI is run. Immutable checks are recorded outside the
repository at `/tmp/gitcraft-fluent-clone-remote-validation.json`. Other app
forms and auxiliary surfaces remain localization WIP.
