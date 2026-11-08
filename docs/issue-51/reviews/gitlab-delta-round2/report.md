# GitLab GL-R1-1 delta closeout — PASS

Exact candidate **7ef8cac882eb7b9433dc12203036ccb3749f6e31**, tree `3da40659c040ce7a91f9bf7bb1ad9c7465c6b5b2`. Correction parent: `9d8f54cf9c58ad70182a88e961dc40ec2356c02e`. Prior reviewed GitLab candidate: `652e63345057aee614480c656edf3b959e8bb56b`.

**GL-R1-1 is resolved.** This accepts the narrow discussion-route correction and carries forward R1's scoped GitLab conclusions. It does not review the intervening Jira unit or certify whole-branch/shared credential/process/native behavior.

Source was read exclusively through immutable Git objects and exported under `/tmp/gitcraft-51-gitlab-delta-r2-evidence/source`. The correction commit changes only native discussion cards and provider regression tests.

- `src/repo/pr_review/native_discussions.rs:13` now iterates **all** native threads, independently of current diff mapping. File discussions retain the stored path, line/side and explicit outdated label. An unmapped line is labeled “unmapped.”
- Cards have a stable discussion-ID salt and outdated cards open by default (`:40`). Reply, resolve and reopen dispatch the original thread ID (`:53`, `:61`, `:77`); capabilities and posting state still control the buttons. General/current file discussions retain their controls.
- The nine directly relevant production dependencies—including GitLab API/details/post, review-box entry, send routing and current-line eligibility—are byte-identical to R1. Old discussions are therefore exposed separately rather than incorrectly attached to current lines; existing Context/stable-account/MR/revision guards remain intact.
- The new actual egui headless test checks visible old-file coordinates/outdated text and uses pointer events to dispatch reply/resolve/reopen for `old-thread`. The new provider recording test uses an outdated position with the current MR snapshot, confirms original discussion-ID writes, then changes current MR revision and confirms reply/resolution refusal with no added writes.

Independent evidence: the original failing reachability fixture was repeated using the exact corrected `show` function and unchanged `thread_is_on`, with an inert recording UI/minimal Thread fixture. **1 passed**: the outdated file remains ineligible for the current-line panel, but now renders `Resolve discussion` and `Reply to discussion` through its own card. This is not an actual egui/native runtime test. Source, binary and log are preserved in `/tmp/gitcraft-51-gitlab-delta-r2-evidence`.

```text
/Users/jonas/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc --edition=2024 --test /tmp/gitcraft-51-gitlab-delta-r2-evidence/reachability.rs -o /tmp/gitcraft-51-gitlab-delta-r2-evidence/reachability-tests
/tmp/gitcraft-51-gitlab-delta-r2-evidence/reachability-tests --nocapture --test-threads=1
```

Exact writer receipts inspected, not rerun: `/tmp/gitcraft-51-gitlab-r1-fix-7ef8cac8-{0..3}.log`, also bound to this SHA in batch-B validation JSON: **1 headless pointer test, 13 pure/recording GitLab tests, check-all-targets and qualified lint PASS**. New test source was inspected for execution class: provider recording returns before command/account discovery; the headless UI test uses egui without app constructors.

No new concrete defect was found in this correction. No actual Git-under-test, CLI/helper, HTTP/network, keyring/auth, native/process fixture, full suite, CI, PR or external write ran. Runtime/platform/provider interoperability gates remain held exactly as in R1.

Manifest: `/tmp/gitcraft-51-gitlab-delta-review-r2-manifest.json`.
