//! Real kills followed by ambiguous ownership or a damaged journal (#205, slice 3): restart
//! refuses destructive cleanup, writes nothing it does not own, keeps the archive
//! discoverable and says where it is.
use super::recording_kill::{boundary, first, fixture, live_original, tree, untouched};
use super::recording_kill_child::kill;
use super::recording_regressions::refs_clean;
use super::*;

const FOREIGN: &[u8] = b"\xffsomeone else's recovery\0";

fn archive_bytes(dir: &Path) -> Vec<Vec<u8>> {
    ["parked.json", "raw/file", "index", "MERGE_HEAD"]
        .iter()
        .map(|name| std::fs::read(dir.join(name)).unwrap())
        .collect()
}

/// Killed after the recovery ref is pinned, before the archive moves (C1-C2). Something not
/// ours then appears where the archive would move: no caller may merge into it or delete it.
#[test]
fn after_a_kill_before_the_move_a_foreign_destination_is_refused_and_both_are_kept() {
    for action in ["restore", "drop"] {
        let at = first(action, "git:update-ref refs/gitcraft/parked-recovery/");
        let (stop, _) = boundary(&at);
        let (f, parked) = fixture(action);
        let p = parked.unwrap();
        kill(action, stop, &f.0);
        let foreign = crate::agent::repo_file(&f.0, "parked-recovery/test-id/foreign").unwrap();
        std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        std::fs::write(&foreign, FOREIGN).unwrap();
        let source = folder(&f.0, &p.id).unwrap();
        let kept = archive_bytes(&source);
        let mut refusals = Vec::new();
        if action == "restore" {
            // The restore itself had finished: listing and bring back try only the move.
            refusals.push(list(&f.0).map(|_| ()));
            refusals.push(bring_back(&f.0, &p));
            live_original(&f);
        } else {
            // A drop in progress is listed as it is; only its own retry moves the archive.
            assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
        }
        refusals.push(drop_to_branch(&f.0, &p).map(|_| ()));
        for refused in refusals {
            let e = refused.unwrap_err();
            assert!(
                e.contains("collision") && e.contains("test-id"),
                "{action}: {e}"
            );
        }
        // Nothing deleted or merged: ours and theirs are both whole and discoverable.
        assert_eq!(std::fs::read(&foreign).unwrap(), FOREIGN);
        assert_eq!(archive_bytes(&source), kept);
        assert_eq!(discovery::locate(&f.0, &p.id).unwrap(), source);
        untouched(&f);
        // The user moves the foreign files away: recovery then completes and stays put.
        std::fs::remove_file(&foreign).unwrap();
        assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
        let branch = drop_to_branch(&f.0, &p).unwrap();
        refs_clean(&f, &p);
        let settled = tree(&f.0);
        assert_eq!(drop_to_branch(&f.0, &p).unwrap(), branch);
        assert_eq!(tree(&f.0), settled);
        untouched(&f);
    }
}

/// Killed right after the Restoring receipt, then the journal is damaged: a receipt from an
/// unknown version, or a legacy receipt torn mid-write.
#[test]
fn after_a_kill_a_corrupt_journal_keeps_the_archive_and_names_it() {
    let at = first("restore", "phase:Restoring");
    let (stop, _) = boundary(&at);
    for torn in [false, true] {
        let (f, parked) = fixture("restore");
        let p = parked.unwrap();
        kill("restore", stop, &f.0);
        let dir = folder(&f.0, &p.id).unwrap();
        let tail = dir.join("journal-v1/99999999.json");
        if torn {
            std::fs::write(&tail, br#"{"version":1,"phase":"Rest"#).unwrap();
            // The torn tail is kept as evidence and read as Uncertain; the clean worktree
            // then proves the Git step may run again, and recovery completes.
            assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
            assert!(tail.with_extension("torn").is_file());
            bring_back(&f.0, &p).unwrap();
            live_original(&f);
        } else {
            std::fs::write(&tail, br#"{"version":99,"phase":"Restored"}"#).unwrap();
            let before = tree(&f.0);
            for refused in [
                list(&f.0).map(|_| ()),
                bring_back(&f.0, &p),
                drop_to_branch(&f.0, &p).map(|_| ()),
            ] {
                let e = refused.unwrap_err();
                assert!(e.contains("version 99") && e.contains("99999999"), "{e}");
            }
            assert_eq!(tree(&f.0), before, "a refused recovery wrote files");
            // Still discoverable at a path the user is told about.
            assert_eq!(discovery::recovery_location(&f.0, &p).unwrap(), dir);
            std::fs::remove_file(&tail).unwrap();
            bring_back(&f.0, &p).unwrap();
            live_original(&f);
        }
        drop_to_branch(&f.0, &p).unwrap();
        refs_clean(&f, &p);
        untouched(&f);
    }
}
