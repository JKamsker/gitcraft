# Unix Command environment correction

Narrow source fix after `dec969b52f09e6f6b1765ccd3054f9810a0224e5`.
The parent's first owned macOS fixture on its separate `f37ca66b` candidate failed;
no runtime was executed by this writer. All additional runtime remains held until
the parent reviews this immutable fix and authorizes its unchanged owned fixture.

Rust 1.95's installed `library/std/src/sys/process/unix/unix.rs` runs pre_exec
callbacks at lines 385–387, installs the captured Command envp at 393–405, and
executes the original program at 408. The previous anchor execv inside pre_exec
bypassed that envp installation. Additions, removals and env_clear therefore failed
for the actual program. Public get_envs cannot distinguish inherited empty overrides
from env_clear with no entries; rebuilding/overlaying an environment cannot fix this.

The target's original opaque Command now reaches normal std spawn unchanged except
for assigning the owned group. A separate minimal packaged C anchor is started with
empty environment, root cwd and null stdin/stderr (its private stdout is only the
startup readiness pipe). Std establishes its group; C verifies getpgrp()==getpid(),
sets its signal disposition, writes exactly R1/newline, closes the readiness writer,
and parks. Rust requires that exact frame and EOF through a nonblocking pipe within
five seconds, then confirms the private anchor has not exited. No target starts on
startup failure. Target arguments/arg0, environment/clear/removals, cwd, stdio and
existing pre_exec callbacks remain owned by the original std Command implementation.
There is no custom postfork callback, allocation, env mutation, exec interposition,
manual PATH resolution or environment reconstruction in production launch code.

The anchor's exact Child is retained unreaped before target join and throughout target
execution/status observation. Reaping the separate target cannot recycle the group
identity. Final group signaling derives authority only from the retained anchor Child;
the diagnostic OwnedChild id now names the target and grants no group authority.
After final signaling, target wait preserves the target exit status, while anchor reap
is separately required for cleanup. Readiness/target/pre_exec launch failure retires
only the newly owned anchor and preserves any cleanup error. Anchor wait ownership failure
forbids subsequent numeric signals. Anchor retirement consumes its Child before wait;
finish memoizes both success and failure, so Drop/repeated finish cannot signal after
anchor reap. Windows launch/Job/thread handling and controller recovery are untouched.
The shared finisher now preserves both a stop error and a reap error if both occur.

Audited recording-only gates: ten actual opaque Command constructions (inherit, set,
remove, clear+set, clear empty, non-UTF8 Unix value, custom PATH, removed PATH,
cleared+custom PATH, non-UTF8 program/arg0 plus argv/cwd/pre_exec). The production
launch orchestrator is injected with a recording backend: it asserts the same Command
object/overrides reach the spawn boundary without reconstruction. Tests do not parse
Debug to infer clear in production; Debug equality is only a test observation of that
unchanged object. They do not claim actual child environment or PATH runtime outcomes.
Additional recording cases cover startup refusal, target failure plus cleanup refusal,
exact readiness framing, signal-before-reap/idempotence, separate stop/reap errors and
live controller no-signal authority. No test starts a helper, child, shell or connection.

The parent can mechanically rename its private fixture assertion `process.status` to
`process.anchor`; no fixture body change is needed. Actual environment/PATH matrix,
canary survival and inherited-process cleanup require the parent's later owned gate.
Bundle changes are absent from this unit. No generated binary evidence is committed.
