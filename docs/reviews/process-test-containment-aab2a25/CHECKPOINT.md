# Independent static containment review — PASS, runtime pending

Exact pushed candidate aab2a257b8598f6718431b40cd1058d9a273bd2c. Full generated-script allowlist is asserted before any shell launch. Same-shell ps/kill/sleep functions record synthetic events only; clean absolute-shell launch cannot reach host ps/signals through the allowlisted script. Forward/reverse/empty tables and forbidden root/ancestor/sibling/unrelated cases have meaningful assertions. Independent source-literal checks passed; the fixture was not executed by this review.

Root authorization is still required for the targeted recording-only fixture phase. Default full suite and final independent adversarial review remain pending. This contains the test; it does not establish ownership of production host PID selectors. No old fixture or old binary may run.
