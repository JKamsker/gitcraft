//! Reading remerge diffs, and real merges redone by the git installed.

use std::path::{Path, PathBuf};

use super::parse::{header_paths, names, unquote};
use super::*;

#[test]
fn git_versions() {
    assert_eq!(parse_git_version("git version 2.43.0"), Some((2, 43)));
    assert_eq!(
        parse_git_version("git version 2.39.3 (Apple Git-145)\n"),
        Some((2, 39))
    );
    assert_eq!(
        parse_git_version("git version 2.41.0.windows.1"),
        Some((2, 41))
    );
    assert_eq!(parse_git_version("git version 3.0"), Some((3, 0)));
    assert_eq!(parse_git_version("hg version 2.43"), None);
    assert_eq!(parse_git_version(""), None);
    assert!(supports_remerge("git version 2.36.0").is_ok());
    assert!(supports_remerge("git version 3.1.0").is_ok());
    let old = supports_remerge("git version 2.35.8")
        .err()
        .unwrap_or_default();
    assert!(old.contains("2.36") && old.contains("2.35"), "{old}");
    assert!(supports_remerge("nonsense").is_err());
}

#[test]
fn conflict_markers() {
    for marker in [
        "<<<<<<< HEAD",
        "<<<<<<<",
        "=======",
        ">>>>>>> abc (side)",
        "||||||| base",
        // `conflict-marker-size=10`.
        "<<<<<<<<<< ecfb012 (m)",
        "==========",
    ] {
        assert!(is_conflict_marker(marker), "{marker}");
    }
    assert!(is_conflict_marker("=======\r"));
    for not in [
        "<<<<<<",
        "======",
        "======= x",
        "== =",
        "",
        "<<<<<<<x",
        " =======",
    ] {
        assert!(!is_conflict_marker(not), "{not}");
    }
}

#[test]
fn which_merges_can_be_redone() {
    assert!(remerge_unavailable(2).is_none());
    assert!(remerge_unavailable(1).is_some());
    let octopus = remerge_unavailable(3).unwrap_or_default();
    assert!(
        octopus.contains("octopus") && octopus.contains('3'),
        "{octopus}"
    );
}

#[test]
fn quoted_names() {
    assert_eq!(unquote("plain name"), "plain name");
    assert_eq!(unquote(r#""q\"x.txt""#), "q\"x.txt");
    assert_eq!(unquote(r#""\303\274n\303\257.txt""#), "ünï.txt");
    assert_eq!(unquote(r#""tab\there\\back""#), "tab\there\\back");
    // Not closed: left as it is.
    assert_eq!(unquote(r#""open"#), r#""open"#);
    assert_eq!(
        header_paths("a/sp ace.txt b/sp ace.txt"),
        Some(("sp ace.txt".to_owned(), "sp ace.txt".to_owned()))
    );
    assert_eq!(
        header_paths(r#""a/q\"t" "b/q\"t""#),
        Some(("q\"t".to_owned(), "q\"t".to_owned()))
    );
    // A rename's two names can't be split on a space; its `rename` lines say them.
    assert_eq!(header_paths("a/x y b/z"), None);
    assert_eq!(header_paths(""), None);
}

const RESOLVED_AND_EVIL: &str = "\
diff --git a/f.txt b/f.txt
remerge CONFLICT (content): Merge conflict in f.txt
index a2ad40a..8bcb16a 100644
--- a/f.txt
+++ b/f.txt
@@ -1,7 +1,3 @@
 a
-<<<<<<< 3332621 (main)
-MAIN
-=======
-SIDE
->>>>>>> ee5c64a (side)
+RESOLVED
 c
diff --git a/sp ace.txt b/sp ace.txt
index 587be6b..b35fb43 100644
--- a/sp ace.txt\t
+++ b/sp ace.txt\t
@@ -1 +1,2 @@
 x
+--- not a header
diff --git a/gone b/gone
remerge CONFLICT (modify/delete): gone deleted in a and modified in b.  Version b of gone left in tree.
diff --git \"a/\\303\\274.txt\" \"b/\\303\\274.txt\"
deleted file mode 100644
index 587be6b..0000000
--- \"a/\\303\\274.txt\"
+++ /dev/null
@@ -1 +0,0 @@
-x
";

#[test]
fn reads_the_files_of_a_remerge_diff() {
    let remerge = parse_remerge(RESOLVED_AND_EVIL);
    assert_eq!(remerge.files.len(), 4);
    let f = &remerge.files[0];
    assert_eq!((f.path.as_str(), f.status), ("f.txt", 'M'));
    assert_eq!((f.added, f.removed), (1, 5));
    assert!(f.conflicted() && !f.edited());
    assert_eq!((f.resolving_hunks, f.edited_hunks), (1, 0));
    assert_eq!(f.conflicts, ["CONFLICT (content): Merge conflict in f.txt"]);
    // Edited beyond the merge: a line that looks like a header is still a line.
    let evil = &remerge.files[1];
    assert_eq!(evil.path, "sp ace.txt");
    assert_eq!((evil.added, evil.removed), (1, 0));
    assert!(!evil.conflicted());
    // A conflict resolved with the file as git left it: no lines, only the message.
    let kept = &remerge.files[2];
    assert_eq!(kept.path, "gone");
    assert!(kept.conflicted() && !kept.edited());
    assert_eq!((kept.resolving_hunks, kept.edited_hunks), (0, 0));
    assert_eq!((kept.added, kept.removed), (0, 0));
    let deleted = &remerge.files[3];
    assert_eq!((deleted.path.as_str(), deleted.status), ("ü.txt", 'D'));
    assert_eq!(deleted.old_path, None);
    assert_eq!(remerge.conflicted().count(), 2);
    assert_eq!(remerge.edited().count(), 2);
    assert!(!remerge.is_clean());
}

#[test]
fn reads_renames_and_empty_output() {
    assert!(parse_remerge("").is_clean());
    assert!(parse_remerge("\n\nwarning: nothing\n").is_clean());
    let renamed = parse_remerge(
        "diff --git a/old name b/new name\nsimilarity index 90%\nrename from old name\nrename to new name\n",
    );
    let f = &renamed.files[0];
    assert_eq!((f.status, f.path.as_str()), ('R', "new name"));
    assert_eq!(f.old_path.as_deref(), Some("old name"));
}

#[test]
fn reads_a_huge_remerge_diff() {
    let mut text = String::new();
    for n in 0..2_000 {
        text.push_str(&format!(
            "diff --git a/f{n} b/f{n}\n--- a/f{n}\n+++ b/f{n}\n@@ -1 +1 @@\n"
        ));
        text.push_str(&"-old\n".repeat(50));
        text.push_str(&"+new\n".repeat(50));
    }
    let remerge = parse_remerge(&text);
    assert_eq!(remerge.files.len(), 2_000);
    assert!(
        remerge
            .files
            .iter()
            .all(|f| (f.added, f.removed) == (50, 50))
    );
    assert_eq!(remerge.files[1999].path, "f1999");
}

/// A repository in a fresh folder named after `name`, with a commit of `files`.
pub(crate) fn repo_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-remerge-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args);
    }
    commit(&dir, files, "base");
    dir
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let out = run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default()
}

/// Writes `files` and commits them all.
pub(crate) fn commit(dir: &Path, files: &[(&str, &str)], message: &str) {
    for (path, text) in files {
        let written = std::fs::write(dir.join(path), text);
        assert!(written.is_ok(), "writing {path}: {written:?}");
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}

/// Merges `branch` into the checked-out branch, conflicts or not (left to be resolved).
pub(crate) fn merge(dir: &Path, branch: &str) {
    let _ = run(Some(dir), &["merge", "-q", "--no-ff", "--no-edit", branch]);
}

pub(crate) fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// Both branches change the second line of `path` differently from `base`.
pub(crate) fn conflicting_branches(dir: &Path, path: &str) {
    git(dir, &["checkout", "-q", "-b", "side"]);
    commit(dir, &[(path, "a\nSIDE\nc\n")], "side");
    git(dir, &["checkout", "-q", "main"]);
    commit(dir, &[(path, "a\nMAIN\nc\n")], "main");
}

fn have_remerge(dir: &Path) -> bool {
    match remerge_support(dir) {
        Ok(()) => true,
        Err(e) => {
            // An old git on the machine running the tests: what the user would be told.
            assert!(e.contains("2.36"), "{e}");
            false
        }
    }
}

#[test]
fn a_resolved_conflict_and_an_extra_edit() {
    let names = ["ünï côdé.txt", "plain.txt"];
    let dir = repo_with("resolved", &[(names[0], "a\nb\nc\n"), (names[1], "x\n")]);
    if !have_remerge(&dir) {
        return;
    }
    conflicting_branches(&dir, names[0]);
    merge(&dir, "side");
    commit(
        &dir,
        &[(names[0], "a\nRESOLVED\nc\n"), (names[1], "x\nevil\n")],
        "merge",
    );
    let hash = head(&dir);
    let found = remerge(&dir, &hash);
    let patch = remerge_file_diff(&dir, &hash, names[0], None, DiffOptions::default());
    let other = remerge_file_diff(&dir, &hash, names[1], None, DiffOptions::default());
    let _ = std::fs::remove_dir_all(&dir);

    let found = found.unwrap_or_default();
    assert_eq!(found.files.len(), 2, "{found:?}");
    let resolved = found.files.iter().find(|f| f.path == names[0]);
    assert!(resolved.is_some_and(|f| f.resolving_hunks == 1 && !f.conflicts.is_empty()));
    let evil = found.files.iter().find(|f| f.path == names[1]);
    assert!(evil.is_some_and(|f| !f.conflicted() && f.added == 1 && f.removed == 0));
    let patch = patch.unwrap_or_default();
    assert!(
        patch.contains("remerge CONFLICT") && patch.contains("+RESOLVED"),
        "{patch}"
    );
    assert!(!patch.contains("evil"), "{patch}");
    assert!(other.unwrap_or_default().contains("+evil"));
}

#[test]
fn a_clean_merge_resolved_nothing() {
    let dir = repo_with("clean", &[("a b.txt", "a\n")]);
    if !have_remerge(&dir) {
        return;
    }
    git(&dir, &["checkout", "-q", "-b", "side"]);
    commit(&dir, &[("side.txt", "s\n")], "side");
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, &[("main.txt", "m\n")], "main");
    merge(&dir, "side");
    let found = remerge(&dir, &head(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, Ok(Remerge::default()));
}

#[test]
fn a_conflict_kept_as_git_left_it() {
    let dir = repo_with("modify-delete", &[("f", "x\n"), ("keep", "k\n")]);
    if !have_remerge(&dir) {
        return;
    }
    git(&dir, &["checkout", "-q", "-b", "side"]);
    git(&dir, &["rm", "-q", "f"]);
    git(&dir, &["commit", "-q", "-m", "delete"]);
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, &[("f", "y\n")], "modify");
    merge(&dir, "side");
    git(&dir, &["add", "f"]);
    git(&dir, &["commit", "-q", "--no-edit"]);
    let hash = head(&dir);
    let found = remerge(&dir, &hash);
    let patch = remerge_file_diff(&dir, &hash, "f", None, DiffOptions::default());
    let _ = std::fs::remove_dir_all(&dir);
    let found = found.unwrap_or_default();
    assert_eq!(found.files.len(), 1, "{found:?}");
    assert!(found.files[0].conflicts[0].contains("modify/delete"));
    assert!(patch.unwrap_or_default().contains("modify/delete"));
}

#[test]
fn an_octopus_merge_is_not_redone() {
    let dir = repo_with("octopus", &[("f", "x\n")]);
    for branch in ["b1", "b2"] {
        git(&dir, &["checkout", "-q", "-b", branch, "main"]);
        commit(&dir, &[(branch, "x\n")], branch);
    }
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["merge", "-q", "--no-ff", "--no-edit", "b1", "b2"]);
    let parents = git(&dir, &["rev-list", "--parents", "-n1", "HEAD"]);
    // git skips it, printing nothing: it would read as clean, so it's never asked.
    let skipped = remerge(&dir, &head(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    if skipped.is_ok() {
        assert_eq!(skipped, Ok(Remerge::default()));
    }
    let count = parents.split_whitespace().count() - 1;
    assert_eq!(count, 3);
    assert!(remerge_unavailable(count).is_some_and(|why| why.contains("octopus")));
}

#[test]
fn a_hash_that_is_no_commit_says_why() {
    let dir = repo_with("bad-hash", &[("f", "x\n")]);
    let err = remerge(&dir, "0123456789abcdef0123456789abcdef01234567");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.is_err_and(|e| e.starts_with("Could not redo the merge")));
}

#[test]
fn an_edit_beside_a_resolution_is_counted_as_one() {
    let text = "\
diff --git a/f b/f
remerge CONFLICT (content): Merge conflict in f
--- a/f
+++ b/f
@@ -1,5 +1,1 @@
-<<<<<<< a
-A
-=======
-B
->>>>>>> b
+AB
@@ -40 +36,2 @@
 z
+evil
";
    let remerge = parse_remerge(text);
    let f = &remerge.files[0];
    assert!(f.conflicted() && f.edited());
    assert_eq!((f.resolving_hunks, f.edited_hunks), (1, 1));
    assert_eq!(remerge.conflicted().count(), 1);
    assert_eq!(remerge.edited().count(), 1);
}

#[test]
fn a_marker_like_line_of_a_file_without_conflicts_is_an_edit() {
    // A setext heading's underline changed by a clean merge's extra edit.
    let text = "diff --git a/README.rst b/README.rst\n--- a/README.rst\n+++ b/README.rst\n\
                @@ -1,2 +1,2 @@\n Title\n-=======\n+-----\n";
    let remerge = parse_remerge(text);
    assert_eq!(remerge.conflicted().count(), 0);
    assert_eq!(remerge.files[0].edited_hunks, 1);
}

#[test]
fn files_a_conflict_over_several_paths_names_are_its_resolution() {
    let text = "\
diff --git a/rr.txt b/rr.txt
remerge CONFLICT (rename/rename): rr.txt renamed to rr_main.txt in HEAD and to rr_side.txt in side.
diff --git a/rr_side.txt b/rr_side.txt
deleted file mode 100644
--- a/rr_side.txt
+++ /dev/null
@@ -1 +0,0 @@
-x
diff --git a/rr.txt.bak b/rr.txt.bak
--- a/rr.txt.bak
+++ b/rr.txt.bak
@@ -1 +1 @@
-a
+b
";
    let remerge = parse_remerge(text);
    let side = &remerge.files[1];
    assert!(side.conflicted() && !side.edited(), "{side:?}");
    assert_eq!(side.resolving_hunks, 1);
    // Only a whole name counts: `rr.txt` isn't `rr.txt.bak`.
    assert!(!remerge.files[2].conflicted());
    assert!(!names("x rr.txt.bak y", "rr.txt"));
    assert!(names("in rr.txt.", "rr.txt"));
}

#[test]
fn a_type_change_is_one_file() {
    let text = "\
diff --git a/link b/link
remerge CONFLICT (distinct types): link had different types on each side.
deleted file mode 120000
--- a/link
+++ /dev/null
@@ -1 +0,0 @@
-target
diff --git a/link b/link
new file mode 100644
--- /dev/null
+++ b/link
@@ -0,0 +1 @@
+text
";
    let remerge = parse_remerge(text);
    assert_eq!(remerge.files.len(), 1);
    let f = &remerge.files[0];
    assert_eq!((f.status, f.added, f.removed), ('T', 1, 1));
    assert_eq!(f.resolving_hunks, 2);
}

/// Merges `side` into `main`, both having changed `path` from `a\nb\nc\n`, resolved as
/// `resolved`; the merge's hash.
fn resolved_merge(dir: &Path, path: &str, resolved: &str) -> String {
    conflicting_branches(dir, path);
    merge(dir, "side");
    commit(dir, &[(path, resolved)], "merge");
    head(dir)
}

#[test]
fn a_bigger_marker_size_is_still_a_resolution() {
    let dir = repo_with("marker-size", &[("f.txt", "a\nb\nc\n")]);
    if !have_remerge(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    commit(
        &dir,
        &[(".gitattributes", "f.txt conflict-marker-size=10\n")],
        "attrs",
    );
    let hash = resolved_merge(&dir, "f.txt", "a\nR\nc\n");
    let found = remerge(&dir, &hash);
    let _ = std::fs::remove_dir_all(&dir);
    let found = found.unwrap_or_default();
    let f = &found.files[0];
    assert_eq!((f.resolving_hunks, f.edited_hunks), (1, 0), "{f:?}");
}

#[test]
fn a_modify_delete_conflict_resolved_by_deleting() {
    let dir = repo_with("deleted", &[("f", "x\n"), ("keep", "k\n")]);
    if !have_remerge(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    git(&dir, &["checkout", "-q", "-b", "side"]);
    git(&dir, &["rm", "-q", "f"]);
    git(&dir, &["commit", "-q", "-m", "delete"]);
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, &[("f", "y\n")], "modify");
    merge(&dir, "side");
    git(&dir, &["rm", "-q", "f"]);
    git(&dir, &["commit", "-q", "--no-edit"]);
    let found = remerge(&dir, &head(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let found = found.unwrap_or_default();
    assert_eq!(found.files.len(), 1, "{found:?}");
    let f = &found.files[0];
    assert_eq!(f.status, 'D');
    assert!(f.conflicted() && !f.edited(), "{f:?}");
}

#[test]
fn a_rename_rename_conflict_resolved_by_keeping_one_name() {
    let dir = repo_with("rename-rename", &[("rr.txt", "one\ntwo\nthree\n")]);
    if !have_remerge(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    git(&dir, &["checkout", "-q", "-b", "side"]);
    git(&dir, &["mv", "rr.txt", "rr_side.txt"]);
    git(&dir, &["commit", "-q", "-m", "side"]);
    git(&dir, &["checkout", "-q", "main"]);
    git(&dir, &["mv", "rr.txt", "rr_main.txt"]);
    git(&dir, &["commit", "-q", "-m", "main"]);
    merge(&dir, "side");
    let _ = run(Some(&dir), &["rm", "-q", "--cached", "rr.txt"]);
    let _ = run(Some(&dir), &["rm", "-q", "-f", "rr_side.txt"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "--no-edit"]);
    let found = remerge(&dir, &head(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let found = found.unwrap_or_default();
    assert!(!found.is_clean(), "{found:?}");
    assert_eq!(found.edited().count(), 0, "{found:?}");
}

#[test]
fn a_clean_merge_with_a_heading_edited_into_it() {
    let dir = repo_with("setext", &[("README.rst", "Title\n=======\n\nText\n")]);
    if !have_remerge(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    git(&dir, &["checkout", "-q", "-b", "side"]);
    commit(&dir, &[("side.txt", "s\n")], "side");
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, &[("main.txt", "m\n")], "main");
    merge(&dir, "side");
    let _ = std::fs::write(dir.join("README.rst"), "Title\n-----\n\nText\n");
    git(&dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let found = remerge(&dir, &head(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let found = found.unwrap_or_default();
    assert_eq!(found.conflicted().count(), 0, "{found:?}");
    assert_eq!(found.edited().count(), 1, "{found:?}");
}

#[test]
fn the_whole_file_and_merge_drivers() {
    let dir = repo_with("whole", &[("f.txt", "a\nb\nc\nd\ne\nf\ng\nh\ni\n")]);
    if !have_remerge(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    assert!(!has_merge_drivers(&dir));
    let with = |second: &str| format!("a\n{second}\nc\nd\ne\nf\ng\nh\ni\n");
    git(&dir, &["checkout", "-q", "-b", "side"]);
    commit(&dir, &[("f.txt", &with("SIDE"))], "side");
    git(&dir, &["checkout", "-q", "main"]);
    commit(&dir, &[("f.txt", &with("MAIN"))], "main");
    merge(&dir, "side");
    commit(&dir, &[("f.txt", &with("R"))], "merge");
    let hash = head(&dir);
    let whole = DiffOptions {
        whole_file: true,
        ..Default::default()
    };
    let patch = remerge_file_diff(&dir, &hash, "f.txt", None, whole);
    git(&dir, &["config", "merge.mine.driver", "true"]);
    let drivers = has_merge_drivers(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let patch = patch.unwrap_or_default();
    // The last line, far from the change, is there as context.
    assert!(patch.contains("+R\n") && patch.ends_with(" i\n"), "{patch}");
    assert!(drivers);
}

#[test]
fn errors_say_what_to_do() {
    let dir = repo_with("read-only", &[("f", "x\n")]);
    let read_only = explain(&dir, "fatal: unable to create temporary object directory");
    let short = explain(&dir, "fatal: bad object 0123");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(read_only.contains("read-only"), "{read_only}");
    assert!(short.contains("fetching more history"), "{short}");
}
