use super::*;
use crate::repo::checkout_blocked::Blockers;
use crate::repo::{Undo, ops};

fn blocked(steps: &[&[&str]], blockers: Blockers) -> BlockedCheckout {
    BlockedCheckout {
        label: "Checkout develop".to_owned(),
        ops: ops(steps),
        undo: Undo::None,
        blockers,
    }
}

fn saved(path: &str, blob: &str, mode: &str) -> Saved {
    Saved {
        path: path.to_owned(),
        blob: blob.to_owned(),
        mode: mode.to_owned(),
    }
}

#[test]
fn the_target_is_the_first_name_that_is_no_option_nor_a_new_branch() {
    let t = |args: &[&str]| target(&ops(&[args])[0]).map(str::to_owned);
    assert_eq!(
        t(&["checkout", "--no-overwrite-ignore", "dev", "--"]).as_deref(),
        Some("dev")
    );
    assert_eq!(
        t(&["checkout", "-b", "feat", "--track", "origin/feat", "--"]).as_deref(),
        Some("origin/feat")
    );
    assert_eq!(t(&["checkout", "--", "dev"]), None);
}

#[test]
fn saved_files_are_written_back_with_their_mode_right_after_the_checkout() {
    let b = blocked(
        &[
            &["checkout", "--no-overwrite-ignore", "develop", "--"],
            &["pull", "--ff-only"],
        ],
        Blockers {
            changed: vec!["a.rs".into()],
            ignored: vec!["gen.json".into(), "run*.sh".into()],
            ..Default::default()
        },
    );
    let carried = Carried {
        saved: vec![
            saved("gen.json", "b1", "100644"),
            saved("run*.sh", "b2", "100755"),
        ],
        untrack: vec![],
    };
    assert_eq!(
        b.carry(&carried),
        ops(&[
            &["checkout", "--merge", "develop", "--"],
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "100644,b1,gen.json",
                "--cacheinfo",
                "100755,b2,run*.sh"
            ],
            &["checkout-index", "-f", "--", "gen.json", "run*.sh"],
            &[
                "reset",
                "-q",
                "--",
                ":(literal)gen.json",
                ":(literal)run*.sh"
            ],
            &["pull", "--ff-only"],
        ])
    );
}

#[test]
fn only_ignored_files_are_carried_by_a_plain_checkout() {
    let b = blocked(
        &[&["checkout", "--no-overwrite-ignore", "develop", "--"]],
        Blockers {
            ignored: vec!["gen.json".into()],
            ..Default::default()
        },
    );
    let carried = b.carry(&Carried {
        saved: vec![saved("gen.json", "b", "100644")],
        untrack: vec![],
    });
    assert_eq!(carried[0], ["checkout", "develop", "--"]);
    assert_eq!(carried.len(), 4);
}

#[test]
fn a_changed_file_the_target_lacks_leaves_the_index_and_is_not_merged() {
    // Back from a branch that tracks a file this one ignores: it stays, ignored again.
    let b = blocked(
        &[&["checkout", "--no-overwrite-ignore", "main", "--"]],
        Blockers {
            changed: vec!["gen.json".into()],
            ..Default::default()
        },
    );
    let carried = Carried {
        saved: vec![],
        untrack: vec!["gen.json".into()],
    };
    assert_eq!(
        b.carry(&carried),
        ops(&[
            &["rm", "--cached", "-q", "--", ":(literal)gen.json"],
            &["checkout", "main", "--"],
        ])
    );
}
