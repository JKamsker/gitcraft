# Fluent app palette and start links

This incremental unit adds 41 paired English/German messages for app palette
entries/kinds, empty states, query guidance, link descriptions, clone offers and
the manual dialog's controls. Each app item keeps its original English recent key
before resolving its display label; translated labels drive visible matching,
while recency and command variants remain stable. Repository-owned palette keys
remain supplied by their existing adapters. Theme/profile names and branch
identifiers remain opaque. Locale generation already invalidates cached entries.

Link descriptions use whole patterns with named repository, host, PR number,
commit hash, file and line arguments. PR and line numbers stay literal identifiers,
without numeric grouping. URLs, hashes, refs, paths and launch commands are not
rewritten. Clone warnings preserve the original non-ASCII warning condition.
The manual dialog's diagnostic help now accurately describes local review before
an explicitly opened issue. The separately assigned manual feature table/render
is untouched.

A new actual-palette headless test ranks a German app command, activates its row,
and checks the original command variant and persisted recent key. A pure link
regression checks whole German/English descriptions and unchanged URL/ref/path
identities. Existing pure recent/ranking cases, catalog checks and all-target
compilation are recorded against the immutable application commit outside the
repository in `/tmp/gitcraft-fluent-palette-links-validation.json`. No actual
command dispatch, Git/process/network fixture, clone, native action, full suite or
CI is invoked. Profiles, remaining tab/window/remote and auxiliary surfaces still
need their follow-up migration; full localization remains WIP.
