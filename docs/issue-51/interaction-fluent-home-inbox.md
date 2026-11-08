# Fluent start page, inbox and window controls

This incremental interaction unit adds 45 paired English/German messages for the
start-page actions and their descriptions, native picker captions, missing-Git
instructions, recent-repository filtering/removal, inbox status/counts and
painted-row semantic names, and custom window caption/grip controls. IDs, URLs,
request numbers, paths, raw Git output and dispatch/index values remain stable.

Inbox presentation uses the exact backend display API dependency preserved in
`94d05a0a11e270b93ce8477c4e1b15ffb7cb4152` from canonical backend source
`e88e0383d48e67b28be959c0b7a592d6aabdd3d2`. Errors are grouped/classified using
their original identities and localized only when rendered. Forge `noun()` stays
raw; `noun_label()` is used in visible prose. Count quantities use Fluent's
formatter, while a request number is an opaque identifier. The actual painted
GitLab row is keyboard-activated in German by the added headless regression;
a separate pure case asserts unchanged raw no-login classification after display.

No application constructor, missing-Git retry click, native picker, forge request,
Git/process/helper or network fixture is executed. Selected existing pure/headless
layout, filtering and title-bar adapter checks are included in the external
immutable receipt `/tmp/gitcraft-fluent-home-validation.json`, together with
catalog and all-target checks. No full-suite/native/CI claim is made.

Full localization remains in progress. In particular, the shared backend-owned
`git::relative_time` still needs its canonical display migration; this unit retains
its API and source. Other owned app/settings/manual/auxiliary strings remain
pending and are not covered by the catalog count.
