use super::*;
use crate::git::{Area, StatusFile, WorkStatus};
use crate::repo::ops;

const BOTH: &str = "error: Your local changes to the following files would be overwritten by checkout:\n\
    \tsrc/a.rs\n\
    \t\"sp ace.txt\"\n\
    Please commit your changes or stash them before you switch branches.\n\
    error: The following untracked working tree files would be overwritten by checkout:\n\
    \tsrc/site/data.json\n\
    \tnew/file.txt\n\
    Please move or remove them before you switch branches.\n\
    Aborting\n";

fn untracked(path: &str) -> StatusFile {
    StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: '?',
        area: Area::Untracked,
    }
}

fn blocked(blockers: Blockers) -> BlockedCheckout {
    BlockedCheckout {
        label: "Checkout develop".to_owned(),
        ops: ops(&[&["checkout", "--no-overwrite-ignore", "develop", "--"]]),
        undo: Undo::None,
        blockers,
    }
}

#[test]
fn both_lists_are_read_and_anything_else_is_not_a_blocked_checkout() -> Result<(), String> {
    let (changed, listed) = parse(BOTH).ok_or("not read as a blocked checkout")?;
    assert_eq!(changed, ["src/a.rs", "sp ace.txt"]);
    assert_eq!(listed, ["src/site/data.json", "new/file.txt"]);
    let only = "error: The following untracked working tree files would be removed by \
                checkout:\n\tgone.txt\nPlease move or remove them before you switch \
                branches.\nAborting";
    assert_eq!(parse(only), Some((vec![], vec!["gone.txt".to_owned()])));
    assert_eq!(
        parse("error: pathspec 'x' did not match any file(s) known to git"),
        None
    );
    Ok(())
}

#[test]
fn untracked_files_status_does_not_show_are_the_ignored_ones() -> Result<(), String> {
    let work = WorkStatus {
        unstaged: vec![untracked("new/")],
        ..Default::default()
    };
    let (changed, listed) = parse(BOTH).ok_or("not read as a blocked checkout")?;
    let b = Blockers::sort(changed, listed, &work);
    assert_eq!(b.untracked, ["new/file.txt"]);
    assert_eq!(b.ignored, ["src/site/data.json"]);
    Ok(())
}

#[test]
fn only_ignored_files_in_the_way_are_carried_or_overwritten_by_plain_checkout() {
    let b = blocked(Blockers {
        ignored: vec!["data.json".into()],
        ..Default::default()
    });
    assert_eq!(
        b.blockers.choices(),
        [Choice::Carry, Choice::OverwriteIgnored]
    );
    assert_eq!(
        b.retry(Choice::OverwriteIgnored),
        ops(&[&["checkout", "develop", "--"]])
    );
}

#[test]
fn changes_can_come_along_be_stashed_or_be_discarded() {
    let b = blocked(Blockers {
        changed: vec!["a.rs".into()],
        ..Default::default()
    });
    assert_eq!(
        b.blockers.choices(),
        [Choice::Carry, Choice::Stash, Choice::Force]
    );
    // Nothing saved: changes the target has are merged along.
    assert_eq!(
        b.retry(Choice::Carry),
        ops(&[&["checkout", "--merge", "develop", "--"]])
    );
    assert_eq!(
        b.retry(Choice::Stash),
        ops(&[
            &[
                "stash",
                "push",
                "--include-untracked",
                "-m",
                "Before Checkout develop"
            ],
            &["checkout", "--no-overwrite-ignore", "develop", "--"],
        ])
    );
    assert_eq!(
        b.retry(Choice::Force),
        ops(&[&["checkout", "-f", "develop", "--"]])
    );
}

#[test]
fn ignored_files_are_replaced_by_every_retry_but_carrying() {
    let b = blocked(Blockers {
        changed: vec!["a.rs".into()],
        untracked: vec!["new.txt".into()],
        ignored: vec!["data.json".into()],
    });
    assert_eq!(
        b.blockers.choices(),
        [Choice::Carry, Choice::Stash, Choice::Force]
    );
    assert_eq!(b.retry(Choice::Stash)[1], ["checkout", "develop", "--"]);
}

#[test]
fn the_checkout_step_is_changed_wherever_it_is_and_the_rest_kept() {
    let mut b = blocked(Blockers {
        changed: vec!["a.rs".into()],
        ..Default::default()
    });
    b.ops = ops(&[
        &[
            "checkout",
            "--no-overwrite-ignore",
            "-b",
            "feat",
            "--track",
            "origin/feat",
            "--",
        ],
        &["pull", "--ff-only"],
    ]);
    let forced = b.retry(Choice::Force);
    assert_eq!(forced[0][..3], ["checkout", "-f", "-b"]);
    assert_eq!(forced[1], ["pull", "--ff-only"]);
}
