# BUNDLE-R1-1 — keep protection through ref publication

Finding delta after `84eb79a0e797015fb2adaa0957c64d0f25932def`.
Read the full independent R1 report and manifest at
`/tmp/gitcraft-51-contained-bundle-review-r1.md` and its `-manifest.json`.
Only BUNDLE-R1-1 is addressed. Other bundle source/platform limits and the three earlier
machine features are unchanged. Status: source correction with pure/compile gates;
independent delta review and all bundle runtime gates remain pending.

Destination indexing now adds `--keep=gitcraft-bundle-keep-v1-<random 128-bit nonce>`.
Git creates this recognized `.keep` before installing the pack/index. The pipeline
requires the exact `keep<TAB><pack-hash><LF>` receipt with the repository hash width.
Git reports `pack` instead if the requested keep already existed; this case refuses
publication and neither borrows the other owner's marker nor retires it. The UI error
identifies that pack and asks the user to inspect ownership/recovery evidence.
Malformed/multiple/trailing receipt data also refuses and retains any created marker.

The marker remains through all destination prerequisite/advertised-object checks,
identity/generation recheck and the complete existing expected-absent ref transaction,
including prepare and commit. Independent ordinary Git repacking sees Git's keep
protection during this interval, regardless of the application's Workspace fence.
Only a successfully confirmed terminal return from the existing owned `update-ref`
operation permits a separate keep retirement operation. No Drop cleanup exists.
Index/output/check/identity/publication errors preserve the marker, including uncertain
publication after refs may already have committed. Conservatively, confirmed failures
also retain protection; inspection/manual retirement may be needed before retrying.

Cleanup uses the existing fenced, durable owned command path. Native cleanup is a
narrow mode of the existing packaged bundle helper; Linux SSH/WSL cleanup is a fixed
Python operation, not a new owner or storage service. Both accept only a full lowercase
pack hash and this import's nonce, derive one fixed pack filename and bound reads to
the exact expected nonce line. They reject symlink/nonregular/multilink or changed
entries and verify the current marker, not cached proof. Unix/remote access pins the
pack directory and checks current entry identity before its single relative unlink.
Windows denies replacement/write sharing and deletes the exact retained file HANDLE.
No scan, glob, recursive removal, pre-existing marker deletion, PID or signal is added.
This uses ordinary Git's contract that other operations respect an owned keep; it does
not claim an atomic Unix namespace lock against hostile manual replacement of marker
files by an actor already able to rewrite the repository.

If cleanup cannot establish ownership, it retains the entry. A cleanup failure after
publication reports **Ref publication completed** and distinguishes possible retained
protection from rollback. The keep is outside the removable workspace, so workspace
cleanup cannot remove uncertain destination protection. Existing resource ceilings,
platform refusals and cancellation infrastructure are unchanged.

Primary source: [Git 2.45 index-pack](https://github.com/git/git/blob/v2.45.0/Documentation/git-index-pack.txt#L41-L67)
documents this exact repack window and keep mechanism. Its
[implementation](https://github.com/git/git/blob/v2.45.0/builtin/index-pack.c#L1380-L1477)
sets the `keep` report only when it creates the marker, before final pack/index names.

Pure recording schedules exercise the production pipeline with an injected Backend:
protection spans object checks and simulated independent repack attempts at prepare /
commit; confirmed terminal precedes retirement; indexing uncertainty after marker
creation, malformed receipt, check/identity/publication uncertainty retain protection;
a pre-existing marker remains unchanged with no publication/retirement; changed marker
or cleanup failure preserves the marker and reports completed publication. Strict receipt
framing/hash-width and native filename/nonce validation are also covered. These tests
run no Git/helper/Python process, filesystem fixture, kernel resource call or signals.

Exact gate commands, external log hashes and source blobs/SHA-256 are in
`contained-bundle-pack-protection-source.json`. Runtime hold remains unchanged. No bundle,
repack, memory-limit, native Job, SSH/WSL or owned-child fixture ran here. Windows remains
an OS-crate compile check, not application-helper/runtime validation; no Linux cross-build
is claimed. No binary evidence is committed.
