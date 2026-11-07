# Independent SSH helper review

Candidate `32f72244ca8b0386915173eacbbb9f87e39d25ad` against base `c56d0502d7deb59ba85ac8a807c59a8d68729874`: **FAIL pending the Windows machine-boundary fix below**. No feature source changes were made.

## Finding: reject malformed reserved WSL UNC paths before local fallback

Priority: P2. Location: `src/repo/ssh_setup/plan.rs`, `Machine::of`, lines 24–43; surrounding contracts are `src/remote/mod.rs::wsl_target` and `src/repo/mod.rs::opening`.

On native Windows, actual WSL repositories use UNC paths, for example `\\wsl.localhost\Ubuntu\home\me\repo` (also `\\wsl$\...` and extended UNC variants). When the recognized WSL share has a missing/invalid distro, for example `\\wsl.localhost\-bad\repo`, `remote::parse` returns `None` because `valid_distro` fails. `Machine::of` then checks only SSH/WSL URI spellings and generic `://`, and constructs `Local { windows: true }` for this UNC spelling. A failed-open repository can still display its normal UI and palette because `RepoTab::opening` returns false after `load_error` is set. The helper can consequently offer generation, agent and account actions on Windows for a path that intended WSL.

Recognized WSL UNC prefixes that fail remote parsing should produce an invalid-machine error, including case-insensitive standard and extended UNC spellings. Ordinary local paths and other network shares should retain their existing behavior. Add focused construction tests for malformed UNC locations and a valid actual WSL UNC path. This finding is based on native-Windows source control flow; no Windows execution is claimed. Existing invalid-location tests cover `wsl://...`, which is not the actual WSL repository representation.

## Completed checks

- Verified candidate HEAD, all 20 manifest source hashes, all five author log hashes, exact reusable library binary hash, and all 14 commit authors. The retained full-test log records fresh compilation in the exact worktree and execution of that binary. This is author compile evidence, not an independent compilation.
- Preserved the review plan and binding before tests: commit `1a4009cf350964bc52bda8c57a420c94638fc20a`, exact author verified and remote branch SHA matched.
- Independently executed the unchanged identified binary with `env -u GITCRAFT_TEST_SSH -u GITCRAFT_TEST_WSL ... ssh_ --nocapture`: **39 passed, 0 failed, 0 ignored, 799 filtered out**. Reviewed fixture implementations before execution. Owned fake OpenSSH/gh/tea tools, disposable directories and owned loopback endpoints only; optional live transports disabled. Exact command, binary and log hashes are in `ssh-helper-review-evidence.json`.
- Reviewed all changed application paths plus relevant profile/credential/terminal/machine/parser contracts. The intended inert opening, per-step execution, exclusive folders, PTY passphrase handling, public-only payloads, exact local account capture, SSH forwarding disablement, stale receiver cancellation, profile adoption, repository shortcut suppression and 800×500 prompt visibility are represented coherently in implementation and focused tests. No other actionable finding was identified.
- Independently inspected retained tea0.16.0 source (zip SHA-256 `561f33e999ef89403fda25990e394b21f45a2e1215ffd1b5803a407b5f3352d8`): `login -- alias` shows account metadata without token output; `ssh-keys add --login alias --title title -- public_file` is the implemented command form.

Author full-suite evidence (bound by hash, not independently rerun here): 835 passed, 3 ignored, 0 filtered, 838 registered; fmt and strict all-target Clippy passed. No CI use, dispatch, rerun, polling or waiting occurred.

Limits: native Windows, real SSH/WSL hosts, real forge accounts and actual key/agent operations were not exercised. Remote runtime fixtures execute decoded POSIX scripts using recording substitutes; they do not establish real transport/account interoperability. Existing OpenSSH and an already available machine agent are prerequisites. Review must be rebound to any corrected immutable candidate, and final consolidated review remains separate.
