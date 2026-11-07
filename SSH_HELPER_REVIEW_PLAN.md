# SSH helper independent correctness review plan

Candidate: `32f72244ca8b0386915173eacbbb9f87e39d25ad`, tree `8986db14cd8f17939e422bca85e2f525a491365d`; base `c56d0502d7deb59ba85ac8a807c59a8d68729874`.

Status: WIP. This is ordinary correctness/adversarial review, not a security scan. No feature source edits, CI actions, real accounts, keys, agents, live SSH/WSL, or host process selectors/signals are authorized by this plan.

Static binding completed before tests: all 20 source hashes, 5 retained logs, exact library binary hash and 14 authors match the author manifest. Retained full-test log explicitly shows fresh compilation from the candidate worktree and execution of the identified binary. Existing binary reuse is not an independent compile. Binding details are in ssh-helper-review-binding.txt.

Review every changed application path and surrounding contracts: inert opening and explicit steps; exclusive key folder creation and interactive PTY passphrases; machine-local OpenSSH, gh/tea credentials and agent isolation; typed public-only registration and captured account identity; profile adoption, switching, cancellation and stale-result handling; keyboard and queued-stage suppression; native Windows versus POSIX scripts and UI sizing. Examine fixture realism and unsupported platforms.

After this plan is committed, pushed, and its author/remote SHA verified, run the existing immutable candidate library binary with the focused `ssh_` filter and `--nocapture`. Read fixture source first to establish that executable effects are owned recording tools and loopback requests only. No independent compilation claim. If source inspection identifies a focused uncovered behavior, preserve a separate synthetic/owned fixture plan and harness before running it; do not edit candidate feature sources. Stop and report any platform block without bypass/retry.

Record exact commands, counts, hashes, findings and platform limitations afterward. Preserve completed review evidence with another correctly authored commit and push, verifying remote SHA. Report PASS/FAIL only for the exact candidate. Final consolidated review remains separate.
