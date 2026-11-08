# Contained bundle import (#37.11)

Source unit after `79d65c9b7721d89a862755a9d8c55cb05b1e056a`.
Status: implementation plus pure recording/compilation gates; independent source
review and all bundle runtime gates remain pending. No bundle/helper/Git fixture,
SSH connection or untrusted pack was executed by this writer.

## Reachable behavior

The existing **Inspect local Git bundle** palette action still reads a bounded local
file once, inspects its refs/prerequisites and performs header-only Git verification
on the repository machine. The new dialog retains those exact bytes with SHA-256 and
the canonical destination common directory. It displays the input hash, incoming refs,
prerequisites, selected refs, and exact proposed destination names.

**Verify complete pack** requires explicit consent to create a private quarantine.
A successful check enables a separate confirmation and **Verify again and import
selected refs**. Every import repeats verification; a cached UI success is never
mutation authority. Selection/name changes reset the mutation confirmation. Stop and
closing the dialog cancel the actual shared job token; late ownership/cleanup errors
are preserved before any UI cancellation normalization. Jobs use the repository
mutation key and never route untrusted Git errors through credential retry.

Imported names are `refs/bundle-import/<group>/<incoming name without refs/>`;
`HEAD` maps to `refs/bundle-import/<group>/HEAD`. The group permits 1–64 lowercase
ASCII letters, digits, underscores and hyphens. Empty/stale selections, invalid refs,
case-folded duplicates and directory/file collisions refuse. All selected refs use
one `update-ref --no-deref --stdin` transaction with `create` (expected absent), prepare
and commit. Existing refs and the working tree are never overwritten. Results list
created refs with copy controls and a repository refresh action. The namespace is
available to Git/revision navigation; it is not presented as ordinary local branches.

## Existing ownership and ordering

The operation uses the existing `Workspace`, stable generation claim, actual-machine
mutation fence, durable owner publication/recovery ledger and shared process Service.
Only two workspace methods gain sibling visibility; their implementation is unchanged.
No supervisor, endpoint, cancellation transport, scratch-init, signal selection or
cleanup framework is added. Every Git stage carries the existing fenced authority.
All fallible post-claim setup paths reach explicit close, preserving primary and
cleanup errors together. Uncertain owners retain recovery evidence.

A fresh bare quarantine has no templates, hooks or inherited global/system config.
It reads existing destination objects only as alternates for incremental prerequisites.
All parsing commands forbid lazy fetch/network protocols, replacement objects, hooks,
fsmonitor, maintenance and auto-GC. Git 2.45+ is required so the existing no-lazy-fetch
policy can establish prohibition; older ordinary-repository fallback remains available
for the pre-existing export/header-inspection path only.

The actual pipeline checks matching object format, creates quarantine, resolves exact
prerequisite commit IDs without fetching, runs `index-pack --stdin --fix-thin --strict
--threads=1 --max-input-size=134217728`, then full strict fsck/connectivity rooted at all
advertised IDs and prerequisites. It confirms all advertised objects. Verification
closes quarantine without installing destination objects/refs. Import checks destination
identity/generation again, indexes the **same held pack bytes** into the pinned common
repository, checks prerequisite/advertised IDs there, rechecks identity/generation and
publishes refs last. There is no Git URL, fetch, FETCH_HEAD, temporary source ref or
force update. Cancellation/failure may leave unreferenced objects. Lost completion
acknowledgement can also mean the new refs already exist; the UI explicitly warns the
user to inspect the group before retrying. Cleanup failure after successful publication
reports that distinction rather than suggesting rollback occurred.

## Enforced memory boundary and supported limits

The separate packaged application mode `--gitcraft-contained-git` runs inside the
existing lifecycle owner. It establishes resource limits **before opening pack input
or starting Git**. Linux then performs normal exec; Windows waits through the shared
Service while the existing outer Job retains lifecycle ownership.

| Target | Enforced boundary | Limits / refusal |
|---|---|---|
| Native Linux | Hard inherited `RLIMIT_AS`, read back after lowering | 1 GiB address space per process; 512 MiB per file, 120 CPU seconds, no core files. Existing tighter soft/hard limits remain tighter. |
| SSH / WSL Linux | Python 3 `resource.setrlimit` then `os.execvpe` in existing owned group | Same Linux limits. Missing Python/resource support or non-Linux refuses before opening pack data; no unlimited Perl fallback. Runtime support has not been demonstrated here. |
| Native Windows | Nested `JOB_OBJECT_LIMIT_JOB_MEMORY` and active-process limit, assigned to exact current helper handle and read back before Git | 1 GiB aggregate committed memory, 8 processes including helper. No breakaway/notification-only limit. No independent Windows disk quota is claimed. |
| macOS / other systems | Refuse unpack/import | Inspection/export remain available. No claim that compressed input size, timeout, `ulimit`, or a pre-exec readback proves expanded-pack containment. |

Input is at most 128 MiB. Each Git operation has a 120-second wall deadline and one
pack thread. Accepted stdout/stderr are at most 2 MiB each; native Unix/remote transport
can capture up to the existing 64 MiB per stream before this smaller acceptance check.
Windows inner Service enforces 2 MiB capture limits directly. Input copies remain
bounded by the compressed input cap; the GUI never expands Git objects.

These are resource bounds for trusted Git parsing untrusted data, not a sandbox for
arbitrary programs or malicious executables. Linux AS is per process, not an aggregate
cgroup memory guarantee. Git uses fixed data-only commands with external hooks/network
and automatic maintenance disabled. On Linux the file limit is per file, not a total
filesystem quota. Repeated authorized imports can grow the destination object store.
Very large legitimate packs/objects/repositories may safely refuse within these bounds.

The macOS refusal is deliberate: XNU `kern_exec.c` reapplies the AS limit to the new
map without checking the result, while `vm_map_set_size_limit` refuses a limit below
the map's existing size. A checked readback before exec alone is therefore insufficient
proof for an arbitrary Git image. No additional injection/sandbox framework was added.
Primary contracts: [Linux getrlimit](https://man7.org/linux/man-pages/man2/getrlimit.2.html),
[Windows enforced Job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information),
[Windows nested Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs),
[XNU exec](https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.1.9/bsd/kern/kern_exec.c#L1434),
[XNU map limit](https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.1.9/osfmk/vm/vm_map.c#L20811).

## Gates and finite handoff

Only new pure recording tests ran: 2 OS limit read/lower/readback refusal cases;
2 snapshot/ref mapping cases plus 1 injected egui/cancel-result case;
3 injected production-pipeline ordering/error/stale-target/publication cases;
1 environment-command recording case. They never invoke resource installation,
spawn helpers, run Git, connect, send signals or expand pack data. The first injected
UI run failed because test texture deltas were not cleared; that harness error was
corrected and the final exact-source rerun passed. Existing runtime tests were adapted
only for dialog construction/visible text and were not executed.

Exact commands/results and application source hashes are in
`contained-bundle-import-source.json`. Logs stay outside the repository at
`/tmp/gitcraft51-bundle-*.log`; no binary evidence is committed. Native macOS all-target
checking/building and the Windows OS-crate cross-check are compilation gates only.
Linux-specific syscalls, the Windows application helper path, remote Python script and
actual memory enforcement have not been runtime validated; no Linux cross-build is
claimed. Full inherited tests remain held by the parent.

Remaining finite gates: independent review of this exact source/containment inventory;
parent-authorized Linux/Windows and Linux SSH/WSL owned tests for valid full/incremental
and corrupt/missing-prerequisite bundles, expanded-object memory refusal, cancellation,
collision/stale destination and cleanup-uncertainty/ref outcomes; final combined
integration checks/review on the parent's fresh-dev candidate. macOS unpack/import
remains unsupported rather than represented as complete on that platform.
