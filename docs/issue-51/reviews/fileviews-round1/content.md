# Fileviews content adversarial review R1 — FAIL

Exact candidate `c82112a5ab8ba461c487c3266462a81896c85d33`; application `5df74dbb3792a34ee0b1d13c301c9aa552aa6c69`; base `a850de26b0a0778ea96fe34cd1498e6e78928492`. Checkout `/Users/jonas/.t3/worktrees/gitcraft/t3-e06bdc42` clean and HEAD exact. Verified application→candidate changes are only the validation Markdown, JSON and compressed logs. Reviewed AGENTS.md, original issue #40 and both validation records.

Scope: SOPS recognition, ciphertext transport, explicit local decrypt, private plaintext/lifecycle/rendering, process helpers; SVG source/rendered preview and external resources. Read-only: no tracked changes, CI, full-suite, unsafe process-control tests, remote cleanup/cancellation or live service access. Benign local fixtures and small headless dependency/source harnesses only. Other reviewers own general UI and mutations/standalone.

## Blocking findings

### 1. [P1] Recognized untracked SOPS files still run textconv before opt-in

`src/repo/diff/mod.rs:233–236` correctly probes and sets `no_textconv=true` for SOPS. But `src/git/diff.rs:245–250` immediately returns from the untracked branch using PATCH_ARGS and `--no-index`, ignoring **all** DiffOptions including no_textconv. An ordinary user SOPS textconv driver therefore runs automatically merely by opening an untracked encrypted file. Its decrypted output becomes the normal patch in `prepare`, DiffData, message results, syntax/layout caches and normal controls, defeating both explicit “Show decrypted” and “plain text never cached.” On SSH/WSL, this same Git conversion can execute on the remote host and return plaintext rather than ciphertext.

Concrete safe reproduction: `python3 /tmp/gitcraft-review-sops-repro.py`. It creates an isolated local repository, `.gitattributes` `secret.json diff=review-sops`, a benign fake textconv program outputting `fixture-private-conversion`, and a small recognizable JSON SOPS-shaped untracked file. The exact candidate-generated untracked argv outputs:

```
candidate untracked argv return 1 textconv_ran True private_in_patch True
with no-textconv return 1 textconv_ran False private_in_patch False
```

No actual secret or encryption was used. The flag comparison proves suppression fixes the missing boundary; the current opt-in test checks only Unstaged and Range keys, not Untracked. Ensure the untracked argv honors no_textconv and add a reachable load/UI regression checking no tool invocation, no normal DiffData plaintext, and retained Show decrypted behavior.

### 2. [P1] Line-history patches bypass SOPS recognition and cache textconv plaintext

`src/repo/diff/mod.rs:222–225` routes selected-line commit diffs directly through `git::line_diff` and `prepare`, bypassing the recognition/options logic in load. `src/git/line_log.rs:83–100` builds `git log -L` with no `--no-textconv`; the line-history query above it has the same plain-patch constants. A user can open a SOPS file/line history, select a commit, and receive a decrypted normal patch without clicking Show decrypted. `prepare` leaves `sops=false`, so the private reading mode is absent. Plaintext takes the ordinary shared result/cache path.

The same local safe reproduction script runs the exact line_diff argv:

```
candidate line_diff argv return 0 textconv_ran True private_in_patch True
```

The fixture program output is a marker, not real plaintext. Guard both the initial line-history query and its per-commit patch path before conversion; either disable textconv for SOPS there or explicitly route through the private supported comparison and reject unsupported line-limited decrypted requests. Include a selected-line regression, not just normal file patch tests.

### 3. [P1] Repository SVGs can read and render arbitrary local filesystem resources

`src/repo/diff/images.rs:105–112` sends repository-controlled SVG bytes to the default egui image loader. `src/app/mod.rs:233` installs egui_extras' default SvgLoader. Candidate dependency `egui_extras-0.36.2/src/loaders/svg_loader.rs:39` constructs `usvg::Options::default()`. In `usvg-0.45.1/src/parser/image.rs:87–113`, the default string resolver interprets image href as a local path and uses unbounded `std::fs::read` before image-type validation. The candidate does not replace that resolver or sanitize external resources. A tiny SVG can therefore read a local image outside the repository and put its contents into the shared rendered image/texture cache, including while viewing a historical or remote SVG. The local read also occurs synchronously in the loader/UI rendering path and can involve an arbitrarily large file or special file.

Safe concrete repro preserved at `/tmp/gitcraft-review-svg.rs`: an 8×8 SVG references absolute `/tmp/gitcraft-review-svg-local.svg`, a separate benign magenta image outside any repository. Compile/run against the candidate dependency artifacts:

```
rustc +1.95.0 --edition=2024 /tmp/gitcraft-review-svg.rs -L dependency=target/debug/deps --extern egui_extras=target/debug/deps/libegui_extras-e8f0b788d741a1f1.rlib -o /tmp/gitcraft-review-svg
/tmp/gitcraft-review-svg
# size=[8, 8] center=#FF_00_FF_FF
```

The same default options used by the installed loader resolved and rendered the external file. No private files, network, scripts, devices or large allocations were tested. Disable external string/file resolvers for repository SVG bytes and bound inline image/decoded resources and parse/render work; preserve embedded safe images where supported. Add absolute/relative/file-resource tests that verify no local read, plus a safely bounded oversized embedded-resource test. This finding does not claim JavaScript execution or demonstrated network fetching.

### 4. [P2] Unreadable/oversize working sides are treated as empty, corrupting comparisons and the recognition decision

`src/repo/diff/encrypted/reader.rs:169` takes `raw_working(...).bytes` and discards Version.warning and mode. `src/git/file_comparison/lens.rs:274–276` returns `Ok(unavailable())` when safe_read refuses an oversized/unreadable regular file; unavailable at lines296–297 has empty bytes plus a warning and mode unknown. Thus an unreadable current working side looks like an empty document rather than a failed read. Comparing an ordinary small encrypted index file against a >2 MiB current encrypted file can show a decrypted deletion instead of the promised size-limit error. If the old side is absent/ordinary plaintext and the new side is oversized SOPS, `probe_checked` sees no envelope and returns Some(false), enabling ordinary textconv for tracked changes despite the intended fail-closed behavior.

Exact reader harness `/tmp/gitcraft-review-sops-reader.rs` includes the unchanged candidate reader.rs, stubs the raw-working transport to return its documented unavailable Version, and prints:

```
exact read_sides with unsupported/oversize Version: Ok(("", ""))
```

This is a transport/model-boundary experiment, not a full application test. Reproduce in UI by staging a small valid encrypted version then replacing its working file with a >2 MiB encrypted version and selecting Show decrypted; it must refuse the read rather than diff against empty. Reject warning/unsupported modes and preserve missing-file versus unreadable-file distinctions before recognition and comparison; test both the correctness and automatic-textconv implications.

## Verified behavior and remaining context/lifecycle assessment

- Reachable SOPS header offers Show decrypted and encrypted patch/cancel; ambiguous JSON-document/binary envelopes require an explicit choice. Recognition supports JSON/YAML/dotenv/INI and extensionless contents. Inspect parses metadata without key lookup, rejects malformed/oversized/deep/aliased YAML, and separates recipients/wrapped keys from other metadata and actual decrypted text differences. These normal-path features are substantive, not skeletal.
- The explicit private decrypt path fetches raw ciphertext through bounded local/encoded remote helpers and starts only local `sops`, fixed format/filename arguments, stdin/stdout, with no repository cwd. No plaintext temp-file path, settings/telemetry/doctor output call or shared Msg payload was found in that path. Raw tool diagnostics become fixed generalized messages. The fixed CLI agrees with [SOPS3.10 upstream main.go](https://github.com/getsops/sops/blob/v3.10.0/cmd/sops/main.go): decrypt without a positional filename reads stdin; input/output-type and filename-override are supported. No real SOPS/age was installed in my PATH and I made no actual-encryption claim from fake fixtures.
- Private State owns receiver/result, drops plaintext on close/drop, cancels its child token and clears active flag. Opening another diff, refresh/request change, reading mode change, changing ambiguity encoding, file-comparison view and parked tab dispose private state. Profile switching rebuilds/drops tabs (`src/app/profiles.rs:77–86`), so it also disposes the private owner. I found no demonstrated profile/account reuse flaw beyond the normal-patch bypasses above. Explicit SOPS currently uses the user's local process environment/key configuration; the first-wave connection/profile agent must preserve disposal when integrating new profile lifetimes.
- Private painting bypasses egui's full-text galley memoization; numeric wrap offsets stay local. Displayed plaintext necessarily exists in that frame's paint data; no extra persisted full-string layout cache was found. Read-only controls and late raw-hunk action guards cover private mode. Because bypasses1/2 never enter private mode, these safeguards cannot protect their output.
- `processes::run_while` submits to the existing service, polls its owned Ticket and cancels that ticket when active becomes false. It introduces no PID discovery, host signaling or replacement supervisor. Tool stdout<=2 MiB, stderr<=32KiB,20s timeout; ciphertext local command limits and SSH timeouts are explicit. Remote cancellation can wait for bounded cleanup; the separate ciphertext-only worker lets private plaintext results drop earlier. `read_sides`' overall deadline is only checked before each command, rather than passed as each command's remaining timeout; a read can exceed the nominal20s wall duration, but the caller's15s await returns and drops its owner while the bounded ciphertext cleanup proceeds. No new unsafe process test was run.
- SVG source/rendered switches and Git-quoted rename before images work in the normal fixtures. The external-resource resolver above prevents acceptance of unrestricted rendering of repository-controlled SVG bytes.

## Verification evidence and scope

Eight exact existing focused tests were re-run with the candidate built binary; all passed, one test each with3,233 filtered out: content versus recipients/reencryption; recognition formats; JSON/binary ambiguity; private galley cache; tracked ordinary patch textconv suppression; private read-only/lifecycle UI; SVG rendered/source data; quoted SVG rename data. Results and binary digest: `/tmp/gitcraft-fileviews-content-focused-r1.log`. These do not cover untracked/line-history/oversize-working bypasses or SVG external resources.

Parent independently reports **real-tool PASS with no skip** using official digest-verified SOPS3.13.3 and age1.3.2, fresh isolated keys and copied exact writer test binary: `/tmp/gitcraft-51-orchestration/real-tools/real-sops-test.log` and `provenance.json`. Scope includes real formats, recipient rotation, ambiguity, rename and binary-output rejection. This is a distinct gate from my fake-converter/source-boundary review; it does not address the four blockers above. The writer's original real-tool fixture self-skipped, as accurately disclosed in its docs.

No broad gate was rerun or inferred. The safe repro script creates only isolated temporary repositories and marker output; the SVG harness reads only the benign /tmp file it creates. Checkout remained unchanged. Status FAIL until all four findings receive source fixes and focused regressions, followed by review on the corrected immutable candidate.

Reviewed built test binary SHA256: `e3316b2d2b86f2667a519340fd7cbc563837a2abb9dc65f967239cf85769cff2`.
