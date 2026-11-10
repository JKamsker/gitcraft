//! Parsing.

use super::*;

#[test]
pub(super) fn git_versions() {
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
pub(super) fn conflict_markers() {
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
pub(super) fn which_merges_can_be_redone() {
    assert!(remerge_unavailable(2).is_none());
    assert!(remerge_unavailable(1).is_some());
    let octopus = remerge_unavailable(3).unwrap_or_default();
    assert!(
        octopus.contains("octopus") && octopus.contains('3'),
        "{octopus}"
    );
}

#[test]
pub(super) fn quoted_names() {
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

pub(super) const RESOLVED_AND_EVIL: &str = "\
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
pub(super) fn reads_the_files_of_a_remerge_diff() {
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
pub(super) fn reads_renames_and_empty_output() {
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
pub(super) fn reads_a_huge_remerge_diff() {
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
