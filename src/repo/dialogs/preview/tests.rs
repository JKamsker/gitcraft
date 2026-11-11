use super::*;

#[test]
fn commands_read_as_typed() {
    let op = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        command_line(&op(&["reset", "--hard", "abc123"])),
        "git reset --hard abc123"
    );
    assert_eq!(
        command_line(&op(&["restore", "--", "a b.txt"])),
        "git restore -- 'a b.txt'"
    );
    assert_eq!(command_line(&op(&["x", "it's", ""])), r"git x 'it'\''s' ''");
    assert_eq!(
        command_line(&op(&["branch", "-d", "ünï"])),
        "git branch -d ünï"
    );
}

#[test]
fn more_commits_say_how_many_are_only_here() {
    assert_eq!(more_commits(340, 0), "and 340 more");
    assert_eq!(more_commits(340, 340), "and 340 more, all only here");
    assert_eq!(more_commits(340, 12), "and 340 more, 12 of them only here");
}

#[test]
fn the_force_push_note_counts() {
    assert_eq!(
        pushed_note("origin/main", 1),
        "Already pushed: 1 commit this rewrites is on origin/main, so pushing it again will \
         need a force push."
    );
    assert!(pushed_note("origin/main", 3).contains("3 commits this rewrites are"));
}

#[test]
fn undo_is_promised_only_for_what_it_restores() {
    let key = crate::manual::command();
    assert_eq!(undo_text(&Undo::None, false), None);
    assert_eq!(
        undo_text(&Undo::HeadMove { mode: "--keep" }, false),
        Some(format!("{key}+Z restores this."))
    );
    assert_eq!(
        undo_text(&Undo::HeadMove { mode: "--keep" }, true),
        Some(format!(
            "{key}+Z moves the branch back, not the uncommitted changes."
        ))
    );
    let custom = Undo::Custom {
        undo: Vec::new(),
        redo: Vec::new(),
    };
    assert!(undo_text(&custom, true).is_some());
}

/// While a hard reset's preview loads, or when it couldn't be read, the files it may lose
/// are not promised back.
#[test]
fn a_hard_reset_promises_no_files_back_before_it_knows() {
    let mut preview = WhatItDoes::new(
        PreviewAsk::reset("main", "h", "t", None, true),
        String::new(),
    );
    preview.state = PreviewState::Loading(1);
    assert!(may_lose_files(&preview));
    preview.state = PreviewState::Failed("offline".to_owned());
    assert!(may_lose_files(&preview));
    preview.state = PreviewState::Ready(Preview::default());
    assert!(!may_lose_files(&preview));
    let soft = WhatItDoes::new(
        PreviewAsk::reset("main", "h", "t", None, false),
        String::new(),
    );
    assert!(!may_lose_files(&soft));
}

/// Refs moved between counting and listing: more listed only here than counted.
#[test]
fn a_count_behind_the_list_does_not_underflow() {
    let only_here = git::LeavingCommit {
        hash: "a".to_owned(),
        summary: String::new(),
        kept: Kept::OnlyHere,
    };
    let found = Preview {
        leaving: vec![only_here.clone(), only_here],
        leaving_total: 30,
        only_here_total: 1,
        ..Preview::default()
    };
    assert_eq!(more_only_here(&found), 0);
}

#[test]
fn named_files_need_no_git() {
    let files = LostFiles::Listed(vec!["b.txt".to_owned(), "a b.txt".to_owned()]);
    let found = listed_only(&files);
    assert_eq!(found.lost_files, ["a b.txt", "b.txt"]);
    assert_eq!(found.lost_total, 2);
    assert_eq!(listed_only(&LostFiles::None), Preview::default());
}
