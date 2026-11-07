# Corrected SSH helper independent re-review plan

Candidate `bcb0e78ba1fca0622126a2e72885554ee3fc9e0e`, tree `9946698fef1a2b66b6f50ef4ab0d0172b98cc362`; originally reviewed `32f72244ca8b0386915173eacbbb9f87e39d25ad`.

Status: WIP before independent focused tests. Read-only source review; no CI, native Windows, real SSH/WSL, keys, agents or accounts. Prior plan and all task safety/author/preservation rules continue.

Static binding verified: candidate HEAD/clean worktree; exact two changed paths plan.rs/tests.rs from original; all20 original source hashes from immutable Git objects; all20 corrected source hashes and18 unchanged application paths; five final retained log hashes, binary hash,16 exact authors. Full author test log records fresh compile from corrected worktree and named binary; no independent compilation is claimed. See ssh-helper-rereview-binding.txt.

Review correction against original P2: parsed remote targets remain first; failed parsing must reject reserved WSL UNC authorities on Windows, including case/mixed separators/extended/device forms. Whole server matching must preserve ordinary shares/local paths/non-Windows behavior. Inspect pure fallback tests and cfgWindows valid-UNC plus failed-open palette tests, with native execution limitation explicit.

After this plan/binding is correctly authored, committed, pushed and remote SHA verified, rerun the exact corrected reusable library binary using `env -u GITCRAFT_TEST_SSH -u GITCRAFT_TEST_WSL <binary> ssh_ --nocapture`. This runs focused owned recording/loopback/pure fixtures already inspected plus new pure Windows fallback checks on Linux. No independent compile or native-Windows runtime claim. Confirm binary hash after execution. Preserve exact command/count/log hash, resolved finding and final candidate-bound verdict in a new commit/push/remote verification. Final consolidated review remains separate.
