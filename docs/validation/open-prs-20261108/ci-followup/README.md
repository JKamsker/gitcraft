# PR 183 CI responsiveness follow-up

Tested and independently reviewed commit: `b84d4b1e1478e516da4fbbe0e6811e2a397a4253`.
Parent PR candidate: `8576e67990f895784b702863f2b619179a91975b`.

Linux CI [run 42537661321, job 128905447287](https://github.com/JKamsker/gitcraft/actions/runs/42537661321/job/128905447287)
reported 4,888 passing tests and two failures. The palette test took 20.201763377 seconds
against its existing 20-second budget while the full suite ran concurrently. Linux now
runs this exact test separately, matching the existing Windows scheduling; the time budget
and functional assertions remain unchanged.

The responsive fixture uses a nonexistent repository. A background weakened-check scan
can finish during its four frames and insert a wrapped Git error above the staged header.
An independent diagnosis reproduced the previous absolute-y assertion failure in four of
six standalone runs. The replacement checks alignment with the staged label and Unstage
all action, bounds above the composer, and nonoverlap with neighboring controls across
the same nine width/scale combinations.

Local validation at the tested commit:

- Formatting, strict Clippy across all targets, and test binary build passed.
- Responsive matrix: ten consecutive fresh-process runs passed (90 size/scale cases).
- Complete responsive suite: four tests passed.
- Exact palette responsiveness test passed with its original 20-second budget.
- Each runtime invocation used a fresh temporary/config directory and a separate PID
  namespace (`bwrap --unshare-pid --die-with-parent --bind / / --dev /dev --proc /proc`).
- Independent adversarial review `/root/review_ci_responsiveness_fix`: PASS on the exact
  tested commit, no blocking findings; static review only, no tests or CI execution.

Only CI scheduling, a Windows scheduling comment, and responsive test assertions changed.
Production source under `src/` is identical to the fully tested application commit
`e2c6bb0f211144033b58621e18804f9d81dfc066`, excluding the changed test file.
The earlier full-suite evidence in `../final/` remains applicable to production code;
that full suite was not rerun for this test-only follow-up. Final CI remains required.

The archived runner, build output, failing CI log, targeted test logs, and result metadata
are bound to the tested commit. `manifest.json` records archive and uncompressed hashes.
