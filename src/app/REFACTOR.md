# App decomposition for issue #198

Base: `a4d841713af3d31178a3b9212fd3c5adcb4172dc`. This is the `src/app/**` slice only; issue #198 remains open.

## Ownership

- `updates::Discovery` privately owns discovery receivers, settings identity, completion status, and invalidation. `App` composes it with the existing update banner and persisted version choices. Receiver retirement still discards stale reads; this does not change executing mutations.
- `windows::Window` retains the complete swappable window session and its saved representation. `windows::desktop` owns the deferred viewport coordinator, callback cache, and eframe lifecycle. `windows::routing` handles restoration and live-session transfers.
- Inbox, workspaces, my-work, scan, clone, remote-open, setup, profile, and recent-repository renderers compose existing state owners and return existing requests. Row layout/status helpers stay with their presentation responsibility.
- Native title-bar border adapters, caption glyphs, and resize hit testing have separate platform/presentation responsibilities.
- The root shell separates construction, command routing, repository opening, frame composition, and lifecycle/persistence. `Saved`, public entry points, and serialized field names/defaults remain compatible.
- External test modules retain their original Rust module names. Large window/application test suites include behavior-specific fixture files into the original test scope, preserving Windows shard selectors. The status-part test accessor keeps presentation fields private.

## Measurements

Physical lines include comments and blanks. Every Rust file in this slice is below the hard cap; 300–500-line files remain candidates for further responsibility extraction.

| Original file | Base lines | Result lines |
| --- | ---: | ---: |
| `src/app/clone.rs` | 995 | 395 |
| `src/app/dropped.rs` | 681 | 322 |
| `src/app/home.rs` | 722 | 462 |
| `src/app/home_recent.rs` | 569 | 195 |
| `src/app/inbox.rs` | 1102 | 462 |
| `src/app/mod.rs` | 1184 | 244 |
| `src/app/my_work.rs` | 976 | 465 |
| `src/app/panels.rs` | 521 | 344 |
| `src/app/profiles.rs` | 509 | 299 |
| `src/app/remote_open.rs` | 720 | 398 |
| `src/app/repo_scan.rs` | 818 | 442 |
| `src/app/setup/mod.rs` | 596 | 413 |
| `src/app/tab_strip/mod.rs` | 708 | 425 |
| `src/app/teleport/view.rs` | 543 | 421 |
| `src/app/tests.rs` | 547 | 29 |
| `src/app/title_bar.rs` | 661 | 272 |
| `src/app/windows.rs` | 1910 | 277 |
| `src/app/workspaces.rs` | 1197 | 407 |

Across the slice: 53 → 119 Rust files; 18 → 0 over 500; maximum 1910 → 482 lines.

## Verification and limits

- `rustfmt --edition 2024 --check` across every `src/app/**/*.rs`: passed, including included fixture files.
- `git diff --check`: passed.
- Scoped physical-line inventory: zero files above 500.
- Static module/include traversal: all 220 original qualified test names preserved; two discovery-owner tests added.
- The complete `Saved` declaration, including serde attributes and defaults, is byte-identical to the base.
- Static function-body comparison accounts for relocation paths, formatter-only closure changes, the discovery-owner extraction/constructor/invalidation, the source-test path update, and private presentation test accessors. This is static evidence, not application test execution.
- Two channel-only discovery tests passed under `rustc --test` using the actual `Discovery` implementation and tests, with minimal stubs for update/network/settings/egui boundaries. They exercise one-time completion, retained errors, successful clearing, and retired receiver isolation. They do not exercise the external boundaries or replace full-crate tests.
- No Cargo build, full application/UI suite, Windows/macOS execution, CI, process-control test, push, PR, or independent adversarial review was performed by this implementation sub-agent. Integration owner must run combined build/tests and review on the assembled candidate before pushing.

Rust source inventory fingerprint (SHA-256 over sorted path, NUL, raw bytes, NUL): `6c0c9395a71b609531b36653704ec6926f45a0b7e0f27332cbbccda293b2932e`.

Remaining issue acceptance work outside this slice includes the repository-wide checker/CI/instructions, other oversized Rust sources and architectural owners, cross-platform validation, and final independent review. Existing #193–#195 work is preserved; these issues are not claimed complete.

## Modified paths

- `src/app/REFACTOR.md`
- `src/app/clone.rs`
- `src/app/clone/fields.rs`
- `src/app/clone/localization_tests.rs`
- `src/app/commands.rs`
- `src/app/construction.rs`
- `src/app/dropped.rs`
- `src/app/dropped/localization_tests.rs`
- `src/app/dropped/recording_tests.rs`
- `src/app/dropped/tests.rs`
- `src/app/frame.rs`
- `src/app/home.rs`
- `src/app/home/cards.rs`
- `src/app/home/retained_error_tests.rs`
- `src/app/home/tests.rs`
- `src/app/home_recent.rs`
- `src/app/home_recent/localized_status_paint_tests.rs`
- `src/app/home_recent/row.rs`
- `src/app/home_recent/tests.rs`
- `src/app/inbox.rs`
- `src/app/inbox/accessibility_tests.rs`
- `src/app/inbox/localization_tests.rs`
- `src/app/inbox/round.rs`
- `src/app/inbox/row.rs`
- `src/app/inbox/tests.rs`
- `src/app/inbox/view.rs`
- `src/app/lifecycle.rs`
- `src/app/mod.rs`
- `src/app/my_work.rs`
- `src/app/my_work/controls.rs`
- `src/app/my_work/localization_tests.rs`
- `src/app/my_work/view.rs`
- `src/app/panels.rs`
- `src/app/panels/home_error_toast_tests.rs`
- `src/app/panels/notice_toast_tests.rs`
- `src/app/panels/remote_display_tests.rs`
- `src/app/panels/tests.rs`
- `src/app/panels/updater_toast_display_tests.rs`
- `src/app/profiles.rs`
- `src/app/profiles/localization_tests.rs`
- `src/app/profiles/view.rs`
- `src/app/remote_open.rs`
- `src/app/remote_open/fields.rs`
- `src/app/remote_open/localization_tests.rs`
- `src/app/remote_open/tests.rs`
- `src/app/repo_scan.rs`
- `src/app/repo_scan/localization_tests.rs`
- `src/app/repo_scan/tests.rs`
- `src/app/repo_scan/view.rs`
- `src/app/repositories.rs`
- `src/app/setup/mod.rs`
- `src/app/setup/view.rs`
- `src/app/startup_display.rs`
- `src/app/tab_strip/actions.rs`
- `src/app/tab_strip/geometry.rs`
- `src/app/tab_strip/mod.rs`
- `src/app/tab_strip/tests.rs`
- `src/app/teleport/view.rs`
- `src/app/teleport/view/completion_tests.rs`
- `src/app/teleport/view/localization_tests.rs`
- `src/app/tests.rs`
- `src/app/tests_cases/handoff.rs`
- `src/app/tests_cases/launch_commands.rs`
- `src/app/tests_cases/persistence.rs`
- `src/app/title_bar.rs`
- `src/app/title_bar/glyph.rs`
- `src/app/title_bar/native.rs`
- `src/app/title_bar/resize.rs`
- `src/app/title_bar/tests.rs`
- `src/app/update_banner.rs`
- `src/app/updates.rs`
- `src/app/windows.rs`
- `src/app/windows/desktop.rs`
- `src/app/windows/routing.rs`
- `src/app/windows/tests.rs`
- `src/app/windows/tests_cases/close_contention.rs`
- `src/app/windows/tests_cases/completion_ownership.rs`
- `src/app/windows/tests_cases/dialog_ownership.rs`
- `src/app/windows/tests_cases/drag_transfer.rs`
- `src/app/windows/tests_cases/geometry.rs`
- `src/app/windows/tests_cases/resident.rs`
- `src/app/windows/tests_cases/restoration.rs`
- `src/app/windows/tests_cases/sessions.rs`
- `src/app/workspaces.rs`
- `src/app/workspaces/localization_tests.rs`
- `src/app/workspaces/row.rs`
- `src/app/workspaces/view.rs`
- `src/app/workspaces_tests.rs`
