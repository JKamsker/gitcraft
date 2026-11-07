# Matched-environment exact-case diagnosis

Status: one explicitly authorized exact case failed; real process exhaustion
was observed. No broad retry, Cargo compilation, container, or CI ran.
Accepted application head remains `ab68bdedd387236c03f26f839572860d405a2f7f`.
The original failed recovery evidence at
`edd79301c86bb2f3c00ceabd3d9715730b382231` remains unchanged.

## Environment comparison and qualification

The prior recovery command sourced `/workspace/.gitcraft-env/env.sh` but
omitted the accepted `/workspace/work/difft-bin` PATH prepend. Reconstructing
that sourced environment selected `/usr/local/bin/git` 2.52.0, but found no
`difft` executable. The prior broad attempt therefore did not fully match the
accepted tool environment, and its individual failures cannot all be ascribed
to process exhaustion.

The narrow diagnosis sourced the same toolchain and prepended real difftastic.
It verified Git 2.52.0 at `/usr/local/bin/git` and Difftastic 0.72.0 at
`/workspace/work/difft-bin/difft`, used the clean integration working directory,
unset live SSH/WSL variables, and added no Git config overrides. Tini and the
ordinary three-second-grace shell supervised the accepted binary with
`--exact app::practice::isolation::normal_practice_tab_operations_and_shells_are_isolated --nocapture`.
No different test or broad retry was run.

The exact test exited 101 after 20.10 seconds (23.67 seconds including grace).
Its child reported:

```text
Practice app did not settle: ... cannot fork() for git-receive-pack ...:
Resource temporarily unavailable
fatal: unable to fork
```

This establishes real fork exhaustion under the matched environment; it does
not prove the precise controlling quota or establish that every prior failed
case had that cause. Both visible process and zombie counts stayed unchanged
(31,317 and 31,301 respectively). Tini reaped three owned adopted zombies.
Integration remained clean. The JSON and raw logs bind the exact case,
accepted gate and binary, environment resolutions, counts, and log hashes.

The prior notify abort must also be interpreted cautiously. In notify 8.2.1,
`inotify.rs:123-125` discards `Builder::spawn`'s Result. A failed thread spawn
can later surface as the `SendError` unwraps at lines 584 and 635, without an
EAGAIN message in the log. Absence of that message does not rule exhaustion out.

## Read-only alternate-route inspection

The managed local Docker socket is healthy. The daemon is rootless Docker
29.9.2, uses `vfs`, reports cgroup driver `none`, and has zero cached images and
zero containers. No pull, container, build, daemon change, or restart occurred.
Endpoint/context/TLS selectors were cleared for the local-socket reads;
Docker credential configuration was preserved and not printed.

The current task UID/GID is 1000/1000 with no effective capabilities. Existing
`setpriv` therefore cannot simply switch this process to an unrelated UID.
`/etc/subuid` and `/etc/subgid` allocate dockerd ranges `100000:999`, `1000:1`,
and `100999:64536`; choosing container UID 1000 may map back to the same
exhausted UID. A different container UID, for example 1001, is the candidate
and its actual kernel mapping must be verified in an owned container before
any test. This is an alternative-route hypothesis, not proof of capacity.

The integration directory and accepted binary/difftastic are mode 0700, so
an isolated UID cannot read them directly. Do not chmod those originals.
Stage separate readable copies and read-only mounts instead. The executor is
Debian 13 (trixie). ELF inspection finds the test binary and difftastic need
libc/libm/libgcc_s and the loader; modern Git additionally needs pcre2 and zlib.
A compatible trusted Debian trixie userspace is the candidate; no compatible
image is currently cached.

Proposed next step, subject to orchestration authorization: obtain a compatible
base image, stage only the required immutable files with separate permissions,
then run a tiny owned-child capacity check under Docker `--init`, private PID
namespace, distinct mapped UID, no capabilities, read-only mounts, private
writable tmp/home, and no network. Verify the real UID mapping and local tool
versions before considering one exact application test. The rootless daemon's
cgroup-driver limitation must be accounted for; do not assume unsupported PID
or memory flags create enforcement. A global task ceiling could still block
this route. No host PID cleanup or workspace restart is justified.
