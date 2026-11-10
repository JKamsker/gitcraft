//! Parsing.

use super::*;

#[test]
pub(super) fn versions_are_read_whatever_the_build() {
    for (out, version) in [
        ("git version 2.43.0\n", Some((2, 43, 0))),
        ("git version 2.45.1.windows.1", Some((2, 45, 1))),
        ("git version 2.39.3 (Apple Git-145)", Some((2, 39, 3))),
        ("git version 2.50.0-rc1", Some((2, 50, 0))),
        ("git version 3.0", Some((3, 0, 0))),
        ("not git", None),
        ("git version x.y", None),
    ] {
        assert_eq!(parse_version(out), version, "{out}");
    }
}

#[test]
pub(super) fn what_each_version_can_check() {
    let old = Features::of((2, 37, 9));
    assert!(!old.merge && !old.replay && !old.trees);
    assert_eq!(
        old.merge_unavailable().as_deref(),
        Some("Conflict check needs git 2.38 or newer (this is 2.37.9)")
    );
    let merges_only = Features::of((2, 39, 0));
    assert!(merges_only.merge && !merges_only.replay);
    assert!(merges_only.merge_unavailable().is_none());
    assert!(merges_only.replay_unavailable().is_some());
    let commits = Features::of((2, 44, 0));
    assert!(commits.replay && !commits.trees);
    assert!(Features::of((2, 45, 0)).trees);
    assert!(Features::of((3, 0, 0)).trees);
}

#[test]
pub(super) fn a_clean_answer_is_only_the_tree() -> Result<(), String> {
    let tree = "ec4eff38302efdf54b2a168fa3698fb13371e19c";
    let merged = parse_merge_tree(format!("{tree}\0").as_bytes(), true)?;
    assert_eq!(merged.tree, tree);
    assert!(merged.conflicts.is_empty());
    Ok(())
}

/// As git 2.43 prints it for a binary file, a modify/delete and an add/add of a name with a
/// space and an `ü`.
#[test]
pub(super) fn conflicts_are_read_with_their_kinds() -> Result<(), String> {
    let out = "1e85aa1cf91bcb17f0d6f1233c1fce7b7868c946\0bin.dat\0del.txt\0sp ace ü.txt\0\0\
        1\0bin.dat\0CONFLICT (binary)\0warning: Cannot merge binary files: bin.dat\n\0\
        1\0bin.dat\0Auto-merging\0Auto-merging bin.dat\n\0\
        1\0bin.dat\0CONFLICT (contents)\0CONFLICT (content): Merge conflict in bin.dat\n\0\
        1\0del.txt\0CONFLICT (modify/delete)\0CONFLICT (modify/delete): del.txt deleted\n\0\
        1\0sp ace ü.txt\0CONFLICT (add/add)\0CONFLICT (add/add): Merge conflict\n\0\
        1\0sp ace ü.txt\0CONFLICT (contents)\0CONFLICT (content): Merge conflict\n\0";
    let merged = parse_merge_tree(out.as_bytes(), false)?;
    let kinds: Vec<(&str, Vec<&str>)> = merged
        .conflicts
        .iter()
        .map(|f| {
            (
                f.path.as_str(),
                f.kinds.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("bin.dat", vec!["binary", "contents"]),
            ("del.txt", vec!["modify/delete"]),
            ("sp ace ü.txt", vec!["add/add", "contents"]),
        ]
    );
    let text_only: Vec<bool> = merged.conflicts.iter().map(|f| f.text_only()).collect();
    assert_eq!(text_only, [false, false, true]);
    Ok(())
}

/// A rename/delete message names both paths; only the conflicted one gets the kind.
#[test]
pub(super) fn a_message_about_two_paths_marks_the_conflicted_one() -> Result<(), String> {
    let out = "aaaa\0new.txt\0\0\
        2\0old.txt\0new.txt\0CONFLICT (rename/delete)\0old.txt renamed to new.txt\n\0";
    let merged = parse_merge_tree(out.as_bytes(), false)?;
    assert_eq!(merged.conflicts.len(), 1);
    assert_eq!(merged.conflicts[0].kinds, ["rename/delete"]);
    // Without messages (`--no-messages`, a git that prints none) the files are still there.
    let bare = parse_merge_tree(b"aaaa\0x.txt\0\0", false)?;
    assert_eq!(bare.conflicts[0].path, "x.txt");
    assert!(!bare.conflicts[0].text_only());
    Ok(())
}

#[test]
pub(super) fn a_garbled_answer_is_an_error() {
    assert!(parse_merge_tree(b"", true).is_err());
    assert!(parse_merge_tree(b"fatal: no\0", true).is_err());
    assert!(parse_merge_tree(b"abcd\0\0", false).is_err());
}

#[test]
pub(super) fn picks_are_read_from_log_records() {
    let out = "c1\0p1 p2\0first\0c2\0\0root commit\0\n";
    let picks = parse_picks(out);
    assert_eq!(
        picks,
        [
            Pick {
                hash: "c1".into(),
                parent: Some("p1".into()),
                summary: "first".into(),
            },
            Pick {
                hash: "c2".into(),
                parent: None,
                summary: "root commit".into(),
            },
        ]
    );
}
