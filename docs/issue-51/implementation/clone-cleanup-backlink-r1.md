# Clone/cleanup R1 delta

Starting tip: `31f9cb2d8f166e51d641bd6ffe41e764cdffad05` (application
`be95891e049ceb07121c12813a936f27572b2467`). This delta responds only to
`CLONE-R1-1`, `CLONE-R1-2`, and `CLEANUP-R1-1` in the complete R1 review.
Backlink navigation retains its scoped source PASS and is unchanged.

- **CLONE-R1-1:** Sparse Apply captures the tab's credential value along with the
  existing managed task/profile context. After credential-free state/cleanliness
  checks, only the consented mutation passes it to accepted `run_with`. Without
  consent the account is omitted and no-lazy policy remains active. An inert
  recording exercises the production mutation seam, credential extras, profile
  binding and local/remote policy builders using a fixed executable; it does not
  instantiate a tab or launch Git. UI capture is verified by source inspection.
- **CLONE-R1-2:** Feature-local patterns reads accept at most 1 MiB of UTF-8 from
  one pinned regular file. Local Unix uses the accepted `8bd3e5` descriptor pattern
  (nonblocking/no-follow), checks size before reading, reads at most cap+1 and
  compares descriptor/path metadata after reading. Windows opens the reparse point
  with rename sharing denied and rejects reparse attributes. The remote route
  requires python3 and nonblocking/no-follow descriptors, checks type/size and
  descriptor/path identity, and withholds output until checks pass. Missing files
  remain empty patterns. HEAD/patterns/enabled/cone stale checks are preserved.
  This is not a filesystem transaction against changes after the final check.
- **CLEANUP-R1-1:** Residual checks use fixed `rev-list` / `cat-file` argv. Neither
  sensitive rule field is included in argv or object-ID stdin; matching is literal
  over in-memory blob bytes. Metadata is checked before requesting blob content:
  at most 20000 objects, 4 MiB per blob and 16 MiB total. Binary blobs (NUL in the
  first 8192 bytes), missing/malformed/truncated output, caps and command failures
  produce failed/unverified results. Messages, LFS, external copies and binary
  replacement remain unverified. Private rule retirement/redaction and separate
  publication/revocation guidance are preserved. This supersedes the earlier
  receipt's description of residual pickaxe checks.

Source-audited executable gates are limited to pure byte readers/matchers and
fixed-command recordings. The new owned regular/missing/oversized/link/FIFO
fixture and previous real Git/filter-repo/backlink fixtures are compile-only.
The authored FIFO fixture block requires Linux (`rustix::mkfifoat` is unavailable
on Apple); it is neither compiled nor executed by this macOS target.
No runtime clearance, CI, PR, external publication, supervisor change, shared
credential-policy fork or publisher is part of this delta. All generated evidence
stays outside the repository; a later plain JSON receipt binds gates and source
hashes to the preserved application commit. Gates are pending at this source unit.
