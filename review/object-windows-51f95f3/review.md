# Windows correction: scoped STATIC PASS

Application `51f95f3c7121082c7ff4afcdf65e1533e27da049`, delta from
`f2b7968b45d90afc1bd2f7d2347482a2463f0742`. Exact clean tree and five changed-file
hashes are bound in `review.json`. No author/integration source was changed.

The original/backup inode is no longer written or truncated. Detach uses the actual
retained READ+DELETE file handle and retained info-directory handle with
`SetFileInformationByHandle(FileRenameInfo)`. The cached Microsoft contract specifies
an error when ReplaceIfExists is false and the target exists, and permits a relative
name under RootDirectory. Recovery writes a fresh create-new inode and publishes it
through another no-replace handle rename. Newer alternates and colliding backups are
preserved; same-file proofs compare volume/file-index while actual handles stay live.
This removes OBJECT-WINDOWS-3's hard-link exclusion dependency at the design level.

The locked windows-sys 0.61.2 `repr(C)` struct/union and API signature match the wrapper.
Typed allocation supplies native alignment and enough space for the header and
bounded ASCII-to-UTF16 name. Both handles and allocation remain live for the synchronous
call. Byte length is bounded and explicit; the API contract does not require a trailing
NUL. Name validation excludes separators, NUL, ADS and traversal components. No
blocking ABI, buffer, lifetime, collision or recovery defect was found statically.

The new native fixtures only manipulate owned temporary repositories. They observe
hard-linked other-clone bytes, detach, failed-proof recovery, newer metadata and backup
collision preservation. No new remote runner, PID selector, signal or controller appears.

**Limits:** this exact source is uncompiled and native-unexecuted. No formatting,
Clippy, cross-target or native runtime pass is claimed. Later owned native tests must
establish actual filesystem/permission/sharing behavior; static review is not that proof.
Compile-only work may resume when root lifts the resource hold. This report alone
grants no runtime approval. OBJECT-PROTOCOL-1/2 remain open: full helper/runtime
acceptance stays blocked until the accepted owned-transport adaptation is reviewed.
The old c805 harness remains prohibited. No build, fixture, helper, native process,
signal or CI operation was executed by this reviewer.

Full application recommendation remains hold for evidence and established transport
correction. Scope risk: high impact, moderate likelihood/confidence, partial protection,
managed recovery. Exact source, cached API and binding hashes are in `review.json`.
