//! Real filesystem failures must travel through the walker and scheduler without pruning.
use super::model::Outcome;
use super::*;
use std::os::unix::fs::PermissionsExt;

use crate::app::library::test_support::AssertValue;
fn scan(state: &mut LibraryState, id: u64) {
    state.rescan(1, &Source::Root(id));
    let end = Instant::now() + Duration::from_secs(10);
    while state.scheduler.busy() || state.scheduler.threads() > 0 {
        state
            .scheduler
            .poll(1, state.saved.get_mut(&1).assert_value(), now_secs());
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn inaccessible_git_metadata_keeps_cached_repositories_stale() {
    for case in [
        "git-directory",
        "gitfile",
        "gitfile-target",
        "head-io-error",
    ] {
        let base = tempfile::tempdir().assert_value();
        let root = base.path().join("root");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).assert_value();
        let gitdir = if case.starts_with("gitfile") {
            base.path().join("gitdir")
        } else {
            repo.join(".git")
        };
        std::fs::create_dir(&gitdir).assert_value();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/main\n").assert_value();
        if case.starts_with("gitfile") {
            std::fs::write(repo.join(".git"), format!("gitdir: {}\n", gitdir.display()))
                .assert_value();
        }
        let mut state = LibraryState::default();
        let id = state.lib(1).add_root(root, 1);
        scan(&mut state, id);
        assert_eq!(state.get(1).assert_value().entries.len(), 1, "{case}");
        let locked = if case == "gitfile" {
            repo.join(".git")
        } else {
            gitdir.clone()
        };
        if case == "head-io-error" {
            std::fs::remove_file(gitdir.join("HEAD")).assert_value();
            std::os::unix::fs::symlink("HEAD", gitdir.join("HEAD")).assert_value();
        } else {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
                .assert_value();
        }
        let refused = if case == "gitfile" {
            std::fs::File::open(&locked).is_err()
        } else {
            std::fs::metadata(gitdir.join("HEAD")).is_err()
        };
        scan(&mut state, id);
        if case != "head-io-error" {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))
                .assert_value();
        }
        // Privileged runners may bypass permissions; the HEAD I/O error always applies.
        if !refused {
            continue;
        }
        let lib = state.get(1).assert_value();
        assert_eq!(lib.entries.len(), 1, "{case}: cached row pruned");
        assert!(lib.entries[0].stale(), "{case}");
        assert_eq!(
            lib.root(id)
                .assert_value()
                .last
                .as_ref()
                .assert_value()
                .outcome,
            Outcome::Partial(super::super::repo_scan::Stopped::Unreadable),
            "{case}"
        );
    }
}
