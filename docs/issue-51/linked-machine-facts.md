# Linked project copies (#34.6)

Source implementation, pending independent review and runtime gates. This unit follows
`e494b165c0ddf502b48b77ac65b231704d81e8a3`; it does not modify supervision.

The command palette's **Linked project copies…** opens a reachable dialog. Save/unlink
buttons change only the open repository's `gitcraft.linkedRepository` local setting.
Saving a link does not connect to it. **Inspect linked copies** explicitly connects to
up to eight local, WSL or SSH paths through existing managed Git commands. Closing the
dialog cancels its child job scope. Config mutations use the existing repository job key.

The collector requests tracked index blob IDs/modes, checkout EOL metadata, status,
HEAD/tree, potential case collisions, mode changes, six fixed Git settings
(`core.autocrlf/eol/safecrlf/filemode/ignorecase/symlinks`) and fixed `--version` calls
for Git, Python 3, Rust, Cargo and Node. Version parsing keeps only a conventional
version token; arbitrary suffixes and unknown tools/settings are discarded. It does
not request environment variables, remote URLs, credential settings or file contents.
The fixed `test.default.command` setting is reduced to a hash inside the collector;
only valid good/bad tree notes from `refs/notes/tests/default` are displayed. These
are explicitly labelled stored results, can be stale or externally written, and do
not certify command provenance. Differing command hashes are flagged. Missing notes
are untested; inspection never executes a test command. Open copy leads to existing
test-lane actions when new results are wanted.

Clean Git blob identities determine tracked-content equality independently of modes
and checkout EOLs. Dirty/concurrently changed, assume-unchanged and sparse/skip-index
samples cannot claim equality. Different hash formats are unconfirmed. Untracked
files, byte-for-byte checkout equivalence, submodule worktrees and symlink-target
filesystem equivalence are excluded explicitly. Samples are bounded observations,
not atomic filesystem snapshots. Unicode lowercase collision candidates are hints,
not an exact model of every filesystem. Limits: 20,000 index entries, 4 MiB per facts
response, existing 120-second bounded-command deadline. Missing tools are omitted;
unavailable copies/errors remain visible. Read-only missing-object behavior reuses
the existing no-lazy-fetch gate; there is no new credential or lazy-fetch policy.

Pure gates (no Git commands, helpers, owned-child fixtures, connections or filesystem
mutation): `git::machines::tests` records the actual collector query boundary and
checks allowlist reduction, unknown/secret suffix removal, index validation, sparse
and assume-unchanged refusal, content match/difference/dirty/hash-format cases;
`repo::dialogs::linked_machines::tests` renders injected local-good/SSH-bad facts in
an egui context without submitting jobs. Runtime local/WSL/SSH/Windows and actual
test-lane interoperability remain held. Build/check commands and exact source hashes
are recorded in the accompanying JSON; no generated binary evidence is committed.
