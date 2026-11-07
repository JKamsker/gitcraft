# Tour / Portability correction evidence

Frozen application: `55df0d2e5fe8a4e46f47127f645030c00e96d4cc` (tree `3a4a060041ef998d68773be5252e6bc3c934fd17`). Prior failed application `b05e51c` and its evidence remain preserved.

Fresh private local gates pass: formatting, strict all-target Clippy, all25 focused cases, full814passed/3ignored/0filtered and817 registered tests. No safety exclusions; optional live SSH/WSL variables are unset. The manifest binds changed sources, actual compiled binary and all bundled gate logs.

The two review findings are addressed by auth-only private clone routing and filesystem-identity worktree lock lookup. Recording-only transport fixtures prove selected public source/authentication and absence of ambient redirects/personal helpers; real partial locked-worktree restore/retry is covered. SSH import requires fresh supported key binding and explicit literal-endpoint confirmation; unexported aliases can be edited as public addresses. Existing Tea policy and actual UI round-trip remain covered.

Independent re-review is pending. Native Windows and real remote/account authentication were not run, and CI was not used. This evidence unit changes validation files only.
