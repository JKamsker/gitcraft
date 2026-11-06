//! A file of what a merge resolved (git's own merge of the parents against the recorded one):
//! what git said of its conflicts on top, and each hunk labelled with what it is.

use super::parse::meta;
use super::{DiffLine, Kind};
use crate::git::is_conflict_marker;

/// The label of a hunk that takes away conflict markers git left.
pub(super) const RESOLVES: &str = "resolves a conflict";
/// The label of a hunk that changes what git merged without a conflict.
pub(super) const EDITED: &str = "edited beyond the merge";

/// Where the diff comes from, for its header.
pub(super) fn origin(hash: &str) -> String {
    format!("Resolved in merge {}", hash.get(..8).unwrap_or(hash))
}

/// `lines` of the remerge diff `text`, after what git said of the file's conflicts, each hunk
/// header saying whether it resolves a conflict or edits beyond the merge.
pub(super) fn label(text: &str, lines: Vec<DiffLine>) -> Vec<DiffLine> {
    let mut out: Vec<DiffLine> = messages(text).map(meta).collect();
    if lines.is_empty() && !out.is_empty() {
        out.push(meta(
            "The recorded merge keeps this file as git's own merge leaves it: no line differs.",
        ));
    }
    let hunks: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].kind == Kind::Hunk)
        .collect();
    let mut lines = lines;
    for (n, &start) in hunks.iter().enumerate() {
        let end = hunks.get(n + 1).copied().unwrap_or(lines.len());
        let resolves = lines[start + 1..end]
            .iter()
            .any(|l| l.kind == Kind::Del && is_conflict_marker(&l.text));
        let what = if resolves { RESOLVES } else { EDITED };
        lines[start].text = format!("{}   · {what}", lines[start].text);
    }
    out.extend(lines);
    out
}

/// What git said of the conflicts in the file (`CONFLICT (content): …`), before its first hunk.
fn messages(text: &str) -> impl Iterator<Item = &str> {
    text.split_terminator('\n')
        .take_while(|l| !l.starts_with("@@"))
        .filter_map(|l| l.strip_prefix("remerge "))
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

    #[test]
    fn hunks_say_what_they_are() {
        let lines = label(RESOLVED_AND_EDITED, parse(RESOLVED_AND_EDITED));
        assert_eq!(lines[0].kind, Kind::Meta);
        assert_eq!(lines[0].text, "CONFLICT (content): Merge conflict in f.txt");
        let hunks: Vec<&str> = lines
            .iter()
            .filter(|l| l.kind == Kind::Hunk)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(hunks.len(), 2);
        assert!(hunks[0].starts_with("@@ -1,7 +1,3 @@") && hunks[0].ends_with(RESOLVES));
        assert!(hunks[1].contains("fn tail") && hunks[1].ends_with(EDITED));
        // The line numbers still count from the hunk's own header.
        let resolved = lines.iter().find(|l| l.text == "RESOLVED");
        assert_eq!(resolved.and_then(|l| l.new), Some(2));
    }

    #[test]
    fn says_which_merge_it_is() {
        assert_eq!(origin("0123456789abcdef"), "Resolved in merge 01234567");
        assert_eq!(origin("abc"), "Resolved in merge abc");
    }

    #[test]
    fn a_file_kept_as_git_left_it_says_so() {
        let text = "diff --git a/f b/f\nremerge CONFLICT (modify/delete): f deleted in x\n";
        let lines = label(text, parse(text));
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.kind == Kind::Meta));
        assert!(lines[1].text.contains("no line differs"));
        // Nothing at all: nothing to say.
        assert!(label("", parse("")).is_empty());
    }

    #[test]
    fn an_added_marker_is_not_a_resolution() {
        let text = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1,2 @@\n x\n+=======\n";
        let lines = label(text, parse(text));
        assert!(lines[0].text.ends_with(EDITED), "{:?}", lines[0]);
    }
}
