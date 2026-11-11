# Issue 198: Git subtree decomposition

Base: `a4d841713af3d31178a3b9212fd3c5adcb4172dc`. This slice owns only `src/git/**`.

## Scope and compatibility

All 57 oversized Rust files in this subtree were decomposed. The subtree
has 378 Rust files, with zero above 500 physical lines (maximum 500).
Production boundaries separate execution, result interpretation, bounded object reads,
credential behavior, registry persistence, preparation, and settlement. Complete types
and their implementations stay together. Existing public names are re-exported at their
original module paths. Serialization declarations, field names, journal formats, and
application-owned sentinel encodings are preserved.

## Shared global option grammar

`git::parse_command_envelope<S: AsRef<str>>(&[S])` returns
`Result<(usize, &str), git::GlobalOptionError>`. It accepts owned strings and borrowed
words without allocation. The index identifies the unchanged argument tail.

Errors are `MissingValue { index }`, `UnknownOption { index }`, `NoCommand`, and
`InternalMarker { index }`. Unknown options are never skipped to guess a command.
Required separate values consume the next token even if named `fetch` or `push`.
Attached empty values count as present. Arguments after the command are opaque.
Internal NUL marker envelopes must be dispatched/unwrapped by the operation owner.

The grammar deliberately matches the prior strict saved-operation envelope whitelist.
The command and hook classifiers use it; their separate execution/network and hook
policies remain distinct. Invalid envelopes yield an empty command tail/no hook command,
leaving Git to diagnose ordinary invalid invocations. This parser does not expand aliases,
authorize network access, validate saved replay, or interpret internal marker payloads.

The operations owner must adopt this API in `reaches_a_remote` and `command_envelope`,
preserve approved-push/internal-step handling, and map errors to existing saved-operation
messages. Those callers are outside this slice's ownership.

## Validation

- `rustfmt --edition 2024 --check src/git/mod.rs`: recursive formatting/module parsing.
- `rustc --edition 2024 --test src/git/global_options.rs -o /tmp/gitcraft-global-options-tests`
  followed by the binary: four pure parser tests passed.
- An isolated Rust harness containing the actual `after_options`, `goes_over_network`,
  `hooks::subcommand`, and `hooks::wants_trace` bodies, the shared parser, and
  `command/global_option_tests.rs`: six tests passed, including the four parser tests.
  This checks classifier behavior without compiling the application or starting Git.
- Tree-sitter parsed every subtree Rust source without syntax errors. A whole-subtree
  function-body token inventory retained all 3,264 original functions. The only deliberate
  body changes are `after_options` and `subcommand`; other differences are formatting,
  relative module qualification, or module-internal visibility adjustments.
- Static test discovery retained all 906 existing test functions and added six. Current
  `ci/test-shards.py` has no exact-name isolated entries under `git::`; both Git shard
  prefix assignments remain unchanged. Full compiled test discovery remains an integration gate.
- Physical-line inventory covers all subtree Rust files, including newly added files.
- `git diff --check` passed.

No Cargo build, application test suite, platform/process test, CI action, push, or PR was
performed. Integration must run full builds/tests, compiled Windows shard discovery,
Linux/Windows and feature-configuration checks, and independent review on the combined
candidate. No whole-issue completion or new persistence/process behavior is claimed.

## Test-name changes

Externalizing inline test modules preserved their qualified names. These 93 tests
needed responsibility submodules because their existing test files themselves exceeded
500 lines. Every original test function remains present; updated qualified names follow.

| Previous qualified name | Current qualified name |
| --- | --- |
| `git::activity::tests::month_math_and_recorded_timezone_boundaries_are_validated` | `git::activity::tests::records::month_math_and_recorded_timezone_boundaries_are_validated` |
| `git::activity::tests::nul_records_keep_non_utf8_paths_and_reject_incomplete_metadata` | `git::activity::tests::records::nul_records_keep_non_utf8_paths_and_reject_incomplete_metadata` |
| `git::activity::tests::a_mailmap_edit_between_real_git_pages_cannot_split_an_author` | `git::activity::tests::mailmap::a_mailmap_edit_between_real_git_pages_cannot_split_an_author` |
| `git::activity::tests::mailmap_blob_and_configured_file_precedence_match_git` | `git::activity::tests::mailmap::mailmap_blob_and_configured_file_precedence_match_git` |
| `git::activity::tests::bare_mailmap_defaults_and_absent_optional_maps_match_git` | `git::activity::tests::mailmap::bare_mailmap_defaults_and_absent_optional_maps_match_git` |
| `git::activity::tests::unavailable_promised_mailmap_blob_is_an_error_without_downloading` | `git::activity::tests::mailmap::unavailable_promised_mailmap_blob_is_an_error_without_downloading` |
| `git::activity::tests::configured_missing_mailmap_file_and_successful_git_errors_are_reported` | `git::activity::tests::mailmap::configured_missing_mailmap_file_and_successful_git_errors_are_reported` |
| `git::activity::tests::configured_submodule_display_filters_never_hide_committed_gitlink_paths` | `git::activity::tests::records::configured_submodule_display_filters_never_hide_committed_gitlink_paths` |
| `git::backend::answer::tests::porcelain_status_matches_git` | `git::backend::answer::tests::status::porcelain_status_matches_git` |
| `git::backend::answer::tests::porcelain_status_modes_and_fallbacks` | `git::backend::answer::tests::status::porcelain_status_modes_and_fallbacks` |
| `git::backend::answer::tests::config_lookups_are_gits` | `git::backend::answer::tests::config::config_lookups_are_gits` |
| `git::bundles::tests::old_git_verification_is_fail_closed_for_promisor_or_unreadable_configuration` | `git::bundles::tests::verification::old_git_verification_is_fail_closed_for_promisor_or_unreadable_configuration` |
| `git::bundles::tests::verification_streams_only_the_validated_header_and_keeps_the_old_git_fallback` | `git::bundles::tests::verification::verification_streams_only_the_validated_header_and_keeps_the_old_git_fallback` |
| `git::bundles::tests::preview_never_unpacks_even_a_corrupt_pack` | `git::bundles::tests::format::preview_never_unpacks_even_a_corrupt_pack` |
| `git::bundles::tests::local_ref_validation_matches_native_git_without_remote_round_trips` | `git::bundles::tests::format::local_ref_validation_matches_native_git_without_remote_round_trips` |
| `git::bundles::tests::export_ref_snapshots_and_saved_files_cannot_be_overwritten` | `git::bundles::tests::format::export_ref_snapshots_and_saved_files_cannot_be_overwritten` |
| `git::bundles::tests::malformed_headers_and_untrusted_refs_are_rejected_without_unpacking` | `git::bundles::tests::format::malformed_headers_and_untrusted_refs_are_rejected_without_unpacking` |
| `git::bundles::tests::unicode_whitespace_refs_and_legacy_prerequisite_subjects_are_preserved` | `git::bundles::tests::format::unicode_whitespace_refs_and_legacy_prerequisite_subjects_are_preserved` |
| `git::bundles::tests::partial_clone_export_never_fetches_promised_blobs` | `git::bundles::tests::format::partial_clone_export_never_fetches_promised_blobs` |
| `git::errors::tests::dubious_ownership_in_every_wording` | `git::errors::tests::messages::dubious_ownership_in_every_wording` |
| `git::errors::tests::dubious_ownership_never_trusts_everything` | `git::errors::tests::messages::dubious_ownership_never_trusts_everything` |
| `git::errors::tests::locks_of_the_index_and_of_refs` | `git::errors::tests::messages::locks_of_the_index_and_of_refs` |
| `git::errors::tests::line_endings_warned_and_refused` | `git::errors::tests::messages::line_endings_warned_and_refused` |
| `git::errors::tests::line_endings_give_way_to_the_real_error` | `git::errors::tests::messages::line_endings_give_way_to_the_real_error` |
| `git::errors::tests::many_line_ending_warnings_are_kept_short` | `git::errors::tests::messages::many_line_ending_warnings_are_kept_short` |
| `git::errors::tests::names_too_long_here_and_on_windows` | `git::errors::tests::messages::names_too_long_here_and_on_windows` |
| `git::errors::tests::certificate_problems_of_every_tls_library` | `git::errors::tests::messages::certificate_problems_of_every_tls_library` |
| `git::errors::tests::broken_transfers` | `git::errors::tests::messages::broken_transfers` |
| `git::errors::tests::histories_shallow_pushes_and_ssh_keys` | `git::errors::tests::messages::histories_shallow_pushes_and_ssh_keys` |
| `git::errors::tests::others_are_not_explained` | `git::errors::tests::messages::others_are_not_explained` |
| `git::errors::tests::value_patterns_match_only_the_value` | `git::errors::tests::messages::value_patterns_match_only_the_value` |
| `git::external_tool::tests::real::the_diff_tool_is_shown_each_kind_of_change` | `git::external_tool::tests::real::diff::the_diff_tool_is_shown_each_kind_of_change` |
| `git::external_tool::tests::real::a_renamed_file_is_shown_against_what_it_was` | `git::external_tool::tests::real::diff::a_renamed_file_is_shown_against_what_it_was` |
| `git::external_tool::tests::real::a_diff_tool_that_fails_or_has_nothing_to_show_says_so` | `git::external_tool::tests::real::diff::a_diff_tool_that_fails_or_has_nothing_to_show_says_so` |
| `git::external_tool::tests::real::a_preset_runs_its_program_with_the_files` | `git::external_tool::tests::real::diff::a_preset_runs_its_program_with_the_files` |
| `git::file_comparison::tests::migration_reads_renames_copies_scores_and_immutable_blob_sides` | `git::file_comparison::tests::migration::migration_reads_renames_copies_scores_and_immutable_blob_sides` |
| `git::file_comparison::tests::unborn_additions_deletions_and_empty_reorganizations_are_explicit` | `git::file_comparison::tests::migration::unborn_additions_deletions_and_empty_reorganizations_are_explicit` |
| `git::file_comparison::tests::matrix_distinguishes_equal_blobs_missing_modes_and_pins_ambiguous_branch_refs` | `git::file_comparison::tests::migration::matrix_distinguishes_equal_blobs_missing_modes_and_pins_ambiguous_branch_refs` |
| `git::file_comparison::tests::link_targets_and_binary_versions_are_read_without_following_links` | `git::file_comparison::tests::paths::link_targets_and_binary_versions_are_read_without_following_links` |
| `git::file_comparison::tests::raw_non_utf8_paths_stay_distinct_and_literal_folders_do_not_expand_globs` | `git::file_comparison::tests::paths::raw_non_utf8_paths_stay_distinct_and_literal_folders_do_not_expand_globs` |
| `git::recovery::tests::content_search_finds_reflog_commits_and_staged_only_blobs_without_any_writes` | `git::recovery::tests::search::content_search_finds_reflog_commits_and_staged_only_blobs_without_any_writes` |
| `git::recovery::tests::dropped_stashes_retain_working_index_and_untracked_content_provenance` | `git::recovery::tests::search::dropped_stashes_retain_working_index_and_untracked_content_provenance` |
| `git::recovery::tests::binary_large_unicode_literal_and_cancelled_scans_have_explicit_limits` | `git::recovery::tests::search::binary_large_unicode_literal_and_cancelled_scans_have_explicit_limits` |
| `git::recovery::tests::commit_and_excerpt_parsers_refuse_invalid_identity_and_keep_unicode_ranges` | `git::recovery::tests::search::commit_and_excerpt_parsers_refuse_invalid_identity_and_keep_unicode_ranges` |
| `git::remerge::tests::git_versions` | `git::remerge::tests::parsing::git_versions` |
| `git::remerge::tests::conflict_markers` | `git::remerge::tests::parsing::conflict_markers` |
| `git::remerge::tests::which_merges_can_be_redone` | `git::remerge::tests::parsing::which_merges_can_be_redone` |
| `git::remerge::tests::quoted_names` | `git::remerge::tests::parsing::quoted_names` |
| `git::remerge::tests::reads_the_files_of_a_remerge_diff` | `git::remerge::tests::parsing::reads_the_files_of_a_remerge_diff` |
| `git::remerge::tests::reads_renames_and_empty_output` | `git::remerge::tests::parsing::reads_renames_and_empty_output` |
| `git::remerge::tests::reads_a_huge_remerge_diff` | `git::remerge::tests::parsing::reads_a_huge_remerge_diff` |
| `git::search_query::tests::text_without_operators_is_searched_as_before` | `git::search_query::tests::grammar::text_without_operators_is_searched_as_before` |
| `git::search_query::tests::every_operator_is_read` | `git::search_query::tests::grammar::every_operator_is_read` |
| `git::search_query::tests::operator_names_ignore_letter_case` | `git::search_query::tests::grammar::operator_names_ignore_letter_case` |
| `git::search_query::tests::free_text_goes_with_the_operators` | `git::search_query::tests::grammar::free_text_goes_with_the_operators` |
| `git::search_query::tests::quoted_values_keep_their_spaces_and_escapes` | `git::search_query::tests::grammar::quoted_values_keep_their_spaces_and_escapes` |
| `git::search_query::tests::unicode_is_kept_as_typed` | `git::search_query::tests::grammar::unicode_is_kept_as_typed` |
| `git::search_query::tests::repeated_operators_add_up` | `git::search_query::tests::grammar::repeated_operators_add_up` |
| `git::search_query::tests::relative_dates_go_to_git` | `git::search_query::tests::grammar::relative_dates_go_to_git` |
| `git::search_query::tests::mistakes_are_explained_where_they_are` | `git::search_query::tests::grammar::mistakes_are_explained_where_they_are` |
| `git::search_query::tests::huge_queries_with_operators_are_refused_plain_ones_are_not` | `git::search_query::tests::grammar::huge_queries_with_operators_are_refused_plain_ones_are_not` |
| `git::search_query::tests::the_piece_at_the_cursor` | `git::search_query::tests::grammar::the_piece_at_the_cursor` |
| `git::search_query::tests::values_are_quoted_when_they_need_it_and_read_back_the_same` | `git::search_query::tests::grammar::values_are_quoted_when_they_need_it_and_read_back_the_same` |
| `git::search_query::tests::each_operator_is_one_git_option` | `git::search_query::tests::grammar::each_operator_is_one_git_option` |
| `git::search_query::tests::free_text_looks_in_messages_and_authors_or_in_code` | `git::search_query::tests::grammar::free_text_looks_in_messages_and_authors_or_in_code` |
| `git::search_query::tests::values_that_look_like_options_stay_values` | `git::search_query::tests::grammar::values_that_look_like_options_stay_values` |
| `git::simulate::tests::versions_are_read_whatever_the_build` | `git::simulate::tests::parsing::versions_are_read_whatever_the_build` |
| `git::simulate::tests::what_each_version_can_check` | `git::simulate::tests::parsing::what_each_version_can_check` |
| `git::simulate::tests::a_clean_answer_is_only_the_tree` | `git::simulate::tests::parsing::a_clean_answer_is_only_the_tree` |
| `git::simulate::tests::conflicts_are_read_with_their_kinds` | `git::simulate::tests::parsing::conflicts_are_read_with_their_kinds` |
| `git::simulate::tests::a_message_about_two_paths_marks_the_conflicted_one` | `git::simulate::tests::parsing::a_message_about_two_paths_marks_the_conflicted_one` |
| `git::simulate::tests::a_garbled_answer_is_an_error` | `git::simulate::tests::parsing::a_garbled_answer_is_an_error` |
| `git::simulate::tests::picks_are_read_from_log_records` | `git::simulate::tests::parsing::picks_are_read_from_log_records` |
| `git::simulate::tests::a_pull_rebase_starts_from_the_fork_point` | `git::simulate::tests::pull::a_pull_rebase_starts_from_the_fork_point` |
| `git::simulate::tests::a_pull_rebase_skips_work_upstream_already_took` | `git::simulate::tests::pull::a_pull_rebase_skips_work_upstream_already_took` |
| `git::drafts::tests::private_index_preserves_racy_content_checks_across_timestamp_boundaries` | `git::drafts::tests::index_safety::private_index_preserves_racy_content_checks_across_timestamp_boundaries` |
| `git::drafts::tests::configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery` | `git::drafts::tests::index_safety::configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery` |
| `git::drafts::tests::tree_roots_survive_gc_restart_and_do_not_change_history_branches_reflog_or_normal_push` | `git::drafts::tests::retention::tree_roots_survive_gc_restart_and_do_not_change_history_branches_reflog_or_normal_push` |
| `git::drafts::tests::retention_expires_owned_refs_with_cas_and_preserves_unknown_versions` | `git::drafts::tests::retention::retention_expires_owned_refs_with_cas_and_preserves_unknown_versions` |
| `git::drafts::tests::changed_symlinks_and_conflicted_indexes_fail_without_following_targets_or_touching_index` | `git::drafts::tests::path_safety::changed_symlinks_and_conflicted_indexes_fail_without_following_targets_or_touching_index` |
| `git::drafts::tests::limits_invalid_paths_and_cancellation_leave_all_real_state_untouched` | `git::drafts::tests::path_safety::limits_invalid_paths_and_cancellation_leave_all_real_state_untouched` |
| `git::drafts::tests::private_index_cache_handles_split_and_sparse_indexes_without_rewriting_the_real_index` | `git::drafts::tests::path_safety::private_index_cache_handles_split_and_sparse_indexes_without_rewriting_the_real_index` |
| `git::drafts::tests::recovery_detects_intent_to_add_changes_even_when_index_object_records_match` | `git::drafts::tests::path_safety::recovery_detects_intent_to_add_changes_even_when_index_object_records_match` |
| `git::drafts::tests::unchanged_large_tracked_files_are_not_copied_as_working_changes` | `git::drafts::tests::path_safety::unchanged_large_tracked_files_are_not_copied_as_working_changes` |
| `git::drafts::tests::typed_remote_reader_quotes_paths_and_never_confuses_links_with_missing_files` | `git::drafts::tests::path_safety::typed_remote_reader_quotes_paths_and_never_confuses_links_with_missing_files` |
| `git::drafts::tests::nested_untracked_repositories_are_excluded_without_hiding_outer_changes` | `git::drafts::tests::path_safety::nested_untracked_repositories_are_excluded_without_hiding_outer_changes` |
| `git::drafts::tests::missing_paths_below_symlinks_fail_closed_and_do_not_publish_deletions` | `git::drafts::tests::path_safety::missing_paths_below_symlinks_fail_closed_and_do_not_publish_deletions` |
| `git::drafts::tests::unborn_sha256_and_linked_worktree_versions_keep_separate_identities` | `git::drafts::tests::path_safety::unborn_sha256_and_linked_worktree_versions_keep_separate_identities` |
| `git::drafts::tests::rename_unicode_no_newline_and_hardlinks_recover_only_the_selected_path` | `git::drafts::tests::path_safety::rename_unicode_no_newline_and_hardlinks_recover_only_the_selected_path` |
| `git::drafts::tests::ignored_rule_read_warnings_fail_closed_before_any_version_is_published` | `git::drafts::tests::path_safety::ignored_rule_read_warnings_fail_closed_before_any_version_is_published` |
| `git::drafts::tests::one_turn_reversal_retains_later_hunks_and_index_and_rejects_stale_overlap` | `git::drafts::tests::turns::one_turn_reversal_retains_later_hunks_and_index_and_rejects_stale_overlap` |
| `git::drafts::tests::review_marks_persist_unchanged_hunks_but_changed_hunks_reappear` | `git::drafts::tests::turns::review_marks_persist_unchanged_hunks_but_changed_hunks_reappear` |
| `git::drafts::tests::file_exchange_rolls_back_owned_changes_when_guard_fails_mid_batch` | `git::drafts::tests::turns::file_exchange_rolls_back_owned_changes_when_guard_fails_mid_batch` |

## Modified paths

```text
src/git/activity/tests.rs
src/git/activity/tests/mailmap.rs
src/git/activity/tests/records.rs
src/git/atlas.rs
src/git/atlas/tests.rs
src/git/backend.rs
src/git/backend/answer.rs
src/git/backend/answer/config.rs
src/git/backend/answer/metadata.rs
src/git/backend/answer/refs.rs
src/git/backend/answer/revision.rs
src/git/backend/answer_tests.rs
src/git/backend/answer_tests/config.rs
src/git/backend/answer_tests/status.rs
src/git/backend/executable_context_tests.rs
src/git/backend/native_disabled.rs
src/git/backend/native_enabled.rs
src/git/backend/patch.rs
src/git/backend/patch/render.rs
src/git/backend/tests.rs
src/git/backend_fluent_adapter_tests.rs
src/git/backend_fluent_identity_tests.rs
src/git/backend_relative_time_tests.rs
src/git/bundles.rs
src/git/bundles/header.rs
src/git/bundles/recording_import_tests.rs
src/git/bundles/storage.rs
src/git/bundles/tests.rs
src/git/bundles/tests/format.rs
src/git/bundles/tests/verification.rs
src/git/cleanup.rs
src/git/cleanup/residuals.rs
src/git/cleanup/runtime_held_tests.rs
src/git/cleanup/tests.rs
src/git/clone_options.rs
src/git/clone_options/runtime_held_tests.rs
src/git/clone_options/tests.rs
src/git/command.rs
src/git/command/backend_fluent_identity_tests.rs
src/git/command/credential_policy.rs
src/git/command/credential_policy/tests.rs
src/git/command/credentials.rs
src/git/command/download_policy_recording_tests.rs
src/git/command/global_option_tests.rs
src/git/command/internals_merge_recording.rs
src/git/command/local_execution.rs
src/git/command/object_reads.rs
src/git/command/output.rs
src/git/command/practice_environment_tests.rs
src/git/command/r2_inert_tests.rs
src/git/command/remote_execution.rs
src/git/command/repository.rs
src/git/command/repository_endpoint_recording.rs
src/git/command/result.rs
src/git/command/reviewed_push.rs
src/git/command/reviewed_push_tests.rs
src/git/command/shell.rs
src/git/command/source.rs
src/git/command/source/configuration.rs
src/git/command/source/tests.rs
src/git/command/sparse_account_recording_tests.rs
src/git/command/tests.rs
src/git/command/watched.rs
src/git/command/watched/tests.rs
src/git/command/workspace.rs
src/git/commits.rs
src/git/commits/detail.rs
src/git/commits/search.rs
src/git/commits/tests.rs
src/git/diff.rs
src/git/diff/patch_export.rs
src/git/diff/tests.rs
src/git/doctor/config.rs
src/git/doctor/config/tests.rs
src/git/doctor/health.rs
src/git/doctor/health/backend_fluent_suggestion_identity_tests.rs
src/git/doctor/health/tests.rs
src/git/doctor/hooks.rs
src/git/doctor/hooks/repair.rs
src/git/doctor/hooks/scan.rs
src/git/doctor/hooks/tests.rs
src/git/doctor/ignore.rs
src/git/doctor/ignore/attributes.rs
src/git/doctor/ignore/tests.rs
src/git/drafts/capture.rs
src/git/drafts/deferred.rs
src/git/drafts/deferred/backend_fluent_notice_tests.rs
src/git/drafts/deferred/journal.rs
src/git/drafts/deferred/ownership.rs
src/git/drafts/deferred/settlement.rs
src/git/drafts/deferred/tests.rs
src/git/drafts/files.rs
src/git/drafts/index.rs
src/git/drafts/lists/commit.rs
src/git/drafts/lists/mod.rs
src/git/drafts/lists/publication.rs
src/git/drafts/lists/shelve.rs
src/git/drafts/mod.rs
src/git/drafts/native_files/mod.rs
src/git/drafts/native_files/saved_policy_tests.rs
src/git/drafts/native_files/tests.rs
src/git/drafts/restore.rs
src/git/drafts/restore/patch.rs
src/git/drafts/tests.rs
src/git/drafts/tests/index_safety.rs
src/git/drafts/tests/path_safety.rs
src/git/drafts/tests/retention.rs
src/git/drafts/tests/turns.rs
src/git/drafts/turn.rs
src/git/drafts/turn/aggregate_tests.rs
src/git/drafts/turn/identity_tests.rs
src/git/drafts/turn/inverse.rs
src/git/drafts/turn/review_state.rs
src/git/drafts/virtuals.rs
src/git/drafts/virtuals/commit.rs
src/git/drafts/virtuals/editing.rs
src/git/drafts/virtuals/snapshot.rs
src/git/errors/tests.rs
src/git/errors/tests/messages.rs
src/git/external_tool/tests.rs
src/git/external_tool/tests/real.rs
src/git/external_tool/tests/real/diff.rs
src/git/file_comparison/index_transaction.rs
src/git/file_comparison/index_transaction/tests.rs
src/git/file_comparison/tests.rs
src/git/file_comparison/tests/migration.rs
src/git/file_comparison/tests/paths.rs
src/git/file_comparison/transaction.rs
src/git/file_comparison/transaction/correction_tests.rs
src/git/file_comparison/transaction/owned_remote_correction_tests.rs
src/git/files.rs
src/git/files/tests.rs
src/git/global_options.rs
src/git/global_options/tests.rs
src/git/hooks.rs
src/git/hooks/backend_fluent_identity_tests.rs
src/git/hooks/tests.rs
src/git/investigations/dossier.rs
src/git/investigations/dossier/history.rs
src/git/investigations/dossier/tests.rs
src/git/investigations/ignore.rs
src/git/investigations/ignore/native.rs
src/git/investigations/symbols.rs
src/git/investigations/symbols/backend_fluent_rename_tests.rs
src/git/investigations/symbols/history.rs
src/git/line_log.rs
src/git/line_log/tests.rs
src/git/machines.rs
src/git/machines/tests.rs
src/git/maintenance-198.md
src/git/mod.rs
src/git/overview.rs
src/git/overview/transport.rs
src/git/preview.rs
src/git/preview/overwritten_files.rs
src/git/preview/tests.rs
src/git/prompt_session/checkpoint.rs
src/git/prompt_session/mod.rs
src/git/recovery/file.rs
src/git/recovery/file/local.rs
src/git/recovery/file/root_replacement_tests.rs
src/git/recovery/file/tests.rs
src/git/recovery/file/windows_ancestor_tests.rs
src/git/recovery/mod.rs
src/git/recovery/objects.rs
src/git/recovery/reader.rs
src/git/recovery/tests.rs
src/git/recovery/tests/search.rs
src/git/remerge/tests.rs
src/git/remerge/tests/parsing.rs
src/git/repository_tools/dependencies.rs
src/git/repository_tools/dependencies/advertisement.rs
src/git/repository_tools/dependencies/cleanup_recording.rs
src/git/repository_tools/dependencies/parent_continuation_recording.rs
src/git/repository_tools/dependencies/tests.rs
src/git/repository_tools/outbox.rs
src/git/repository_tools/outbox/consent_tests.rs
src/git/repository_tools/outbox/delivery.rs
src/git/repository_tools/outbox/tests.rs
src/git/repository_tools/routes.rs
src/git/repository_tools/routes/tests.rs
src/git/repository_tools/subtrees.rs
src/git/repository_tools/subtrees/preparation.rs
src/git/repository_tools/subtrees/publication.rs
src/git/repository_tools/subtrees/registry.rs
src/git/repository_tools/subtrees/settlement.rs
src/git/search_query/tests.rs
src/git/search_query/tests/grammar.rs
src/git/simulate/merge_tree.rs
src/git/simulate/mod.rs
src/git/simulate/replay.rs
src/git/simulate/tests.rs
src/git/simulate/tests/parsing.rs
src/git/simulate/tests/pull.rs
src/git/stack/restack.rs
src/git/stack/restack/execution.rs
src/git/unity.rs
src/git/unity/metadata.rs
```
