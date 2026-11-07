# Recovered Codex increments — 7 November 2026

The integration starts at `dev` (`d82b2586aa073d23bc031717576d883d02bd187c`). All 57 fetched `codex/*` branch tips were inventoried and preserved. The source consolidation `ab68bdedd387236c03f26f839572860d405a2f7f` was based on older dev (`74027116e8f20334716a6b1208717bb33c9c6280`); its historical checks do not validate today's combined application. The recovery branch `c88eb285962ce885c5682430cf568640f959a6c9` explicitly describes its contents as backup/evidence rather than an application merge.

## Included

| Feature | Frozen source tip | Source integration delta | Final feature commit |
| --- | --- | --- | --- |
| Gerrit review submission and amendments | `c08ff3b` | `90d2521` | `43ee866` |
| Theme and painted-text contrast | `1a77cfd` | `419ade0` | `697ae7a` |
| Read-only structural diffs with Difftastic | `c4971a8` | `234000a` | `5f58cb5` |
| Graph columns, lane labels and full-message tooltips | `05d818a` | `14bcdd2` | `25854b0` |
| Bounded Git bundle export, inspection and atomic import | `6625a2c` | `a175310` plus accepted `ab68bde` bundle fixes | `fb88026` |

Full SHAs and the complete branch inventory are recorded in [the validation record](validation/codex-recovery-2026-11-07.json).

The ports retain current dev's operation expectations and cancellation controls, push rejection handling, profile-specific Git environment, blame/search behavior, rich diff views, graph overlays, first-parent layout, keyboard and drag interactions. Structural views remain read-only, and switching diff modes clears incompatible rendering state. Bundle imports create new refs atomically and revalidate the exact previewed bytes; they do not check out work or overwrite existing refs. No Cargo dependency or CI workflow change is included.

## Review and exclusions

Round one reviewed `112c86e8fb4e6e62bf93e9a9996411d65d389abc` and found four blockers. Gerrit Sync could bypass review confirmation, amend preparation dropped sign-off and hook choices, and the review dialog lost the push hook choice. All three Gerrit paths were corrected and covered by regression tests. The fourth finding was broader: selecting history in a blobless clone could download missing blobs before the new consent dialog. The entire clone-options feature is excluded pending a shared policy across passive readers.

Practice repositories are deferred because their old isolation layer does not cover current dev's additional Git execution routes. The standalone-file-tools checkpoint depends on the older file-comparison/connection stack. Onboarding, profiles/connections, SSH helper, content recovery, file comparisons and encrypted diffs require a dedicated port and fresh combined review of that stack. Weather, keyboard staging, activity and working drafts overlap current dev and need separate review of their remaining deltas. The remaining feature branches are WIP or lack validation/review against current dev. These decisions describe readiness for this integration, not a judgment that every deferred implementation is defective. Review, transport, containment and recovery branches remain available as evidence.

Round two independently reviewed all five features at `6489f81d180e5a53ea73fbe8fd0861fae20eea45`, tree `507661d2fada3a255a30823a6be6f95d5cc78d9e`, and passed with no blocking findings. It also independently exercised Difftastic 0.72.0's actual external-diff protocol on Unicode Rust. The final feature history has the identical tree to the preserved checkpoint `0963b41fe1499859e3e17a2a1debeb775c7ab10a` (`t3/codex-integration-checkpoints-20261107`).

## Validation

On macOS arm64 with Rust 1.95.0, the complete locked test command passed **3,050 tests, zero failures, seven existing ignores, zero filtered tests** (3,042 library tests plus eight executable/protocol/OS-helper tests). The run included installed Difftastic 0.72.0; its live-tool structural tests did not take the missing-tool early return. The synthetic command-argument/confirmation gate and Gerrit sign-off/hook regression tests also passed.

`cargo fmt --check` and the all-target test build passed. All-target Clippy passed with `-D warnings -A dead_code`; the allowance covers six pre-existing macOS-only dead-code warnings listed in the validation record. This is not a claim that unmodified strict Clippy passes on macOS. The complete test output is preserved as a compressed log alongside the validation record, with its SHA-256 and binary digest.

The first native run exposed five failures that also reproduced on unchanged dev, plus an existing process assertion that passes serially. The final environment uses GNU sed 4.10 from a temporary directory, canonical `TMPDIR=/private/tmp`, and a small shell wrapper that preserves the validation PATH rather than loading machine-specific login profiles. Serial tests avoid overlap between existing process fixtures. One existing Git regex test now uses portable POSIX whitespace (`[[:space:]]`) instead of the nonportable `\s`; production search code is unchanged. The validation record preserves these diagnostics rather than treating the first run as a pass.

Windows and live SSH/WSL runtime execution remain unverified locally. The seven existing ignored tests require live forge services or the system credential store. Documentation-only additions after the pinned application candidate retain its application gates; they are not a claim of new application test execution.
