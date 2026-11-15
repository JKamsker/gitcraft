# Windows live-tab stall coverage (#211)

Dev run 43721621876 at 014add85891638e721151222029824e8fcb30751 stalled
for 735.937 seconds in the squash receipt test. Its Job Object snapshot contained
only Rust PID 3616 (25 threads); this does not identify a blocked frame or prove
a root cause. The receipt helper opens a live tab, polls synchronously inside
30-second elapsed-time loops, performs Git/actions outside those loop checks,
and previously dropped without draining jobs before directory removal.

The seven live receipt cases now reuse #246/#247's 90-second whole-body worker
supervisor, context-local settings and pinned connection context, and live-tab
guard. They trace fixture Git, opening, rewrite dispatch/review, receipt readiness,
disk persistence, assertions, tab close/drain and removal. The tab drains before
fixture deletion, including early exits/unwinding. Exclusive temporary directories
keep the original spaces/Unicode path coverage; the interactive todo now lives
inside its fixture's .git directory and is removed with it. Existing assertions,
inner wait budgets and the two pure parser tests are unchanged. Cleanup is best
effort with the existing bounded Windows sharing-violation retries and warnings.

On timeout, unmatched `[hang-stage TEST] begin LABEL` scopes and the active chain
in `[hang-deadline TEST]` locate the pending work. Existing Windows process tracing,
owned snapshots and bounded minidump capture are unchanged. Inspect the matching
head's test-report artifacts and stacks; Linux passes do not prove the native
Windows cause is resolved. Production behavior is unchanged.

## Full family inventory

All 30 cases below have opt-in stages and a whole-body deadline. These include
historically stalled tests and adjacent live-lifecycle cases found during audit.
The #246 cases retain their original explicit drop instrumentation; #247 and the
receipt cases additionally drain jobs/watchers before best-effort fixture removal.
The Stop case is Unix-only. This inventory does not claim broader worker-drain
coverage or native Windows root-cause resolution.

### Earlier #246/#247 coverage (23)

- `repo::test_lane::tests::testing_a_branch_fills_the_lane`
- `repo::test_lane::tests::a_rebase_keeping_the_trees_keeps_the_dots`
- `repo::test_lane::tests::testing_up_to_a_commit_starts_at_the_base`
- `repo::test_lane::tests::stop_ends_the_run` (Unix-only)
- `repo::test_lane::tests::a_new_command_can_forget_the_old_results`
- `repo::test_lane::tests::a_bad_range_says_why`
- `repo::test_lane::tests::the_first_failure_is_the_oldest_whatever_finishes_first`
- `repo::test_lane::tests::a_stored_command_with_spaces_is_the_same`
- `git::testlane::tests::results_survive_a_rebase_that_keeps_the_trees`
- `repo::stack::tests::a_restack_stopped_on_conflicts_continues_or_aborts_from_the_tab`
- `repo::ghost::tests::the_key_follows_what_a_rebase_checks`
- `repo::practice::tests::a_reopened_tab_loads_and_checks_its_goal_in_the_background`
- `repo::practice::tests::an_answer_from_before_a_reload_is_discarded`
- `repo::push::tests::toolbar_push_sends_even_heuristic_findings_without_a_checklist`
- `repo::push::tests::toolbar_refusal_preserves_remote_and_offers_explicit_force_review`
- `repo::push::tests::toolbar_publish_starts_directly_with_one_remote_and_asks_with_two`
- `repo::push::tests::toolbar_never_silently_follows_a_configured_force_refspec`
- `repo::push::tests::toolbar_honors_push_remote_and_keeps_gone_upstream_target`
- `repo::push::tests::toolbar_honors_the_saved_push_branch`
- `repo::push::tests::cancelling_ordinary_push_preparation_never_sends`
- `repo::push::tests::offline_push_settles_preparation_and_drops_queued_actions`
- `repo::push::tests::direct_push_checks_submodules_without_publishing_them`
- `repo::push::tests::ordinary_push_does_not_follow_reachable_annotated_tags`

### Timeline receipt coverage (7)

- `repo::timeline::receipt_tests::a_rebase_follows_each_commit_to_its_copy_on_the_new_base`
- `repo::timeline::receipt_tests::a_squash_says_the_files_are_identical_and_a_commit_went`
- `repo::timeline::receipt_tests::a_reorder_keeps_every_change_and_the_files`
- `repo::timeline::receipt_tests::an_amend_says_the_author_and_signature_changed`
- `repo::timeline::receipt_tests::a_commit_has_no_receipt`
- `repo::timeline::receipt_tests::a_reset_back_has_no_receipt`
- `repo::timeline::receipt_tests::a_ref_on_an_old_commit_does_not_cut_the_series`

The additional bounded Windows cleanup regression is `repo::practice::tests::sharing_violation_cleanup_warns_and_can_be_retried_after_release`; it is not a historical stalled case.
