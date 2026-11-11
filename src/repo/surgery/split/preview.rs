//! What each new commit of a split changes, as checked: its patch line by line, ready to be
//! shown a screenful at a time. A binary file's encoded contents are one line saying so.

/// How a line of the preview reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// The heading of new commit `0`, with its file count.
    Commit(usize, usize),
    /// A file's `diff --git` line or a hunk header.
    Header,
    /// `---`, `+++`, `index` and the like.
    Meta,
    Add,
    Del,
    Context,
}

/// Every line of every new commit's patch, in order.
#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub lines: Vec<(Look, String)>,
}

impl Preview {
    /// From each new commit's patch and the files it changes.
    pub fn new(patches: &[(String, Vec<String>)]) -> Self {
        let mut lines = Vec::new();
        for (g, (patch, files)) in patches.iter().enumerate() {
            lines.push((Look::Commit(g, files.len()), files.join(", ")));
            let mut binary = false;
            for line in patch.lines() {
                if line.starts_with("diff ") {
                    binary = false;
                    lines.push((Look::Header, line.to_owned()));
                } else if binary {
                    continue;
                } else if line.starts_with("GIT binary patch") {
                    binary = true;
                    lines.push((Look::Meta, "(binary contents)".to_owned()));
                } else if line.starts_with("@@") {
                    lines.push((Look::Header, line.to_owned()));
                } else if line.starts_with("+++") || line.starts_with("---") {
                    lines.push((Look::Meta, line.to_owned()));
                } else if line.starts_with('+') {
                    lines.push((Look::Add, line.to_owned()));
                } else if line.starts_with('-') {
                    lines.push((Look::Del, line.to_owned()));
                } else if line.starts_with(' ') {
                    lines.push((Look::Context, line.to_owned()));
                } else {
                    lines.push((Look::Meta, line.to_owned()));
                }
            }
        }
        Self { lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_of_each_commit_and_binary_contents_left_out() {
        let first =
            "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n c\n";
        let second = "diff --git a/p b/p\nGIT binary patch\nliteral 3\nKcmZQzWMT\n\nliteral 0\nHcmV?d00001\n\ndiff --git a/y b/y\n";
        let preview = Preview::new(&[
            (first.to_owned(), vec!["x".to_owned()]),
            (second.to_owned(), vec!["p".to_owned(), "y".to_owned()]),
        ]);
        let looks: Vec<Look> = preview.lines.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            looks,
            [
                Look::Commit(0, 1),
                Look::Header,
                Look::Meta,
                Look::Meta,
                Look::Meta,
                Look::Header,
                Look::Del,
                Look::Add,
                Look::Context,
                Look::Commit(1, 2),
                Look::Header,
                Look::Meta,
                Look::Header,
            ]
        );
        assert_eq!(preview.lines[9].1, "p, y");
        assert_eq!(preview.lines[11].1, "(binary contents)");
    }
}
