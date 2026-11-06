//! The hunks of an attempt's change to a file, without context (`diff -U0`), and the base file
//! with some of them applied, from one attempt or several.

/// One change to a file, relative to the base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hunk {
    /// `@@ -a,b +c,d @@`, as git wrote it.
    pub header: String,
    /// The first line of the base it replaces (1-based); for a hunk that only adds lines, the
    /// line they come after (0: before the first).
    pub old_start: usize,
    /// The base's lines it removes.
    pub removed: Vec<String>,
    /// The lines it puts in their place.
    pub added: Vec<String>,
    /// The base's last line had no newline, and is among those removed.
    pub old_no_newline: bool,
    /// Its last added line is the end of the file, without a newline.
    pub new_no_newline: bool,
}

impl Hunk {
    /// Where in the base's lines (0-based) it starts replacing.
    fn start(&self) -> usize {
        if self.removed.is_empty() {
            self.old_start
        } else {
            self.old_start.saturating_sub(1)
        }
    }
}

/// The hunks of a `-U0` patch of one file (empty for a binary file, which is taken whole).
pub(crate) fn parse(patch: &str) -> Vec<Hunk> {
    let mut out: Vec<Hunk> = Vec::new();
    let mut last = ' ';
    for line in patch.split_terminator('\n') {
        // A `\r` at the end belongs to the file's line (CRLF files), not to the patch.
        if let Some(header) = line.strip_prefix("@@ ") {
            let header = header.trim_end_matches('\r');
            let old = header.split_whitespace().next().unwrap_or("-0");
            let start = old
                .trim_start_matches('-')
                .split(',')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            out.push(Hunk {
                header: format!("@@ {header}"),
                old_start: start,
                removed: Vec::new(),
                added: Vec::new(),
                old_no_newline: false,
                new_no_newline: false,
            });
            last = ' ';
            continue;
        }
        let Some(hunk) = out.last_mut() else { continue };
        match line.chars().next() {
            Some('-') => {
                hunk.removed.push(line[1..].to_owned());
                last = '-';
            }
            Some('+') => {
                hunk.added.push(line[1..].to_owned());
                last = '+';
            }
            Some('\\') if last == '-' => hunk.old_no_newline = true,
            Some('\\') if last == '+' => hunk.new_no_newline = true,
            _ => {}
        }
    }
    out
}

/// `base` with `picked` applied (each a hunk of some attempt's change to the same file, against
/// this base). Two that change the same lines can't both be taken.
pub(crate) fn apply(base: &str, picked: &[&Hunk]) -> Result<String, String> {
    let ends_with_newline = base.is_empty() || base.ends_with('\n');
    let lines: Vec<&str> = base.split_terminator('\n').collect();
    let mut sorted = picked.to_vec();
    sorted.sort_by_key(|h| (h.start(), !h.removed.is_empty()));
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0;
    let mut final_newline = ends_with_newline;
    let mut previous: Option<&Hunk> = None;
    for hunk in sorted {
        let start = hunk.start();
        if start < cursor {
            let other = previous.map_or("another hunk", |p| p.header.as_str());
            return Err(format!(
                "{} and {other} change the same lines: take only one of them",
                hunk.header
            ));
        }
        let end = start + hunk.removed.len();
        let fits = lines.get(start..end).is_some_and(|there| {
            there
                .iter()
                .zip(&hunk.removed)
                .all(|(a, b)| a.trim_end_matches('\r') == b.trim_end_matches('\r'))
        });
        if !fits {
            return Err(format!(
                "{} no longer fits the base: compare the attempts again",
                hunk.header
            ));
        }
        out.extend(lines[cursor..start].iter().map(|l| (*l).to_owned()));
        out.extend(hunk.added.iter().cloned());
        cursor = end;
        if end == lines.len() {
            if hunk.new_no_newline {
                final_newline = false;
            } else if hunk.old_no_newline || !hunk.added.is_empty() {
                final_newline = true;
            }
        }
        previous = Some(hunk);
    }
    out.extend(lines[cursor..].iter().map(|l| (*l).to_owned()));
    let mut text = out.join("\n");
    if final_newline && !out.is_empty() {
        text.push('\n');
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -2 +2 @@\n-two\n+TWO\n@@ -4,0 +5,2 @@\n+four and a half\n+ü\n";

    #[test]
    fn hunks_are_read_with_their_place_in_the_base() {
        let hunks = parse(PATCH);
        assert_eq!(hunks.len(), 2);
        assert_eq!((hunks[0].old_start, hunks[0].removed.len()), (2, 1));
        assert_eq!(hunks[1].added, ["four and a half", "ü"]);
        assert!(parse("Binary files a/x and b/x differ\n").is_empty());
    }

    #[test]
    fn some_hunks_are_applied_and_the_rest_left_out() -> Result<(), String> {
        let base = "one\ntwo\nthree\nfour\nfive\n";
        let hunks = parse(PATCH);
        assert_eq!(
            apply(base, &[&hunks[1]])?,
            "one\ntwo\nthree\nfour\nfour and a half\nü\nfive\n"
        );
        assert_eq!(
            apply(base, &[&hunks[0], &hunks[1]])?,
            "one\nTWO\nthree\nfour\nfour and a half\nü\nfive\n"
        );
        assert_eq!(apply(base, &[])?, base);
        Ok(())
    }

    #[test]
    fn hunks_of_two_attempts_go_in_together_unless_they_overlap() -> Result<(), String> {
        let base = "a\nb\nc\nd\n";
        let first = parse("@@ -1 +1 @@\n-a\n+A\n");
        let second = parse("@@ -4 +4 @@\n-d\n+D\n");
        let clash = parse("@@ -1,2 +1 @@\n-a\n-b\n+x\n");
        assert_eq!(apply(base, &[&second[0], &first[0]])?, "A\nb\nc\nD\n");
        let err = apply(base, &[&first[0], &clash[0]])
            .err()
            .unwrap_or_default();
        assert!(err.contains("change the same lines"), "{err}");
        let stale = parse("@@ -2 +2 @@\n-not b\n+x\n");
        assert!(apply(base, &[&stale[0]]).is_err_and(|e| e.contains("no longer fits")));
        Ok(())
    }

    #[test]
    fn the_end_of_the_file_keeps_or_changes_its_newline() -> Result<(), String> {
        let base = "a\nb";
        let added = parse("@@ -2 +2,2 @@\n-b\n\\ No newline at end of file\n+b\n+c\n");
        assert_eq!(apply(base, &[&added[0]])?, "a\nb\nc\n");
        let still_none = parse(
            "@@ -2 +2 @@\n-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n",
        );
        assert_eq!(apply(base, &[&still_none[0]])?, "a\nB");
        // Into an empty file.
        let fresh = parse("@@ -0,0 +1 @@\n+x\n");
        assert_eq!(apply("", &[&fresh[0]])?, "x\n");
        Ok(())
    }

    #[test]
    fn crlf_lines_match_and_stay() -> Result<(), String> {
        let base = "a\r\nb\r\n";
        let hunks = parse("@@ -1 +1 @@\n-a\r\n+A\r\n");
        assert_eq!(apply(base, &[&hunks[0]])?, "A\r\nb\r\n");
        Ok(())
    }
}
