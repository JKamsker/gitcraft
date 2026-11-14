//! Real filesystem failures must travel through the walker and scheduler without pruning.
use super::model::Outcome;
use super::*;
use std::os::unix::fs::PermissionsExt;

fn scan(state: &mut LibraryState, id: u64) {
    state.rescan(1, &Source::Root(id));
    let end = Instant::now() + Duration::from_secs(10);
    while state.scheduler.busy() || state.scheduler.threads() > 0 {
        state
            .scheduler
            .poll(1, state.saved.get_mut(&1).unwrap(), now_secs());
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
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("root");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let gitdir = if case.starts_with("gitfile") {
            base.path().join("gitdir")
        } else {
            repo.join(".git")
        };
        std::fs::create_dir(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        if case.starts_with("gitfile") {
            std::fs::write(repo.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        }
        let mut state = LibraryState::default();
        let id = state.lib(1).add_root(root, 1);
        scan(&mut state, id);
        assert_eq!(state.get(1).unwrap().entries.len(), 1, "{case}");
        let locked = if case == "gitfile" {
            repo.join(".git")
        } else {
            gitdir.clone()
        };
        if case == "head-io-error" {
            std::fs::remove_file(gitdir.join("HEAD")).unwrap();
            std::os::unix::fs::symlink("HEAD", gitdir.join("HEAD")).unwrap();
        } else {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let refused = if case == "gitfile" {
            std::fs::File::open(&locked).is_err()
        } else {
            std::fs::metadata(gitdir.join("HEAD")).is_err()
        };
        scan(&mut state, id);
        if case != "head-io-error" {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        // Privileged runners may bypass permissions; the HEAD I/O error always applies.
        if !refused {
            continue;
        }
        let lib = state.get(1).unwrap();
        assert_eq!(lib.entries.len(), 1, "{case}: cached row pruned");
        assert!(lib.entries[0].stale(), "{case}");
        assert_eq!(
            lib.root(id).unwrap().last.as_ref().unwrap().outcome,
            Outcome::Partial(super::super::repo_scan::Stopped::Unreadable),
            "{case}"
        );
    }
}
