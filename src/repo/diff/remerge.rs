//! A file of what a merge resolved (git's own merge of the parents against the recorded one):
//! what git said of its conflicts on top, and each hunk labelled with what it is.

use super::parse::meta;
use super::{DiffLine, Kind};
use crate::git::{hunk_resolves, is_conflict_marker};

/// The label of a hunk that resolves a conflict.
pub(super) const RESOLVES: &str = "repo-core-1952";
/// The label of a hunk that changes what git merged without a conflict.
pub(super) const EDITED: &str = "repo-core-1953";

/// Where the diff comes from, for its header: git's own merge of the parents, then the
/// recorded one.
pub(super) fn origin(hash: &str) -> String {
    crate::i18n::message("repo-core-1954", &[("hash", format!("{}", hash.get(..8).unwrap_or(hash)).into())])
}

/// `lines` of a file's remerge diff, after what git said of its `conflicts`, each hunk header
/// saying whether it resolves a conflict or edits beyond the merge (`ignore_ws`: the diff was
/// read without whitespace changes).
pub(super) fn label(conflicts: &[String], lines: Vec<DiffLine>, ignore_ws: bool) -> Vec<DiffLine> {
    let mut out: Vec<DiffLine> = conflicts.iter().map(|c| meta(c)).collect();
    if lines.is_empty() && !out.is_empty() {
        out.push(meta(if ignore_ws {
            crate::i18n::text("repo-core-1955")
        } else {
            crate::i18n::text("repo-core-1956")
        }));
    }
    let hunks: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].kind == Kind::Hunk)
        .collect();
    let marked: Vec<bool> = hunks
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = hunks.get(n + 1).copied().unwrap_or(lines.len());
            lines[start + 1..end]
                .iter()
                .any(|l| l.kind == Kind::Del && is_conflict_marker(&l.text))
        })
        .collect();
    let any_marked = marked.contains(&true);
    let mut lines = lines;
    for (&start, &marked) in hunks.iter().zip(&marked) {
        let what = if hunk_resolves(conflicts, marked, any_marked) {
            crate::i18n::text(RESOLVES)
        } else {
            crate::i18n::text(EDITED)
        };
        lines[start].text = format!("{}   · {what}", lines[start].text);
    }
    out.extend(lines);
    out
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    const RESOLVED_AND_EDITED: &str = "\
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
@@ -20,1 +16,2 @@ fn tail
 z
+extra
";

    fn content() -> Vec<String> {
        vec!["CONFLICT (content): Merge conflict in f.txt".to_owned()]
    }

    #[test]
    fn hunks_say_what_they_are() {
        let lines = label(&content(), parse(RESOLVED_AND_EDITED), false);
        assert_eq!(lines[0].kind, Kind::Meta);
        assert_eq!(lines[0].text, "CONFLICT (content): Merge conflict in f.txt");
        let hunks: Vec<&str> = lines
            .iter()
            .filter(|l| l.kind == Kind::Hunk)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(hunks.len(), 2);
        assert!(hunks[0].starts_with("@@ -1,7 +1,3 @@") && hunks[0].ends_with(crate::i18n::text(RESOLVES)));
        // Away from the markers, in the same file: an edit beyond the merge.
        assert!(hunks[1].contains("fn tail") && hunks[1].ends_with(crate::i18n::text(EDITED)));
        // The line numbers still count from the hunk's own header.
        let resolved = lines.iter().find(|l| l.text == "RESOLVED");
        assert_eq!(resolved.and_then(|l| l.new), Some(2));
    }

    #[test]
    fn says_which_merge_it_is() {
        assert_eq!(origin("0123456789abcdef"), "git's merge → merge 01234567");
        assert_eq!(origin("abc"), "git's merge → merge abc");
    }

    #[test]
    fn a_file_kept_as_git_left_it_says_so() {
        let conflicts = vec!["CONFLICT (modify/delete): f deleted in x".to_owned()];
        // A conflict over several paths git printed nothing for with the path alone.
        let lines = label(&conflicts, parse(""), false);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.kind == Kind::Meta));
        assert!(lines[1].text.contains("no line differs"));
        let lines = label(&conflicts, parse(""), true);
        assert!(lines[1].text.contains("whitespace"));
        // Nothing at all: nothing to say.
        assert!(label(&[], parse(""), false).is_empty());
    }

    #[test]
    fn a_conflict_not_of_lines_is_resolved_by_any_hunk() {
        let conflicts =
            vec!["CONFLICT (modify/delete): f deleted in a and modified in b".to_owned()];
        let text = "diff --git a/f b/f\n--- a/f\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
        let lines = label(&conflicts, parse(text), false);
        assert!(lines[1].text.ends_with(crate::i18n::text(RESOLVES)), "{:?}", lines[1]);
        // A content conflict whose markers git couldn't write (a link): resolved there too.
        let text = "diff --git a/l b/l\n--- a/l\n+++ b/l\n@@ -1 +1 @@\n-one\n+two\n";
        let lines = label(&content(), parse(text), false);
        assert!(lines[1].text.ends_with(crate::i18n::text(RESOLVES)), "{:?}", lines[1]);
    }

    #[test]
    fn markers_in_a_file_without_conflicts_are_text() {
        // A setext heading's underline, or an added marker: an edit, not a resolution.
        let text = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n T\n-=======\n+-----\n";
        let lines = label(&[], parse(text), false);
        assert!(lines[0].text.ends_with(crate::i18n::text(EDITED)), "{:?}", lines[0]);
        let text = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1,2 @@\n x\n+=======\n";
        let lines = label(&content(), parse(text), false);
        assert!(
            lines[1].text.ends_with(crate::i18n::text(RESOLVES)),
            "no hunk marked: {:?}",
            lines[1]
        );
    }
}
