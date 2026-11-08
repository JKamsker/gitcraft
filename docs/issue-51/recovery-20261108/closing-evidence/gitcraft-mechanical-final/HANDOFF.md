# Mechanical Fluent lint handoff

Final commit: `8733bd8be5fe2a23edab0da99efd956aca9befa9`. Base: `cbafbbac903456da94b69d5eb2eee51115fe2005`. Branch: `t3/mechanical-clippy-cleanup`.
Status: source reviewed and all-target compiler validated within assigned mechanical scope; root Clippy remains blocked as listed below.
Both author and committer are Jonas Kamsker <11245306+JKamsker@users.noreply.github.com> on every published unit. Only the verified `https://github.com/JKamsker/gitcraft` remote was pushed. Published WIP commits were retained; no history was rewritten. The final remote SHA equals this commit. No agents, CI, PR operations, test execution, application runtime, process/native fixtures, or Git-under-test fixtures were used.

## Patch and retention proof

`8733bd8be5fe2a23edab0da99efd956aca9befa9-minimal-vs-base.patch` is the source-only diff against the exact assigned base. SHA-256: `0a58f5c244308a3ff15619c4124eb5a29ff740dd04a5713697a193d54a7ac183`.
180 Rust source files changed; catalogs, raw string literals, production literal tokens, and all hard-excluded files remain unchanged. All unmodified tracked paths retain their original bytes. Rustfmt 1.95.0 used edition 2024 with skip_children=true on changed files only. Inspection found no unrelated formatting-only diff fragments; the one documentation fragment removes the diagnosed empty line.

The bound retention proof is `8733bd8be5fe2a23edab0da99efd956aca9befa9-retention-proof.json`. Changed files contain 11,950 retained production literal tokens, 1,666 retained assertion macros, 311 retained test names, and 1,721 verified production function bodies. All 22 hard-excluded files are byte-identical to the base. Every production token stream equals the original after replaying the recorded compiler suggestions and explicit import cleanup (ignoring comments/whitespace and rustfmt's canonical punctuation). Assertion predicates and expected values are identical after normalizing only clone and fallible-accessor syntax. The final documentation follow-up is bound by identical non-comment tokens and unchanged hashes for every other source file.

Permitted mechanical edits: compiler-identified noop/Copy clones (including required dereferences), redundant borrows, let-chain collapse, no-question-mark closure-to-block simplification, redundant closures/conversions, and import cleanup. Production APIs/types, algorithms, receipt schemas, dispatchers, classifiers, and literals were not redesigned.

Necessary test-scaffolding exceptions:
- 59 diagnosed test functions return Result<(), String>; failed Option/Result extraction propagates an error. Existing expectation-message literals are retained. No assertion, expected output, or test name was removed or weakened.
- Five existing unit-returning UI callbacks run a fallible inner closure and record failures for the enclosing test; initial font-install returns now return Ok(()) inside that wrapper. Callback draws and assertions retain their order.
- The value-producing `compose` closure in package/run/witness_tests.rs and supplied-input `click` helper in repo/push_preflight.rs return Result; callers propagate errors.
- The incomplete-ref test's panic tripwire uses Cell plus an explicit Err instead, retaining its original failure text. Other diagnosed test-only panic branches return explicit Err. No fallback successful value was introduced.
- Five inert test state setups use field initializers with the same values; no tested production behavior changes.
- Complete test modules in fence/display.rs, git/dry_run/reach.rs, and parallel_bisect/session.rs move after runtime declarations. Assertions and test names remain intact.
- Unused reexports with test consumers are kept under cfg(test); wholly unused reexports are removed, while their definitions remain.

## Exact-candidate command receipts

Every command used CARGO_BUILD_JOBS=2, RUSTUP_AUTO_INSTALL=0, the private target /tmp/gitcraft-mechanical-final/target, --locked and --offline. No toolchain was installed. Only compiler checks ran; no test execution is claimed.

- `cargo +1.95.0 check --locked --offline --all-targets --message-format=json` — exit 0; `8733bd8be5fe2a23edab0da99efd956aca9befa9-all-target-check.jsonl` and bound receipt.
- `cargo +1.95.0 clippy --locked --offline --all-targets --message-format=json -- -D warnings` — exit 101; `8733bd8be5fe2a23edab0da99efd956aca9befa9-strict-clippy.jsonl` and bound receipt.
- `cargo +1.95.0 clippy --locked --offline --all-targets --message-format=json` — exit 101; `8733bd8be5fe2a23edab0da99efd956aca9befa9-normal-clippy.jsonl` and bound receipt.

Normal Clippy has exactly 2 denied errors: src/repo/operations.rs:1928 (clippy::unwrap_used); src/repo/operations.rs:1959 (clippy::expect_used). They are hard-excluded and parent-owned. Normal Clippy therefore fails; this is not a clean-root receipt.

Strict Clippy has 136 distinct source-span diagnostics, versus 905 at the exact supplied base (not the older c2da523e log). Counts are deduplicated across lib and lib-test emissions. 769 distinct diagnostics were removed. No remaining noop/Copy clone, borrow, collapsible-if, unwrap/expect/panic diagnostic exists in permitted paths. The raw compiler logs preserve target-specific emission counts.

No lint suppression, global allow flag, public boxing, schema changes, or production invariant removal was used. The original #[expect(dead_code)] on CredentialCommandPolicy::bind_git is untouched; the normal and strict root receipts preserve the unfulfilled expectation finding at src/git/command/credential_policy.rs:13.

## Why remaining diagnostics were not changed

- Hard-excluded sources remain parent-owned, including activity clones, gerrit imports, notice drain_collect, devcontainer test initialization, messages item order, operations denied test accessors, and the command credential-policy expectation.
- `package::witness::Failure` produces 18 result_large_err findings (largest variant at least 168 bytes) in package/archive.rs, package/tools.rs, package/mod.rs, and repo/package/mod.rs. Boxing/redesigning these errors would change types/schemas and is outside scope.
- `Entry::Repo(Action)`, `DoctorAct::Confirm(Confirm)` and `PaletteRun::Action(Action)` produce three large_enum_variant findings (at least 272 bytes). Their representation was preserved.
- parallel_bisect/bundle.rs::run_with_presentation has eight arguments. Changing its signature would expand scope.
- 88 dead_code findings retain existing definitions/API compatibility. No definitions or meaningful narrow expectations were removed to silence them.
- Raw dispatcher match syntax remains in authorship/display.rs, drift/display.rs, and weakened/display.rs because changing dispatcher/control bodies is prohibited.
- app/workspaces.rs retains push_str("\n") because its suggested fix changes a production literal token.
- app/dropped.rs and package/run/witness_tests.rs retain the existing assertion forms (cloned_ref_to_slice_refs and bool_assert_comparison).
- standalone/windows_metadata.rs retains the manual-strip test oracle because changing its control/calculation would alter existing expectation scaffolding beyond fallible error handling.

## Remaining exact paths and lint types

- `src/across/discover.rs`: dead_code at 38.
- `src/app/dropped.rs`: clippy::cloned_ref_to_slice_refs at 620.
- `src/app/palette/items.rs`: clippy::large_enum_variant at 47.
- `src/app/recent_status.rs`: dead_code at 49.
- `src/app/workspaces.rs`: clippy::single_char_add_str at 1028.
- `src/authorship/display.rs`: clippy::match_single_binding at 7; dead_code at 6, 13.
- `src/authorship/mod.rs`: dead_code at 84.
- `src/custom_commands/program.rs`: dead_code at 133, 138.
- `src/drift/display.rs`: clippy::single_match at 7.
- `src/drift/mod.rs`: dead_code at 79, 107.
- `src/fence/display.rs`: dead_code at 311.
- `src/gerrit.rs` (EXCLUDED): unused_imports at 9.
- `src/gerrit/display.rs` (EXCLUDED): dead_code at 5, 31, 39.
- `src/git/bundles.rs`: dead_code at 232.
- `src/git/command/credential_policy.rs` (EXCLUDED): unfulfilled_lint_expectations at 13.
- `src/git/dry_run/picks.rs`: dead_code at 101.
- `src/git/dry_run/reach.rs`: dead_code at 187, 190, 199, 209.
- `src/git/dry_run/version.rs`: dead_code at 31.
- `src/package/archive.rs`: clippy::result_large_err at 29, 179; dead_code at 25.
- `src/package/mod.rs`: clippy::result_large_err at 85, 98, 128, 165, 174; dead_code at 81, 94, 118.
- `src/package/run.rs`: dead_code at 109, 142.
- `src/package/run/witness_tests.rs`: clippy::bool_assert_comparison at 57.
- `src/package/tools.rs`: clippy::result_large_err at 21, 187, 223, 253, 266, 322, 408, 452; dead_code at 13, 75, 180, 219, 249, 318.
- `src/package/witness.rs`: dead_code at 85, 177, 96.
- `src/parallel_bisect/bundle.rs`: clippy::too_many_arguments at 37; dead_code at 16, 181.
- `src/parallel_bisect/display.rs`: dead_code at 5, 317, 515, 550, 688, 697, 707, 713, 734, 759, 842.
- `src/parallel_bisect/observe.rs`: dead_code at 160.
- `src/parallel_bisect/session.rs`: dead_code at 140.
- `src/parallel_bisect/verdict.rs`: dead_code at 22.
- `src/parallel_bisect/workspace.rs`: dead_code at 209.
- `src/practice.rs`: dead_code at 28.
- `src/push_preflight/display.rs`: dead_code at 518, 523.
- `src/release/mod.rs`: dead_code at 302.
- `src/remote/display.rs`: dead_code at 1658, 1686, 1696, 1708.
- `src/repo/activity/mod.rs` (EXCLUDED): clippy::clone_on_copy at 181, 222, 252, 272, 276, 281, 282; noop_method_call at 180, 184.
- `src/repo/activity/view.rs` (EXCLUDED): clippy::clone_on_copy at 215, 219, 223.
- `src/repo/dco/view.rs`: dead_code at 79.
- `src/repo/devcontainer/mod.rs` (EXCLUDED): clippy::field_reassign_with_default at 516.
- `src/repo/dialogs/behaviour.rs`: dead_code at 20.
- `src/repo/doctor/mod.rs`: clippy::large_enum_variant at 71.
- `src/repo/ghost/mod.rs`: dead_code at 278, 570.
- `src/repo/ghost/plan.rs`: dead_code at 83.
- `src/repo/local_ci/model.rs`: dead_code at 41.
- `src/repo/local_ci/run.rs`: dead_code at 9.
- `src/repo/messages.rs` (EXCLUDED): clippy::items_after_test_module at 1190.
- `src/repo/notice.rs` (EXCLUDED): clippy::drain_collect at 183; dead_code at 107.
- `src/repo/operations.rs` (EXCLUDED): clippy::expect_used at 1959; clippy::unwrap_used at 1928.
- `src/repo/package/mod.rs`: clippy::result_large_err at 68, 100, 120.
- `src/repo/palette/mod.rs`: clippy::large_enum_variant at 34.
- `src/repo/parallel_bisect/marks.rs`: dead_code at 124.
- `src/settings/mod.rs`: dead_code at 260.
- `src/standalone/windows_metadata.rs`: clippy::manual_strip at 583.
- `src/teleport/mod.rs`: dead_code at 192.
- `src/terminal/display.rs`: dead_code at 5, 16, 36, 62.
- `src/tray/mod.rs`: dead_code at 28.
- `src/update/display.rs`: dead_code at 10, 95.
- `src/update/install.rs`: dead_code at 175, 197, 238, 264, 280.
- `src/update/previous.rs`: dead_code at 91, 122.
- `src/update/witness.rs`: dead_code at 36.
- `src/weakened/display.rs`: clippy::match_single_binding at 7.
- `src/weakened/mod.rs`: dead_code at 26, 111.

Machine-readable diagnostics retain complete compiler explanations and exact source locations. All logs, proof scripts, and suggestion ledgers remain under /tmp/gitcraft-mechanical-final.
