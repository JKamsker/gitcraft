# Recovered Codex increments — 7 November 2026

The integration starts at `dev` (`d82b2586aa073d23bc031717576d883d02bd187c`). All 57 fetched `codex/*` branch tips were inventoried and preserved. The source consolidation `ab68bdedd387236c03f26f839572860d405a2f7f` was based on older dev (`74027116e8f20334716a6b1208717bb33c9c6280`); its historical checks do not validate today's combined application. The recovery branch `c88eb285962ce885c5682430cf568640f959a6c9` explicitly describes its contents as backup/evidence rather than an application merge.

## Included

The table records the original feature-port commits. Subsequent review narrowed bundle support to export and read-only inspection, and Syntax execution to local repositories; the current scope and gates are recorded in the final follow-up below.

| Feature | Frozen source tip | Source integration delta | Final feature commit |
| --- | --- | --- | --- |
| Gerrit review submission and amendments | `c08ff3b` | `90d2521` | `43ee866` |
| Theme and painted-text contrast | `1a77cfd` | `419ade0` | `697ae7a` |
| Read-only structural diffs with Difftastic | `c4971a8` | `234000a` | `5f58cb5` |
| Graph columns, lane labels and full-message tooltips | `05d818a` | `14bcdd2` | `25854b0` |
| Git bundle export and inspection (import subsequently deferred) | `6625a2c` | `a175310` plus accepted `ab68bde` bundle fixes | `fb88026` |

Full SHAs and the complete branch inventory are recorded in [the validation record](validation/codex-recovery-2026-11-07.json).

The ports retain current dev's operation expectations and cancellation controls, push rejection handling, profile-specific Git environment, blame/search behavior, rich diff views, graph overlays, first-parent layout, keyboard and drag interactions. Structural views remain read-only, and switching diff modes clears incompatible rendering state. The original bundle import was removed after follow-up review found that input limits and deadlines do not bound expanded pack memory. Current bundle inspection reads headers and prerequisites without installing objects; it explicitly leaves pack contents unverified. No Cargo dependency or CI workflow change is included.

## Review and exclusions

Round one reviewed `112c86e8fb4e6e62bf93e9a9996411d65d389abc` and found four blockers. Gerrit Sync could bypass review confirmation, amend preparation dropped sign-off and hook choices, and the review dialog lost the push hook choice. All three Gerrit paths were corrected and covered by regression tests. The fourth finding was broader: selecting history in a blobless clone could download missing blobs before the new consent dialog. The entire clone-options feature is excluded pending a shared policy across passive readers.

Practice repositories are deferred because their old isolation layer does not cover current dev's additional Git execution routes. The standalone-file-tools checkpoint depends on the older file-comparison/connection stack. Onboarding, profiles/connections, SSH helper, content recovery, file comparisons and encrypted diffs require a dedicated port and fresh combined review of that stack. Weather, keyboard staging, activity and working drafts overlap current dev and need separate review of their remaining deltas. The remaining feature branches are WIP or lack validation/review against current dev. These decisions describe readiness for this integration, not a judgment that every deferred implementation is defective. Review, transport, containment and recovery branches remain available as evidence.

Round two independently reviewed all five features at `6489f81d180e5a53ea73fbe8fd0861fae20eea45`, tree `507661d2fada3a255a30823a6be6f95d5cc78d9e`, and passed with no blocking findings. It also independently exercised Difftastic 0.72.0's actual external-diff protocol on Unicode Rust. The final feature history has the identical tree to the preserved checkpoint `0963b41fe1499859e3e17a2a1debeb775c7ab10a` (`t3/codex-integration-checkpoints-20261107`).

## Validation

On macOS arm64 with Rust 1.95.0, the complete locked test command passed **3,050 tests, zero failures, seven existing ignores, zero filtered tests** (3,042 library tests plus eight executable/protocol/OS-helper tests). The run included installed Difftastic 0.72.0; its live-tool structural tests did not take the missing-tool early return. The synthetic command-argument/confirmation gate and Gerrit sign-off/hook regression tests also passed.

`cargo fmt --check` and the all-target test build passed. All-target Clippy passed with `-D warnings -A dead_code`; the allowance covers six pre-existing macOS-only dead-code warnings listed in the validation record. This is not a claim that unmodified strict Clippy passes on macOS. The complete test output is preserved as a compressed log alongside the validation record, with its SHA-256 and binary digest.

The first native run exposed five failures that also reproduced on unchanged dev, plus an existing process assertion that passes serially. The final environment uses GNU sed 4.10 from a temporary directory, canonical `TMPDIR=/private/tmp`, and a small shell wrapper that preserves the validation PATH rather than loading machine-specific login profiles. Serial tests avoid overlap between existing process fixtures. One existing Git regex test now uses portable POSIX whitespace (`[[:space:]]`) instead of the nonportable `\s`; production search code is unchanged. The validation record preserves these diagnostics rather than treating the first run as a pass.

Windows and live SSH/WSL runtime execution remain unverified locally. The seven existing ignored tests require live forge services or the system credential store. The initial documentation-only delivery retained that pinned application candidate. Subsequent application changes and their own gates are recorded below.


## PR #164 Linux performance follow-up

Linux CI on `0e65092` passed 3,043 library tests but failed the existing 3,000-branch palette performance check: 12.93 seconds exceeded its unchanged 10-second limit. Adaptive graph sizing adds a badge calculation, exposing repeated scans of the full local/remote lists and previously accumulated badges. `2140f7e8ff5eef87fe218a45ccaac32d19942768` replaces those repeated scans with per-calculation lookup indexes. Exact remote precedence, local names under remote prefixes, tag separation, branch folding and stable display order are retained. A mixed 3,000-branch local/remote regression checks those properties.

The unchanged palette test took 5.77 seconds locally before and 0.92 seconds after the fix. All 77 palette and graph tests passed, as did the all-target test build, formatting and Clippy with the same documented macOS dead-code allowance. Independent review passed on `2140f7e`. The prior full native suite was not rerun for this focused change. Details and complete focused logs are in [the follow-up validation record](validation/pr164-palette-2026-11-07.json); updated Linux/Windows CI remains a separate gate.


## PR #164 Windows portability follow-up

Linux CI passed on `5529892`. Windows found three failures: native Git could not open the long bundle scratch pathname despite `core.longpaths=true`; the structural rename fixture used a double quote forbidden in Windows filenames; and the existing squash receipt test exhausted its ten-second completion wait after the rebase had finished. The aggregate Windows check reflected those shard failures.

Candidate `d73f132a929e8ded9d306f4ef5136b146ad56bcf` streams the exact validated bundle header only to supported local native Windows Git. Unix, SSH/WSL repositories and legacy Git retain pathname verification. An earlier all-platform stdin attempt failed connected-prerequisite verification locally and its initial review pass was revoked; it was corrected before updating the PR. Git's Unix and Windows subprocess handling differ here. No-lazy-fetch, strict pack validation and immutable preview protections remain intact. The rename fixture now proves Git quotes a Windows-legal Unicode name while retaining the Unix quote case. The receipt test waits up to thirty seconds for asynchronous completion and reports its state on failure; its functional assertions remain unchanged.

All 74 bundle, structural-diff and timeline tests passed on the final candidate, as did two synthetic command/argument checks, the all-target test build, formatting and Clippy with the previously documented macOS allowance. The receipt test also passed with an injected eleven-second post-rebase snapshot delay. Independent review passed. Native Windows execution remains pending; source-based reasoning is not reported as a Windows runtime pass. The [follow-up record](validation/pr164-windows-2026-11-07.json) preserves diagnostics, rejected-candidate evidence, tagged Git source links, review and exact-candidate logs.


## PR #164 review follow-up and current scope

The earlier `0d2d86d5` Windows portability delivery passed Linux tests, strict lint, Windows build and all ten Windows shards plus their aggregate. CodeRabbit then identified functional and containment issues. All 20 inline comments and 22 grouped findings/notes were triaged; the [review validation record](validation/pr164-review-2026-11-07.json) records each disposition. The bot's separate OSS review rate limit and inconclusive docstring metric were not treated as repository gates.

Current application candidate `f8cacb06cffaf8a942da6301c11e8265d52fec35`, tree `72c0c69df7930fed45bb49fab688789a0d281be3`, retains Gerrit submission/amendment, graph readability, contrast, local read-only Syntax, and bundle export/header inspection. Gerrit preparation now honors cancellation, selected-branch upstreams and symbolic-HEAD expectations; identity restoration preserves trailer blocks and invalid web overrides produce warnings. Rebuilds reuse graph indexes. Painted labels and selected intraline backgrounds have contrast checks. The full-message popup retains pointer interaction and its final paragraph is reachable by scrolling.

Bundle export refuses missing-object lazy fetch, checks selected refs against one listing, and logs completion while resuming queued operations. Unicode ref whitespace and legacy prerequisite descriptions are covered. **Bundle import is deferred**: compressed input and deadline limits do not contain `index-pack` memory allocation. Its UI, action and backend execution were removed; source branches and the immutable pre-review delivery retain the recovered work. Inspection never verifies or installs the pack. This also removes import collision, non-UTF8 worktree enumeration and external worktree-switch races from the active API.

**Remote Syntax is deferred** until remote cleanup can be independently bounded through disconnects. SSH/WSL use patch fallback before starting the tool. Local Syntax has one active comparison per tab and one coalesced replacement, bounded Git preparation with no lazy fetch, correct rename sources, aligned scrolling headings and working patch-fallback navigation/Find. Syntax remains read-only across toolbar and filename menus. Custom Windows Difftastic launchers still have best-effort descendant cleanup; no Windows Job or arbitrary-launcher containment is claimed.

The complete locked native suite passed **3,056 tests, zero failures, seven existing ignores, zero filtered tests**, including real Difftastic 0.72.0. All 98 focused tests, the all-target build, formatting and all-target Clippy with the existing six-warning macOS dead-code allowance passed. The environment uses the previously documented GNU sed, canonical temporary path and validation shell, and the full suite ran serially. Independent adversarial review passed on the exact candidate after rejecting an earlier review candidate for four issues, all subsequently corrected. The [compressed logs](validation/pr164-review-2026-11-07-tests.log.gz) and record bind these results to the immutable application commit.

Native Windows execution of this review candidate awaits CI; the preceding delivery's passing Windows checks are not reused as a runtime pass for these changes. Live SSH/WSL bundle export/inspection remain untested locally. No dependency or workflow change is included.


## PR #164 integration with updated dev

The next watcher message repeated the older review publication; all inline findings were already triaged. Read-only host state additionally revealed conflicts after `dev` advanced to `d7b3757ac4ed1e6a955c5845a898cb1bfca7ff11` through PRs #161, #162 and #165. Merge `80d96e7d` preserves upstream macOS/CPU/memory improvements, asynchronous subprocess I/O, bounded task pools and focused PR comparison/pill behavior. Seven conflict resolutions retain shared lane memory alongside lane names; deterministic branch priorities alongside cached lookups; the 70px minimum and shortening setting alongside automatic widths and contrast; and fixed-pool reads alongside Gerrit detection.

Independent review rejected the first integration candidate because recovered subprocess runners bypassed new tab cancellation, and generic background reads could strand bundle/message loading on queue rejection. Both local Git preparation/bundle work and Difftastic now use the shared process supervisor, retain independent stderr caps and inherit tab cancellation. Bundle file creation checks cancellation. Owner-checked read completions handle rejected, cancelled and panicked work. Preliminary argument and precancelled-loader tests passed before owned-child cancellation tests, which confirmed that closing a tab stops both preparation and tool execution without later marker effects.

Application candidate `978fb9bb97e69193caa0121664ff39a93165f133` passed independent review, the all-target build, formatting, Clippy with the previously documented native dead-code allowance, focused integration tests and the complete serial locked native suite: **3,124 passed, zero failures, 9 existing ignores, zero filtered**. The [integration record](validation/pr164-dev-merge-2026-11-07.json) and [compressed logs](validation/pr164-dev-merge-2026-11-07-tests.log.gz) pin these gates and rejected attempts. Native Windows execution awaits fresh CI; upstream Windows Job supervision is inherited, with no untested runtime claim. Bundle import and remote Syntax remain deferred. Earlier validation sections refer to their own immutable candidates, not this updated merge.


The final base also includes documentation-only `1ead560bc87805d65ccbd423215b96c949623228`. Its welcoming README and guide structure are retained; recovered graph, bundle, Syntax, Gerrit and contrast guidance moved into the corresponding guide pages. Both sets of agent notes are retained, with remote aliases qualified by worktree and all writes restricted to the fork. Application files, crates, manifests, lockfile, tests and workflows remain byte-for-byte identical to the validated application candidate; no test execution is attributed to this documentation merge.


## PR #164 subsequent CodeRabbit review

Review `5814242652`, originally against `f6840306`, added two actionable findings and two small improvements. Syntax errors now recommend a Git upgrade only for the unsupported no-lazy-fetch option; unrelated errors retain their original diagnostics. Ghost rows no longer show a full-message loading popup while their loader is disabled. Built-in contrast adjustment runs once before deriving selected diff tints. Graph rendering reuses an owned ref index for width calculation, pills and accessibility labels, refreshed alongside incoming refs and graph-label rebuilds. A regression changes a remote ref into a local/current branch without replacing the graph and checks the refreshed metadata.

Production candidate `f65685c3683e0cbf145b25f7df231d402d7309cd` passed the full serial locked native suite: **3,127 passed, zero failures, 9 existing ignores, zero filtered**, plus 228 focused tests and independent review. Review also found a test-only ignored CPU benchmark bypassed cache setup; follow-up `803037f8d09452d81b2a0e6e5437616dbdafc9e3` adds that initialization without changing production code. The follow-up passed the all-target build, 228 focused tests, formatting, Clippy with the documented native dead-code allowance, and independent review. The full suite was not repeated or attributed to that benchmark-only commit. No benchmark timing result is claimed. The [validation record](validation/pr164-polish-2026-11-07.json) and [compressed logs](validation/pr164-polish-2026-11-07-tests.log.gz) preserve the exact provenance. Fresh Linux/Windows CI remains separate.
