# Tour and portable setup validation

Application candidate: `b05e51c529c12cc66bb907772e66450cd4ad9cc8`

Accepted review base: `ff901aad4346ee409509b60f9c6e7b12689fde44`. This evidence unit changes only validation documents/logs; application bytes remain those of the candidate.

Fresh private gates passed: formatting, strict all-target Clippy,20 targeted regressions and the complete default suite (809passed,3ignored,0filtered;812total). Optional live SSH/WSL test environment variables were unset. There are no safety exclusions. The manifest binds all changed application files and bundled gate logs to SHA-256 hashes.

The UI round-trip exercises physical pointer interactions and actual local Git clone/restoration. Its deliberate profile activation runs in an exact-test subprocess so synthetic signing choices cannot affect unrelated parallel Git fixtures. Fake remote launchers only execute the local recording fixture shell; no external SSH/provider/account writes were used.

Independent adversarial review is pending. These gates do not authorize the final consolidated PR.
