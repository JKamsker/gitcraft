//! Regressions from the repository-library review (#225).
#[cfg(unix)]
use super::checks::{Checks, Guards};
use super::model::{Outcome, ScanRecord};
use super::*;

use crate::app::library::test_support::AssertValue;
fn settle(state: &mut LibraryState, profile: u64) {
    let end = Instant::now() + Duration::from_secs(10);
    while state.checks.busy() {
        state.answered(profile);
        assert!(Instant::now() < end, "filesystem check timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn partial_nearby_results_are_not_retried_on_every_open() {
    let mut state = LibraryState::default();
    let parent = PathBuf::from("/work/projects");
    state.lib(1).note_nearby(parent.clone());
    state.lib(1).nearby[0].last = Some(ScanRecord {
        at: now_secs(),
        found: 0,
        outcome: Outcome::Partial(super::super::repo_scan::Stopped::Folders),
    });
    state.look_nearby(1, parent);
    assert!(!state.scheduler.busy());
}

#[test]
fn root_moves_reject_duplicates_and_save_as_root_preserves_depth() {
    let mut state = LibraryState::default();
    state.add_root(1, PathBuf::from("/work/a"), 1);
    state.add_root(1, PathBuf::from("/work/b"), 1);
    let id = state.get(1).assert_value().roots[1].id;
    state.move_root(1, id, PathBuf::from("/work/a/"));
    assert_eq!(
        state.get(1).assert_value().root(id).assert_value().path,
        Path::new("/work/b")
    );
    state.add_root(1, PathBuf::from("/work/a"), 4);
    assert_eq!(state.get(1).assert_value().roots[0].depth, 4);
}

#[test]
fn failed_open_drops_missing_auto_entry_but_keeps_existing_and_other_profile() {
    let base = tempfile::tempdir().assert_value();
    let missing = base.path().join("removed");
    let existing = base.path().join("still-here");
    std::fs::create_dir(&existing).assert_value();
    let mut state = LibraryState::default();
    for profile in [1, 2] {
        let id = state.lib(profile).add_root(base.path().to_owned(), 1);
        state
            .lib(profile)
            .add_found(&Source::Root(id), &[missing.clone(), existing.clone()]);
    }
    state.open_failed(1, &missing);
    state.open_failed(1, &existing);
    settle(&mut state, 1);
    assert_eq!(state.get(1).assert_value().entries.len(), 1);
    assert_eq!(state.get(1).assert_value().entries[0].path, existing);
    assert_eq!(state.get(2).assert_value().entries.len(), 2);
}

#[test]
fn temporary_repositories_never_queue_nearby() {
    let base = tempfile::tempdir().assert_value();
    let root = base.path().join("practice/repository");
    std::fs::create_dir_all(&root).assert_value();
    let mut state = LibraryState::default();
    state.noticed_open(1, &std::fs::canonicalize(root).assert_value());
    settle(&mut state, 1);
    assert!(state.get(1).is_none_or(|l| l.nearby.is_empty()));
    assert!(!state.scheduler.busy());
}

#[cfg(unix)]
#[test]
fn canonical_home_and_temp_aliases_are_both_guarded() {
    let base = tempfile::tempdir().assert_value();
    let home = base.path().join("real-home");
    let temp = base.path().join("real-temp");
    std::fs::create_dir_all(&home).assert_value();
    std::fs::create_dir_all(&temp).assert_value();
    let home_alias = base.path().join("home");
    let temp_alias = base.path().join("temp");
    std::os::unix::fs::symlink(&home, &home_alias).assert_value();
    std::os::unix::fs::symlink(&temp, &temp_alias).assert_value();
    let guards = Guards::canonical(vec![home_alias.clone()], vec![temp_alias.clone()]);
    for repo in [
        home.join("repo"),
        home_alias.join("repo"),
        temp.join("practice/repo"),
        temp_alias.join("practice/repo"),
    ] {
        assert_eq!(checks::vet(&repo, &guards), None, "{repo:?}");
    }
    // A real device boundary is rejected even outside the named mount locations.
    #[cfg(target_os = "linux")]
    assert!(checks::is_mount(Path::new("/proc")));
    let mut checks = Checks::default();
    checks.set_guards(guards);
    checks.nearby(1, home.join("repo"));
    let end = Instant::now() + Duration::from_secs(10);
    while checks.busy() {
        for answer in checks.poll() {
            assert!(matches!(
                answer,
                checks::Answer::Nearby { parent: None, .. }
            ));
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn profile_switch_discards_pending_checks_even_after_switching_back() {
    let base = tempfile::tempdir().assert_value();
    let path = base.path().join("removed");
    let mut state = LibraryState::default();
    let id = state.lib(1).add_root(base.path().to_owned(), 1);
    state
        .lib(1)
        .add_found(&Source::Root(id), std::slice::from_ref(&path));
    state.open_failed(1, &path);
    state.switch_profile();
    state.switch_profile();
    settle(&mut state, 1);
    assert_eq!(state.get(1).assert_value().entries.len(), 1);
}

#[test]
fn disabling_nearby_retires_checks_before_it_is_enabled_again() {
    let base = tempfile::tempdir().assert_value();
    let parent = base.path().join("projects");
    std::fs::create_dir(&parent).assert_value();
    let mut state = LibraryState::default();
    state.checks.set_guards(checks::Guards::default());
    state.noticed_open(1, &parent.join("repo"));
    state.set_nearby(1, false);
    state.set_nearby(1, true);
    settle(&mut state, 1);
    assert!(state.get(1).assert_value().nearby.is_empty());
}

#[test]
fn delayed_check_cannot_undo_forget_or_cancel_another_folder() {
    let base = tempfile::tempdir().assert_value();
    let forgotten = base.path().join("forgotten");
    let kept = base.path().join("kept");
    for parent in [&forgotten, &kept] {
        std::fs::create_dir(parent).assert_value();
    }
    let mut state = LibraryState::default();
    state.checks.set_guards(checks::Guards::default());
    state.lib(1).note_nearby(forgotten.clone());
    state.noticed_open(1, &forgotten.join("repo"));
    state.noticed_open(1, &kept.join("repo"));
    state.forget_nearby(1, &forgotten);
    // Apply both answers only after Forget, regardless of worker scheduling.
    settle(&mut state, 1);
    assert_eq!(state.get(1).assert_value().nearby.len(), 1);
    assert_eq!(state.get(1).assert_value().nearby[0].parent, kept);
}
