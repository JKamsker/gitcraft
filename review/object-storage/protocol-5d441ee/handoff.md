# Static handoff: object storage and clone independence

Frozen product **5d441ee3c40dc3bc942d429781617189b8c2d52f**, codex/object-storage.
Exact author/remote SHA verified; manifest pins tree and all changed files. Source
complete, **uncompiled/runtime unvalidated**. Direct rustfmt check PASS only.

Entrypoints: inspector toolbar/palette -> git::object_storage::inspect -> independent
prepare/explicit make; local uses retained Linux/Windows ownership. Remote uses
helper::prepare/execute -> exchange -> generic remote::exec_owned. Supplied headless
binary src/bin/gitcraft-object-storage-helper.rs -> app::object_storage_helper ->
helper::entrypoint/serve/process. Deb/rpm assets and README matching-build/PATH setup
remain supplied. No automatic installation, fetching, donor writes or cleanup.

Changes since f2 static FAIL:

* Generic owned source from preserved881a2ff (e5 scoped static provenance), no
  competing edits: synchronous actual queue/connect/send/result, presend active
  check, no legacy stop or retry after partial send. Completion scope remains until
  actual transport returns; transport Completed alone is not feature success.
* Each prepared action is single-use. Bounded64 pending registry retains captured
  owner/Context/ticket and blocks replacement on Unknown/mismatched acknowledgement.
* Only exact protocol+transaction completion succeeds. Complete server error and
  known NotDispatched retire this attempt; uncertain/malformed outcomes retain it.
* Helper-only receipts: local::make_receipted_transaction publishes full ticket/result
  after copy/proof/recovery in actual held private Stage. Linux publishes a flushed
  fresh file with create-only descriptor link; Windows Stage uses its held no-replace
  rename wrapper. Existing local action without receipt keeps original algorithm.
* Inspect resolves pending via a fresh owned reader under original captured Context.
  helper/receipt.rs opens every actual ancestor/.git/objects/info/pack NOFOLLOW,
  compares device/inode/birth-time identities against the ticket, reads a bounded
  regular receipt through actual stage/file handles, rechecks named identities,
  and requires exact full ticket/result. Missing/incomplete/stale receipt is never
  completion. Early failures can require deliberate host recovery and reopening app.
* Windows51f95 no-original-content-write rename retains its scoped STATIC PASS2b801c5;
  hardlink/collision/newer-file recovery source cases remain native-unexecuted.

Fixture SOURCE includes pure wire/schema/cancel/one-shot/owned scope/Unknown barrier
cases, actual helper ticket recapture refusals, identity-bound terminal receipts,
local/Windows donor removal/recovery and fake SSH/WSL full protocol round trips.
No new helper/native/owned-shell fixture or full suite has executed. Shared seven
pure recordings bind only to prior e5 binary; earlier full886+3 binds only a3b4d7d.

Request independent STATIC review of this adapter/receipt delta before any protocol
execution; then pure recordings, owned temporary repositories and fake transports,
exact fmt/strict Clippy/full tests/list; native Windows gates still required.
No CI/live hosts/accounts/user-repository mutations. Old blocked c805 harness remains
prohibited. f2 FAIL89e7ee and all earlier source/review history remain immutable.
