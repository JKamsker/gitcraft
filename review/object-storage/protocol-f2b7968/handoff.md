# Updated static candidate f2b7968

Product source **f2b7968b45d90afc1bd2f7d2347482a2463f0742** supersedes 9191812.
Exact author and remote SHA verified; working tree clean. Original handoff and
immutable failed c805f6e history remain preserved. See ../protocol-9191812/handoff.md
for unchanged protocol entrypoint/availability/ownership/outcome inventory. This
candidate's manifest pins every changed product file and all current compile logs.

Windows hard-link correction requested by root: the actual alternate's exclusive
held handle must report exactly one hard link at capture and every current/proof
check. Query uses the safe winapi-util::file::information API; version 0.1.11 was
already locked/transitive and is now a direct Windows-only dependency. No new unsafe
call or permission is introduced. The existing no-DELETE-sharing handle prevents
other handles obtaining DELETE access to add/remove links while capture/checks/action
hold it. A native fixture rejects two clones sharing a hard-linked alternate and
checks both files/content unchanged. Existing native ownership fixture also verifies
new hard-link creation is denied while its plan holds the alternate. These fixtures
have NOT executed; final native runtime gates remain required.

Exact fmt check and locked strict all-target Clippy PASS. Actual Windows held source
compiles with safe dependency via isolated metadata-only cross-target Cargo check,
warnings denied. The included adapter stubs existing Git/final-path facade signatures;
it does not replace the actual held module or claim native runtime. New helper protocol
runtime and full tests remain deferred until independent static PASS.

Additional review focus discovered while reviewing Preflight: this candidate's helper
uses existing comparison_remote -> remote::exec. That inherited transport's timeout
can invoke its existing legacy stop selector. No new selector was added, but do not
assume that the lifecycle suppresses it. Root and reviewer were alerted before any
new protocol runtime. A corrected independently accepted shared owned transport is
the preferred route; no unreviewed sibling dependency has been imported and no
competing edit to shared remote/session source was made. This remains an explicit
static review concern before runtime authorization.

No helper protocol fixture, native Windows fixture, full test suite, CI, live host,
account operation, donor write or user-repository mutation was executed for this
candidate. Earlier Linux full gates remain bound only to a3b4d7d, not this source.
