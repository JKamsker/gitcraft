# Issue 192 Windows registration identity

Application source: `000f077fe5c67ae0b7afa6030756d53fd009ca98`, including a plain merge of current dev. Every command in
validation.json passed on this immutable candidate. The evidence commit changes only docs.

Windows run 43712030002 at 7ba8d993 failed four resource tests before collection: literal
Path equality could not recognize the registered path with another Windows spelling.
Exact failed job logs and artifact-derived failures are preserved here. Recovery #224
uses remote::same_dir while the directory exists and then preserves Git's spelling.
Resource collection now reuses that check through a shared read-only registration helper.
For missing paths on Windows, it canonicalizes the deepest existing ancestor on both sides
(resolving short names) and compares keys without extended prefixes, separator/case or
trailing-slash differences. Subsequent collection uses the exact Git-listed path.

A pure-function regression runs on Linux and Windows with extended-drive, UNC, short-name
spelling, separator, case and trailing-slash inputs and distinct-drive/path negatives.
The existing real-Git tests cover existing linked, missing, detached and stale-capability
collection. The missing-path test asserts the returned Git registration spelling.
New Windows-only helpers/imports carry matching cfg gates; the pure key is also compiled
for tests. No Windows/macOS local suite or cross-target compilation is claimed.

The unrelated git::file_comparison guarded-patch failure in git-a-m was a process timeout
in unchanged code, confirmed in both the exact job log and test-report artifact. It was
not modified and no old-head run was rerun; any retry is limited to the current new head.

Formatting, Clippy 1.100 all application targets -D warnings, size/complexity, committed-HEAD
architecture and baseline-growth checks passed. Published architecture declarations are
unchanged. Scoped application test executions (overlap intentional):

- tests-worktree_resources: 10 passed
- tests-worktree_inspection: 1 passed
- tests-constellation: 17 passed
- tests-repo-insights: 84 passed
- tests-git-worktree_cleanup: 42 passed
- tests-repo-branch_cleanup: 29 passed
- tests-terminal: 51 passed
- tests-a11y-tests-repo_view: 2 passed
- tests-palette: 61 passed
- tests-repo-layout: 4 passed
- tests-repo-tests: 33 passed
- tests-i18n-: 5 passed

All Rust commands used nice -n 19 cargo +stable with the issue-owned target/TMPDIR and
local fixture Git 2.49. No full suite or local Windows suite ran. Prior review and tooling
evidence remains preserved. Final new-head CI is required before merge. No destructive
action, timeout increase or skipped test was added.
