# Resolver, parking and insights repair evidence

Tested source: `0fed49da19be2c9bc5b1f4feeb9bf369e03678b3`, based on the exact combined candidate `0f002ffc0f9334271b14ae79d746ccd35466457a`.

Application fixes: `2abc7d92085ac82f003949cd1ff801181df13830` (cleanup proof and preservation), `1d5b9bd53073af3398cdb8b6fbb3d9682e32b6da` (raw-byte restore guard), `dedcecf977ce26498a23fbbeac6fd57a4032daaa` (insights review dismissal), and `ca08e1556fc4f453a721fd956d042123c4b88b8f` (service-independent recovery record). The two following commits correct only test metadata refresh. All commits use Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>.

## Local checks

`cargo fmt --check` and `git diff --check` passed. `cargo test --locked --lib --no-run --message-format=json` passed with ALSA metadata at `/tmp/gitcraft-pr179-build-deps/root/usr/lib/x86_64-linux-gnu/pkgconfig`. The build used the existing target cache at `/home/jonas/.t3/worktrees/gitcraft/t3-01301ba9/target`; all source came from this repair worktree.

The checked-in runner and compressed logs record the exact order. Recording-only failure mapping, worker panic and service outcome checks passed **before** owned-child tests. Each invocation used `bwrap --unshare-pid --die-with-parent --bind / / --dev /dev --proc /proc -- <binary>`, with a fresh mkdtemp TMPDIR, Git 2.50.1 and LFS 3.4.1 on PATH. No host-wide process selectors or signals were used. Source units were committed, pushed to `t3/repair-resolver-park-review` on JKamsker/gitcraft, and remote SHA verified before runtime tests.

- Recording mapping/join proof: 1 passed.
- Recording service outcomes: 3 passed.
- Resolver process suite: 5 passed.
- Parking suite: 28 passed.
- Cleanup retention/action guard regression: 1 passed.
- Insights action-dispatch regression: 1 passed.
- Complete resolver flow suite: 5 passed (includes the two preceding regressions).

The filtered-clean regression initially needed index stat metadata refreshed through `git add notes.txt`: the filtered blob remains HEAD's, status becomes clean, and the CRLF worktree edit remains. The test proves refusal before writes, unchanged index/worktree and retained archive, then successful restoration once the hidden edit is saved. Earlier fixture failures are not counted as passing runs.

## Behavior and limits

Generic process errors and worker panics fail closed. Cleanup uncertainty clears selections/review, leaves the run failed, retains the workspace/MCP artifact, writes diagnostics without reading agent-owned files, and marks scratch worktrees against automatic deletion and later sweeps. Durable run loading refuses an uncertainty marker. Diagnostics and marker persistence require successful filesystem writes; in-memory scratch preservation still prevents deletion during the current process if writing its marker fails.

Restore checks every regular file in current HEAD before checkout/read-tree/raw writes, using byte-exact post-park baselines when the object matches. Other files and older archives must match Git's current filtered checkout bytes; unusual or nondeterministic filters can therefore cause conservative refusal. Concurrent external edits after the final preflight remain subject to the existing filesystem race boundary; this does not introduce an atomic worktree lock against editors.

Insights dispatch closes only read-only review state and retains run identity, phase, results, selections and the same workspace Arc.

No CI, PR, merge, delegation, full final all-target build or independent adversarial review was performed here. The parent owns final combined gates and review. This evidence-only commit inherits the recorded checks after verifying application/test files match the tested source; it does not claim new runtime execution.
