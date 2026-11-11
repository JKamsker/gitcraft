# Historical integration checks for four PRs

Application commit: `30a9c4f12c0c75658bc1c570c1cd52356b3dbd37` includes #179, #180, #181, and #182, plus integration repairs. The five-PR candidate `0f002ffc0f9334271b14ae79d746ccd35466457a` also passed the all-target compile check, but its resolver safety review failed and repairs remain required. This evidence does not approve that candidate for merging.

The four-PR candidate passed formatting, all-target compilation, strict all-target Clippy, test compilation, 18 OS recording checks, three process outcome recording checks, and six architecture fixtures. CLI, checkpoint CLI, MCP stdio, standalone CLI and Linux OS suites passed in a PID namespace.

The archived full library run reported **4746 passed, 52 failed, 22 ignored**. It used Git 2.48.1 without HTTP, LFS or subtree helpers and reused temporary fixture paths across PID namespaces. Follow-up groups passed after supplying Git 2.50.1 with HTTP/subtree support, Git LFS 3.4.1, and fresh temporary paths: drafts, recovery, deferred drafts, subtree, push preflight, activity redaction, and checkpoint. Two ACL filesystem checks passed outside the user namespace; their alternate fixture UIDs are unmapped inside it. A profile check also passed with isolated Git/app configuration. These are focused reruns, not a new full-suite pass. The initial checkpoint failure did not reproduce; its production and test files equal dev. A possible existing racy-index weakness was identified by static review without attributing it as the proven cause.

Runtime runs used `bwrap --unshare-pid --die-with-parent --bind / / --dev /dev --proc /proc`. The two ACL checks acted on their own temporary filesystem fixtures. No host-wide process selector or signal was used. Earlier runs without a fresh /dev had pervasive permission failures and were superseded as environment-invalid checks; they are not application passes.

The manifest hashes raw and compressed logs. Logs remain bound to their actual source commit. Final combined gates and independent review must be recorded separately after resolver repairs. No CI was started or awaited during this implementation.
