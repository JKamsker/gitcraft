//! A commit's changes file by file, each change line of them picked on its own (or the whole
//! file when its lines can't be), and patches of some of them for `git apply`.

use std::collections::BTreeSet;

use crate::repo::diff::{self, DiffLine, Kind, Pick};

/// One file of a patch.
#[derive(Clone, Debug)]
pub(crate) struct FilePatch {
    pub path: String,
    /// Its part of the patch, as git wrote it.
    pub raw: String,
    /// Everything before its first hunk, for patches of some of its lines.
    header: String,
    pub lines: Vec<DiffLine>,
    /// Why it only goes whole, when its lines can't be picked one by one.
    pub whole: Option<&'static str>,
}

impl FilePatch {
    fn new(path: String, raw: String) -> Self {
        let lines = diff::parse(&raw);
        let header = diff::patch_header(&raw);
        let whole = why_whole(&raw, &header, &lines);
        Self {
            path,
            raw,
            header,
            lines,
            whole,
        }
    }

    /// The units it is picked by: its added and removed lines (indices into `lines`), or the
    /// file as unit `0` when it only goes whole.
    pub fn units(&self) -> Vec<usize> {
        if self.whole.is_some() {
            return vec![0];
        }
        (0..self.lines.len())
            .filter(|&i| matches!(self.lines[i].kind, Kind::Add | Kind::Del))
            .collect()
    }

    /// The patch of the units `picked` keeps, `None` when it keeps none.
    fn patch(&self, picked: impl Fn(usize) -> bool) -> Option<String> {
        if self.whole.is_some() {
            return picked(0).then(|| with_newline(&self.raw));
        }
        let chosen: BTreeSet<usize> = self.units().into_iter().filter(|&i| picked(i)).collect();
        if chosen.is_empty() {
            return None;
        }
        diff::partial_patch(&self.header, &self.lines, &chosen, Pick::Stage)
    }
}

fn with_newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    }
}

/// Why the lines of a file's patch can't be picked one by one, if they can't.
fn why_whole(raw: &str, header: &str, lines: &[DiffLine]) -> Option<&'static str> {
    let preamble: Vec<&str> = raw
        .split_terminator('\n')
        .take_while(|l| !l.starts_with("@@"))
        .collect();
    let has = |start: &str| preamble.iter().any(|l| l.starts_with(start));
    if has("GIT binary patch") || has("Binary files") {
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-2dab3c8e",
        ))
    } else if has("new file mode 120000")
        || has("deleted file mode 120000")
        || preamble
            .iter()
            .any(|l| l.starts_with("index ") && l.ends_with(" 120000"))
    {
        // A link's target is one line: half of it is no link at all.
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-f5d6a31f",
        ))
    } else if has("deleted file mode") {
        Some("deleted")
    } else if has("old mode") || has("new mode") {
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-c7123097",
        ))
    } else if lines
        .iter()
        .any(|l| l.raw.starts_with("+Subproject commit"))
    {
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-fcedacb9",
        ))
    } else if header.is_empty() {
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-070ca29b",
        ))
    } else if lines
        .iter()
        .any(|l| l.kind == Kind::Meta && !l.raw.starts_with('\\'))
    {
        Some(crate::i18n::text(
            "repo-workflows-surgery-why-whole-fa288625",
        ))
    } else {
        None
    }
}

/// The files of `patch` (as [`crate::git::patch_between`] writes it), named by `paths` (in its
/// order, from [`crate::git::paths_between`]).
pub(crate) fn files_of(patch: &[u8], paths: Vec<String>) -> Result<Vec<FilePatch>, String> {
    let mut sections: Vec<Vec<u8>> = Vec::new();
    for line in patch.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"diff --git ") || sections.is_empty() {
            sections.push(Vec::new());
        }
        if let Some(last) = sections.last_mut() {
            last.extend_from_slice(line);
        }
    }
    sections.retain(|s| s.starts_with(b"diff --git "));
    if sections.len() != paths.len() {
        return Err(crate::i18n::message(
            "repo-workflows-surgery-files-of-758821ec",
            &[
                ("paths-len-1", (paths.len()).clone().into()),
                ("sections-len-2", (sections.len()).clone().into()),
            ],
        ));
    }
    paths
        .into_iter()
        .zip(sections)
        .map(|(path, raw)| match String::from_utf8(raw) {
            Ok(raw) => Ok(FilePatch::new(path, raw)),
            // A patch of it read with its odd bytes replaced would write the replacements.
            Err(_) => Err(crate::i18n::message(
                "repo-workflows-surgery-files-of-caaf4495",
                &[("path", (path).clone().into())],
            )),
        })
        .collect()
}

/// The patch of what `picked(file, unit)` keeps of `files`: one file after the other, each
/// whole or only the lines kept. Empty when it keeps nothing.
pub(crate) fn patch_of(files: &[FilePatch], picked: impl Fn(usize, usize) -> bool) -> String {
    files
        .iter()
        .enumerate()
        .filter_map(|(f, file)| file.patch(|unit| picked(f, unit)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "\
diff --git a/a.txt b/a.txt
index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,3 @@
 one
-two
+deux
 three
diff --git a/bin.png b/bin.png
new file mode 100644
index 0000000000000000000000000000000000000000..3333333333333333333333333333333333333333
GIT binary patch
literal 1
Icmd;J00031

literal 0
HcmV?d00001

diff --git a/new file.txt b/new file.txt
new file mode 100644
index 0000000000000000000000000000000000000000..4444444444444444444444444444444444444444
--- /dev/null
+++ b/new file.txt
@@ -0,0 +1,2 @@
+héllo
+wörld
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 5555555555555555555555555555555555555555..0000000000000000000000000000000000000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/run.sh b/run.sh
old mode 100644
new mode 100755
";

    fn files() -> Vec<FilePatch> {
        let paths = ["a.txt", "bin.png", "new file.txt", "gone.txt", "run.sh"]
            .map(str::to_owned)
            .to_vec();
        files_of(PATCH.as_bytes(), paths).unwrap_or_default()
    }

    #[test]
    fn files_and_what_can_be_picked_of_them() {
        let files = files();
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].whole, None);
        assert_eq!(files[0].units(), [2, 3]);
        assert_eq!(files[1].whole, Some("a binary file"));
        assert_eq!(files[2].whole, None);
        assert_eq!(files[3].whole, Some("deleted"));
        assert_eq!(files[4].whole, Some("its mode changes"));
        assert_eq!(files[4].units(), [0]);
        assert!(files[1].raw.contains("literal 0"));
    }

    #[test]
    fn a_patch_of_some_lines_and_some_files() {
        let files = files();
        // Only "deux" of a.txt (its removal stays as context), the first line of the new file,
        // and the binary file whole.
        let patch = patch_of(&files, |f, u| match f {
            0 => u == 3,
            1 => true,
            2 => u == 1,
            _ => false,
        });
        assert!(
            patch.contains("@@ -1,3 +1,4 @@\n one\n two\n+deux\n three\n"),
            "{patch}"
        );
        assert!(patch.contains("GIT binary patch"));
        assert!(patch.contains("+héllo\n") && !patch.contains("wörld"));
        assert!(!patch.contains("gone.txt") && !patch.contains("run.sh"));
        assert_eq!(patch_of(&files, |_, _| false), "");
    }

    #[test]
    fn a_patch_whose_listing_does_not_match_is_refused() {
        assert!(files_of(PATCH.as_bytes(), vec!["a.txt".to_owned()]).is_err());
        assert!(files_of(b"diff --git a/x b/x\n+caf\xe9\n", vec!["x".to_owned()]).is_err());
        // U+FFFD written as such is text like any other.
        assert!(
            files_of(
                "diff --git a/x b/x\n+\u{FFFD}\n".as_bytes(),
                vec!["x".to_owned()]
            )
            .is_ok()
        );
        assert!(files_of(b"", Vec::new()).is_ok_and(|f| f.is_empty()));
    }
}
