# Issue 198: direct repository integration slice

Base: `a4d841713af3d31178a3b9212fd3c5adcb4172dc`.
Ownership: direct `src/repo/*.rs`, plus new `tab_owner/`, `core_seams/`,
`direct_tests/`, `operation_plan/`, and `cochange_owner/` directories.
Existing child directories (including `terminals/`) were not edited.

## Delivered behavior and interfaces

All 30 oversized direct Rust files are under the mandatory 500 physical line
limit. Every new Rust file is also under the limit. Root integration modules
retain their names and re-export their existing APIs. The complete `RepoTab`
type and its construction/query boundary are separated; repository-private
fields retain exactly their former effective visibility through
`pub(in crate::repo)`. This is not a claim that all tab fields have feature owners.

Operation execution now decodes a typed `Plan`, retaining original source indices
and unchanged borrowed argv. Ref, symbolic-ref, tree, HTTPS route, fold and
feature-owned preconditions are explicit values. Ref/tree/symref validation
passes, HTTPS proof validation and step execution consume that plan. Validation
order, before/after-preparation rechecks, original route receipts, optional-step
failures, fork guards, mutation settlement, and the `i + 1` retry position after
successful remote creation remain intact. Reviewed push, preflight fix, Gerrit
amend, optional commands, expectations and ordinary Git commands have distinct
step kinds. Unknown/malformed markers retain their original error paths.

Persisted `UndoEntry` fields and serde attributes are unchanged. Public operation
entrypoints and pending/authentication retry storage still accept legacy argv.
This is a typed execution migration, not a new journal format or a migration of
every operation producer. Feature-specific guards retain their existing validators.

Both network classification and strict saved-command classification use
`git::parse_command_envelope<S: AsRef<str>>`. The strict adapter maps errors to
existing saved-operation messages. Network policy keeps approved-push handling
before grammar parsing, refuses to guess unknown/incomplete/internal envelopes,
and retains its remote/LFS policy. This requires Git owner commit
`3fd9f5eebee599e75e30ee58a83d2a5abc4e2aa5`; it is deliberately **not** cherry-picked
here. The parent must integrate that commit before full type checking.

Cochange suggestions now have a private owner for context binding, request
identity, cancellation/retirement, result acceptance, ranking, reset and teardown.
Immutable typed read effects leave the owner; completions return with monotonic
identity. Context is reconciled before accepting answers. A→B→A, out-of-order
success/error, hide/reopen, suspension/resume, repository changes with identical
HEAD, manual refresh, duplicate completion and completed-cache retention have
pure fixtures. Existing bounded `Reader` transport still owns cancellation and
worker settlement. No central Action/Msg variants or tab fields were added.

Read retirement is separate from operation settlement. A recording-only application
fixture verifies that a successful mutation keeps its undo-bearing timeline outcome
and releases busy state when the subsequent reload fails. That fixture is added
but awaits the parent application's test run.

## Validation evidence

The evidence applies to the source tree in the commit containing this note.
No Cargo build, application test run, risky process test, CI action, push or PR
was performed in this slice.

- `python3 src/repo/cochange_owner/check_transitions.py`: **6 passed**. Executes
  actual owner transition bodies, request identity and lifecycle fixtures with
  an inert recording-only Reader. It does not test the transport implementation.
  The actual production OID validator is included in the harness.
- `python3 src/repo/operation_plan/check_plan.py`: **3 passed**. Compiles the actual
  typed plan, marker predicates and retry-retention helpers. Covers validation
  operands/order, unchanged payloads, malformed markers and retry source indices.
- `python3 src/repo/operation_plan/check_grammar.py <Git-owner>/src/git/global_options.rs`:
  **8 passed**, using the Git owner's actual shared source read-only. Four are
  the owner's parser fixtures; four cover this slice's adapter/network policies.
- Recursive `rustfmt --edition 2024 --emit stdout src/repo/mod.rs`: module resolution
  and parsing passed. Changed/new Rust files separately pass formatting checks.
- Tree-sitter syntax inspection of every direct/new Rust file: zero syntax errors.
  All **1,388** original named function definitions remain present. Function-body
  comparison found the intended dispatch/ownership/typed-plan changes, explicit
  module qualification, and rustfmt changes; no original function was omitted.
- Static test discovery: **413 original tests retained**, **16 added**. **39** tests
  moved to responsibility suites (table below). Existing exact-name entries in
  `ci/test-shards.py` remain valid; in particular
  `repo::tests::the_command_line_opens_a_file_s_blame_at_a_line` is preserved.
- Physical source inventory: zero files above 500 in the owned direct/new paths;
  maximum 500 (`src/repo/branch_visibility.rs`, unchanged).
- `git diff --check`: passed.

## Required integration gates and remaining issue work

The parent must integrate the Git API and the other agents' slices, run the
full-tree size checker, compile all supported feature configurations, run the
combined application tests (including legacy serde/retry and mutation/reload
fixtures), verify compiled Windows shard discovery, and obtain independent
adversarial review on the exact final candidate. Linux/Windows, SSH/WSL/forge
behavior is not newly runtime-validated here. No independent review is claimed.

Remaining issue-wide work includes evidence from the other ownership slices,
full combined gates and CI. Existing query-text-based commit search and other
feature request lifetimes are not comprehensively migrated by this cochange
owner. Resolver recovery, settings/diff/preflight engine ownership and global
checker/CI policy are outside this slice. Do not mark issue 198 or 193–195 complete
based on this commit alone.

## Existing test-name changes

| Previous qualified name | Current qualified name |
| --- | --- |
| `repo::branch_cleanup::tests::nothing_to_offer_in_a_new_repository` | `repo::branch_cleanup::tests::branch_cleanup_safety::nothing_to_offer_in_a_new_repository` |
| `repo::branch_cleanup::tests::a_branch_in_use_in_a_worktree_stops_everything_before_it_starts` | `repo::branch_cleanup::tests::branch_cleanup_safety::a_branch_in_use_in_a_worktree_stops_everything_before_it_starts` |
| `repo::branch_cleanup::tests::only_a_remote_branch_of_the_same_name_goes_along` | `repo::branch_cleanup::tests::branch_cleanup_safety::only_a_remote_branch_of_the_same_name_goes_along` |
| `repo::branch_cleanup::tests::a_branch_that_moved_since_the_read_is_not_merged` | `repo::branch_cleanup::tests::branch_cleanup_safety::a_branch_that_moved_since_the_read_is_not_merged` |
| `repo::branch_cleanup::tests::cherry_answers_are_kept_between_reads` | `repo::branch_cleanup::tests::branch_cleanup_safety::cherry_answers_are_kept_between_reads` |
| `repo::forge_selection::tests::explicit_origin_then_first_usable_and_no_usable_remote` | `repo::forge_selection::tests::forge_selection_preferences::explicit_origin_then_first_usable_and_no_usable_remote` |
| `repo::forge_selection::tests::selection_follows_unique_rename_and_same_name_url_changes_but_removal_falls_back` | `repo::forge_selection::tests::forge_selection_preferences::selection_follows_unique_rename_and_same_name_url_changes_but_removal_falls_back` |
| `repo::forge_selection::tests::preference_survives_context_serialization_and_is_per_repository` | `repo::forge_selection::tests::forge_selection_preferences::preference_survives_context_serialization_and_is_per_repository` |
| `repo::operations::merge_pure_reviewed_echo_operands_are_strict_and_survive_retry` | `repo::operations::dispatch::merge_pure_reviewed_echo_operands_are_strict_and_survive_retry` |
| `repo::operations::tests::short_rebase::selected_submodule_scope_is_literal_and_excludes_ignored_siblings` | `repo::operations::tests::ignored_scans::selected_submodule_scope_is_literal_and_excludes_ignored_siblings` |
| `repo::operations::tests::short_rebase::value_consuming_submodule_options_never_narrow_the_ignored_scan` | `repo::operations::tests::ignored_scans::value_consuming_submodule_options_never_narrow_the_ignored_scan` |
| `repo::operations::tests::short_rebase::recursive_pull_inspects_nested_ignored_files_before_any_mutation` | `repo::operations::tests::ignored_scans::recursive_pull_inspects_nested_ignored_files_before_any_mutation` |
| `repo::operations::tests::selected_continuation_recording_retains_actual_route_receipt_and_expect_retry` | `repo::operations::tests::routes::selected_continuation_recording_retains_actual_route_receipt_and_expect_retry` |
| `repo::operations::tests::recording_reviewed_route_is_captured_before_dispatch_and_proves_one_shot_receipt` | `repo::operations::tests::routes::recording_reviewed_route_is_captured_before_dispatch_and_proves_one_shot_receipt` |
| `repo::operations::tests::recording_reviewed_auth_missing_foreign_or_unproven_routes_refuse_without_dispatch` | `repo::operations::tests::routes::recording_reviewed_auth_missing_foreign_or_unproven_routes_refuse_without_dispatch` |
| `repo::operations::tests::recording_https_route_proof_is_rechecked_after_draft_and_retained_for_retry` | `repo::operations::tests::routes::recording_https_route_proof_is_rechecked_after_draft_and_retained_for_retry` |
| `repo::operations::tests::recording_safety_draft_cannot_authorize_an_operation_after_its_context_moves` | `repo::operations::tests::routes::recording_safety_draft_cannot_authorize_an_operation_after_its_context_moves` |
| `repo::tests::a_linked_worktree_is_watched_where_git_keeps_its_data` | `repo::tests::watcher_integration::a_linked_worktree_is_watched_where_git_keeps_its_data` |
| `repo::tests::a_change_on_disk_reads_again_what_it_can_have_changed` | `repo::tests::watcher_integration::a_change_on_disk_reads_again_what_it_can_have_changed` |
| `repo::tests::headline_prefers_conflicts_and_errors` | `repo::tests::diagnostic_presentation::headline_prefers_conflicts_and_errors` |
| `repo::tests::a_pull_request_that_cannot_open_leaves_the_builds_shown` | `repo::tests::authentication_integration::a_pull_request_that_cannot_open_leaves_the_builds_shown` |
| `repo::tests::an_editor_command_with_arguments_is_run_with_the_path_as_one_argument` | `repo::tests::authentication_integration::an_editor_command_with_arguments_is_run_with_the_path_as_one_argument` |
| `repo::tests::alias_authentication_text_cannot_authorize_retry_of_remaining_steps` | `repo::tests::authentication_integration::alias_authentication_text_cannot_authorize_retry_of_remaining_steps` |
| `repo::tests::aborting_asks_first` | `repo::tests::authentication_integration::aborting_asks_first` |
| `repo::tests::unverified_alias_authentication_keeps_undo_without_authorizing_retry` | `repo::tests::authentication_integration::unverified_alias_authentication_keeps_undo_without_authorizing_retry` |
| `repo::tests::an_operation_that_panics_leaves_the_tab_idle` | `repo::tests::authentication_integration::an_operation_that_panics_leaves_the_tab_idle` |
| `repo::tests::a_drop_confirmed_while_busy_says_so` | `repo::tests::authentication_integration::a_drop_confirmed_while_busy_says_so` |
| `repo::tests::dropped_patches_are_applied` | `repo::tests::request_lifetimes::dropped_patches_are_applied` |
| `repo::tests::closing_a_tab_stops_its_background_child` | `repo::tests::request_lifetimes::closing_a_tab_stops_its_background_child` |
| `repo::tests::linked_worktrees_share_a_mutation_key_before_the_first_load` | `repo::tests::request_lifetimes::linked_worktrees_share_a_mutation_key_before_the_first_load` |
| `repo::tests::feature_failure_continuation_runs_when_a_queued_job_is_cancelled` | `repo::tests::request_lifetimes::feature_failure_continuation_runs_when_a_queued_job_is_cancelled` |
| `repo::tests::rapid_reloads_finish_and_coalesce` | `repo::tests::request_lifetimes::rapid_reloads_finish_and_coalesce` |
| `repo::tests::quick_drop_refuses_a_different_branch_at_the_same_commit` | `repo::tests::startup_and_mutations::quick_drop_refuses_a_different_branch_at_the_same_commit` |
| `repo::tests::remote_spawn_mutation_serializes_whole_recorded_operations` | `repo::tests::startup_and_mutations::remote_spawn_mutation_serializes_whole_recorded_operations` |
| `repo::tests::successful_prefetch_settles_omitted_and_oversized_entries` | `repo::tests::startup_and_mutations::successful_prefetch_settles_omitted_and_oversized_entries` |
| `repo::tests::completed_read_remains_recent_for_focus_without_retaining_its_timer` | `repo::tests::startup_and_mutations::completed_read_remains_recent_for_focus_without_retaining_its_timer` |
| `repo::tests::startup_snapshot_cannot_authorize_merge_even_after_a_failed_load` | `repo::tests::startup_and_mutations::startup_snapshot_cannot_authorize_merge_even_after_a_failed_load` |
| `repo::tests::startup_snapshot_remotes_stay_unread_until_a_fresh_load` | `repo::tests::startup_and_mutations::startup_snapshot_remotes_stay_unread_until_a_fresh_load` |
| `repo::tests::startup_snapshot_retains_queued_links_through_failed_loading` | `repo::tests::startup_and_mutations::startup_snapshot_retains_queued_links_through_failed_loading` |

## Modified paths

```text
src/repo/actions.rs
src/repo/apply.rs
src/repo/branch_cleanup_tests.rs
src/repo/branch_health.rs
src/repo/branch_tree.rs
src/repo/celebration.rs
src/repo/cochange_owner/check_transitions.py
src/repo/cochange_owner/lifecycle_tests.rs
src/repo/cochange_owner/requests.rs
src/repo/cochange_owner/tests.rs
src/repo/cochange_owner/view.rs
src/repo/cochanges.rs
src/repo/columns.rs
src/repo/commands.rs
src/repo/conflict_check.rs
src/repo/core_seams/action_preconditions.rs
src/repo/core_seams/branch_actions.rs
src/repo/core_seams/conflict_badges.rs
src/repo/core_seams/conflict_edits.rs
src/repo/core_seams/conflict_labels.rs
src/repo/core_seams/diff_navigation.rs
src/repo/core_seams/display_actions.rs
src/repo/core_seams/file_navigation.rs
src/repo/core_seams/graph_narration.rs
src/repo/core_seams/history_projection.rs
src/repo/core_seams/job_submission.rs
src/repo/core_seams/keyboard_commands.rs
src/repo/core_seams/label_templates.rs
src/repo/core_seams/load_requests.rs
src/repo/core_seams/message_dispatch.rs
src/repo/core_seams/operation_labels.rs
src/repo/core_seams/operation_progress.rs
src/repo/core_seams/operation_state.rs
src/repo/core_seams/palette_actions.rs
src/repo/core_seams/partial_patch_edits.rs
src/repo/core_seams/preflight_dialog.rs
src/repo/core_seams/rebase_execution.rs
src/repo/core_seams/rebase_todo.rs
src/repo/core_seams/repository_shortcuts.rs
src/repo/core_seams/repository_toolbar.rs
src/repo/core_seams/working_tree_edits.rs
src/repo/direct_tests/authentication_integration.rs
src/repo/direct_tests/branch_cleanup_safety.rs
src/repo/direct_tests/branch_health_fluent_reason_tests.rs
src/repo/direct_tests/branch_health_tests.rs
src/repo/direct_tests/branch_tree_tests.rs
src/repo/direct_tests/celebration_tests.rs
src/repo/direct_tests/columns_exchange_tests.rs
src/repo/direct_tests/columns_sequential_exchange_tests.rs
src/repo/direct_tests/columns_snapshot_allocation_tests.rs
src/repo/direct_tests/columns_tests.rs
src/repo/direct_tests/commands_tests.rs
src/repo/direct_tests/conflict_check_tests.rs
src/repo/direct_tests/diagnostic_presentation.rs
src/repo/direct_tests/file_text_r2_navigation_tests.rs
src/repo/direct_tests/file_text_tests.rs
src/repo/direct_tests/forge_selection_preferences.rs
src/repo/direct_tests/graph_walk_tests.rs
src/repo/direct_tests/growth_tests.rs
src/repo/direct_tests/keyboard_menu_registry_tests.rs
src/repo/direct_tests/keys_tests.rs
src/repo/direct_tests/layout_followup_tests.rs
src/repo/direct_tests/layout_responsive_tests.rs
src/repo/direct_tests/lifecycle_tests.rs
src/repo/direct_tests/load_connection_identity_tests.rs
src/repo/direct_tests/load_tests.rs
src/repo/direct_tests/messages_fluent_presentation_tests.rs
src/repo/direct_tests/navigate_tests.rs
src/repo/direct_tests/preflight_core_guard_paint_tests.rs
src/repo/direct_tests/preflight_test_support.rs
src/repo/direct_tests/push_preflight_route_caption_tests.rs
src/repo/direct_tests/push_preflight_tests.rs
src/repo/direct_tests/quick_edit_tests.rs
src/repo/direct_tests/rebase_tests.rs
src/repo/direct_tests/request_lifetimes.rs
src/repo/direct_tests/rewrite_tests.rs
src/repo/direct_tests/startup_and_mutations.rs
src/repo/direct_tests/symbol_biography_tests.rs
src/repo/direct_tests/toolbar_area_d_tests.rs
src/repo/direct_tests/watcher_integration.rs
src/repo/file_text.rs
src/repo/forge_selection_tests.rs
src/repo/graph_walk.rs
src/repo/growth.rs
src/repo/keyboard.rs
src/repo/keys.rs
src/repo/layout.rs
src/repo/lifecycle.rs
src/repo/load.rs
src/repo/maintenance-198.md
src/repo/messages.rs
src/repo/mod.rs
src/repo/navigate.rs
src/repo/operation_plan/check_grammar.py
src/repo/operation_plan/check_plan.py
src/repo/operation_plan/completion.rs
src/repo/operation_plan/credentials.rs
src/repo/operation_plan/dispatch.rs
src/repo/operation_plan/envelope.rs
src/repo/operation_plan/executor.rs
src/repo/operation_plan/expectations.rs
src/repo/operation_plan/grammar_adapter.rs
src/repo/operation_plan/ignored_files.rs
src/repo/operation_plan/ignored_scans.rs
src/repo/operation_plan/model.rs
src/repo/operation_plan/model_pure_tests.rs
src/repo/operation_plan/preconditions.rs
src/repo/operation_plan/pull_options.rs
src/repo/operation_plan/routes.rs
src/repo/operation_plan/short_rebase.rs
src/repo/operation_plan/start.rs
src/repo/operation_plan/tests.rs
src/repo/operations.rs
src/repo/push_preflight.rs
src/repo/quick_edit.rs
src/repo/rebase.rs
src/repo/rewrite.rs
src/repo/state.rs
src/repo/symbol_biography.rs
src/repo/tab_owner/construction.rs
src/repo/tab_owner/model.rs
src/repo/tests.rs
src/repo/toolbar.rs
```
