Validated application correction: `3cef22c2110ce42bbaa83465e8e5a6a5f81a3efa`.
Review base: `ff901aad4346ee409509b60f9c6e7b12689fde44`.

DRAFTS-1 from the independent review of `51ea049` is corrected with per-request cancellation, one running/latest-pending reader, and actual completion tickets for detached remote work. The 15-second caller/request timeouts are not treated as an end-to-end completion bound. Tickets cover pool/connect waits and nested cleanup until the underlying worker actually returns. The manifest contains the exact proof trace and source/log hashes.

All local gates passed: fmt, strict all-target Clippy, synthetic completion test, 6 focused UI/lifecycle tests, and full suite **816 passed / 3 ignored / 0 filtered** (819 total). All 27 new feature/helper regressions executed. Original failed review and its source/evidence history remain preserved. Independent re-review is pending.

This commit adds evidence only; application source is unchanged from the validated correction. Raw logs are preserved verbatim, including native trailing blank lines. No CI was used, dispatched, rerun, polled or awaited; the user has deferred it until all final work and local acceptance are complete.
