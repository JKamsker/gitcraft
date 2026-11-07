# Private user/PID/mount namespace exact-case validation — failed

One exact previously failed practice-isolation case was explicitly authorized
after the smaller standard namespace smoke was committed, pushed, and remote
verified at `d243f5e8c84f9f8b86bbadc337fd5a15cd6e3238`.
Accepted application head: `ab68bdedd387236c03f26f839572860d405a2f7f`.
The accepted binary SHA-256 was reverified immediately before this run;
integration remained clean before and afterward. No Cargo compile or CI ran.

The standard launcher created new self-mapped USER, PID, and MOUNT namespaces:

```text
unshare --user --map-current-user --pid --fork --mount --mount-proc --propagation=private -- /usr/local/bin/docker-init -s -w -v -- /bin/bash ...
```

It mounted a new procfs only inside the owned mount namespace with private
propagation. No host proc bind, host mount mutation, existing PID selection,
signal, mapping-range extension, service change, or privilege workaround was
requested. Maintained Tini ran as PID 1 in the newly created PID namespace.

The same-run shell preflight passed: Bash PID 2 matched both the first field
of `/proc/self/stat` and `/proc/2/stat`, and its parent was PID 1 `docker-init`.
UID/GID maps were each `1000 1000 1`. This addresses the inherited-proc mismatch
that would otherwise invalidate RootPin's `/proc/{inner-pid}/fd` paths.
Private proc mounting was permitted; there was no denied-action retry.

The sourced toolchain plus `/workspace/work/difft-bin` PATH prepend selected
Git 2.52.0 and Difftastic 0.72.0, both verified inside the namespace. The run
used the clean integration working directory, unset SSH/WSL live-test variables,
RUST_TEST_THREADS=2/RAYON_NUM_THREADS=2, and no Git configuration overrides.
The shell then execed the existing bound binary with only:

```text
--exact app::practice::isolation::normal_practice_tab_operations_and_shells_are_isolated --nocapture
```

Result: **failed, exit 101**, in 20.678 seconds (test harness 20.10 seconds).
The fixture child reported that Git could not fork `git-receive-pack`:
`Resource temporarily unavailable; fatal: unable to fork`.
Thus private proc/PID isolation did not restore the required fork capacity.
The precise controlling quota remains undiagnosed; this self mapping retains
the same host UID 1000.

Count-only visible process/zombie snapshots stayed 31,318/31,302 before and
after (zombie delta zero). Tini logged three owned adopted-zombie reaps. The
fresh PID namespace teardown was limited to its newly owned descendants.
No host process or init was mutated.

The standing authorization allowed one full accepted suite only if this exact
case passed. That condition failed, so **no full-suite retry was run**.
The JSON binds the exact launch command, accepted gate/binary, environment,
private-proc identity evidence, source cleanliness, counts, and raw-log hashes.
All prior failure/smoke evidence remains unchanged.
