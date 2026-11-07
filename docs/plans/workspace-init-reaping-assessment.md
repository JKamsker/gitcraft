# Workspace init reaping assessment

Status: **not suitable for execution or a live-target approval request**.
This is a recovery assessment, not an implementation. No debugger was attached,
no existing process was signalled, and no workspace service was changed.

## Evidence and scope

The coordinator supplied these observations: about 31,301 zombies are children
of workspace PID 1; its executable is `tail`, with one thread. Existing local
tests fail to create processes/threads with `EAGAIN`. A new subreaper can reap
its own descendants but cannot reap the existing children of PID 1. The
coordinator's isolated Docker alternative failed before entering its user
mapping. These observations were not independently re-probed here.

Read-only discovery in this assessment found neither `gdb` nor `lldb` in PATH.
There were no matching debugger executables at `/usr/bin/gdb*`,
`/usr/bin/lldb*`, `/usr/local/bin/gdb*`, `/usr/local/bin/lldb*`, or
`/usr/local/swift/usr/bin/lldb*`; the unrelated `gdbus` was present. This is
limited executable discovery, not proof that no debugger exists anywhere.
No packages were installed. No attach or permission probe was performed,
including against an owned fixture. Current managed-environment status reported
running/connected; that does not establish debugger or ptrace capability.

The application baseline is `ab68bdedd387236c03f26f839572860d405a2f7f`.
This branch changes documentation only. It supplies no fresh application test
result, and must not be used to bypass the combined candidate's remaining local
test/review gates. No CI was started, dispatched, rerun, or awaited.

## Proposed mechanism and its limits

Only the parent (or an appropriate ancestor subreaper after reparenting) can
consume these zombie child statuses. The proposed mechanism would temporarily
attach a standard debugger to this workspace's known PID 1, interrupt it, invoke
libc `waitpid(-1, NULL, WNOHANG)` repeatedly, and detach/resume it. It would not
send termination signals to children. Successful calls consume exited child
statuses permanently; this cannot be rolled back. A child that exits during the
operation could also have its status consumed.

`WNOHANG` avoids waiting for a live child, but does not make debugger attachment
or an inferior function call harmless. The following blockers remain:

1. **Authorization and identity.** The user has not authorized stopping or
   changing PID 1. An execution wrapper would need an exact operator confirmation
   argument bound to the reviewed revision and workspace instance, with no
   generic PID option. Identity checks must use the known workspace namespace,
   executable, command line, start time, and single-thread precondition before
   attachment, and revalidate after attachment. Never derive targets from a
   host-wide search. Such checks are requirements, not checks performed here.
2. **Permission is unknown.** Linux ptrace access depends on credentials,
   dumpability, capabilities in the target's user namespace, LSM/Yama policy,
   seccomp and namespace boundaries. Numeric PID 1 and the ability to read procfs
   establish none of these. Namespace init has special signal/exit semantics;
   neither a blanket claim that PID 1 is untraceable nor a claim that it is safe
   follows. Do not change policy, capabilities, privileges, or namespaces to
   force an attach.
3. **Inferior state is not transactional.** GDB normally saves/restores registers
   and its dummy call frame on a completed inferior call. This does not undo
   arbitrary memory or libc state changes. In particular, after the last zombie,
   `waitpid` can return `-1` with `errno = ECHILD`, changing the stopped thread's
   TLS errno. Preserving/restoring it needs additional target state access, and
   normally another libc call such as `__errno_location`; this exceeds the
   proposed restriction to calls of `waitpid` only. No target memory reads/dumps
   or writes were used here. It is unproven that changing errno is irrelevant at
   the exact interrupted instruction in this particular `tail` build.
4. **Interruption and unwind are version-dependent.** A debugger can attach while
   a syscall or libc operation is interrupted. ABI, symbol resolution, lazy
   binding, cancellation handling, signal delivery and GDB's inferior-call
   unwind settings affect what is resumed. A finite loop limits call count,
   not attach time or the duration of a failed inferior call. No installed GDB
   version or matching executable/libc behavior has been validated.
5. **Detach is best effort.** A Python `try/finally` inside GDB should attempt
   detach after every normal exception, but cannot guarantee it after debugger
   death, a stuck inferior call, or a failed detach. Batch command files stop on
   some errors, so an unguarded `call ...; detach` sequence is inadequate.
   Automatically killing a debugger, signalling PID 1, or retrying attachment
   is not an accepted fallback. Losing the namespace init can terminate the
   workspace; no independently controlled recovery channel is established.

## Concrete future review gate

Do not run this mechanism now. An executable script is intentionally absent
because the identified state-restoration and recovery requirements are not met.
The exact restricted algorithm that would need review is:

```text
require explicit confirmation bound to reviewed script SHA + workspace identity
reject any target other than the known workspace namespace's init
verify expected identity and single thread; refuse on any mismatch
start a pinned standard GDB with -nx -nh and all auto-loading disabled
try:
    attach; revalidate identity and thread count
    at most 65,536 times:
        call only libc waitpid(-1, NULL, WNOHANG)
        positive: count one consumed status
        zero: stop (no exited child immediately available)
        negative: stop and report an error/terminal condition without retry
    report count and whether bound was reached
finally:
    attempt detach/resume; report and stop if unsuccessful
```

This outline is **not an executable recipe or a safety claim**. In particular,
the errno and failed-inferior-call issues above prevent accepting it unchanged.
Do not add register patching, raw syscalls, shellcode, custom remote-code
injection, memory dumps, privilege changes, host PID selectors, or process
termination to work around those blockers.

If an independently controlled recovery channel and an acceptable inferior
state contract become available, first author the exact wrapper and fixture,
commit/push them, and obtain an independent static review on that immutable
revision. Only after that review passes may an owned, isolated fixture be
considered: synthetic identity/command checks first; then a deliberately owned
single-thread parent with a few exited children and one live child. Verify
status consumption, the live child's survival, return to the same execution
state, and detach/error paths using owned handles only. The fixture does not
prove the live workspace's ptrace permissions or equivalence at its interrupted
instruction. Any live PID 1 action would still need separate explicit user
approval of the exact reviewed operation and its residual risks.

The appropriate current disposition is to preserve the pushed source and seek
an environment/operator-supported restart, replacement executor, or external
test environment. Do not silently substitute debugger mutation of workspace
init for that recovery capability.
