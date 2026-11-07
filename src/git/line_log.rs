//! The history of a few lines of a file (`git log -L`): every commit that touched them,
//! following the lines through later edits and renames, and the part of each commit's diff
//! that changed them.

use std::path::Path;

use super::{FS, FileCommit, NO_SIGNATURE, RS, UTF8, run};

/// Where the numbers of the lines picked count from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LinesAt {
    /// The file as it is in a commit (or any revision).
    Rev(String),
    /// The file as it is staged.
    Index,
    /// The file on disk.
    WorkTree,
}

/// The lines `start..=end` (counted from 1) of a file.
pub type LineRange = (u32, u32);

/// What `git log -L` prints commits as: a record separator, the fields, then the patch.
const LINE_LOG_FORMAT: &str = "--format=\x1e%H\x1f%an\x1f%ae\x1f%at\x1f%s";

/// What makes git print a plain patch, whatever the user's config says.
const PLAIN_PATCH: [&str; 4] = [
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// Every commit that touched lines `range` of `path` as it is `at`, newest first. Lines not
/// committed yet (picked on disk or in the index) are first found in HEAD.
pub fn line_history(
    repo: &Path,
    at: &LinesAt,
    path: &str,
    range: LineRange,
) -> Result<Vec<FileCommit>, String> {
    let (rev, path, range) = match at {
        LinesAt::Rev(rev) => (rev.as_str(), path.to_owned(), ordered(range)),
        LinesAt::Index | LinesAt::WorkTree => {
            let (path, range) = committed_range(repo, at, path, range)?;
            ("HEAD", path, range)
        }
    };
    let path = path.as_str();
    let spec = line_spec(range, path);
    let args = [
        "log",
        NO_SIGNATURE,
        UTF8,
        PLAIN_PATCH[0],
        PLAIN_PATCH[1],
        PLAIN_PATCH[2],
        PLAIN_PATCH[3],
        LINE_LOG_FORMAT,
        &spec,
        super::END_OPTS,
        rev,
    ];
    let out = run(Some(repo), &args).map_err(|e| line_log_error(&e, path))?;
    Ok(parse_line_log(&out, path))
}

/// The part of commit `hash`'s diff that changed lines `ranges` of `path` (as they are in
/// that commit), as a one-file patch.
pub fn line_diff(
    repo: &Path,
    hash: &str,
    path: &str,
    ranges: &[LineRange],
) -> Result<String, String> {
    if ranges.is_empty() {
        return Err(format!("No lines of {path} picked"));
    }
    let specs: Vec<String> = ranges.iter().map(|&r| line_spec(r, path)).collect();
    let mut args = vec![
        "log",
        NO_SIGNATURE,
        UTF8,
        "--format=",
        "-n",
        "1",
        PLAIN_PATCH[0],
        PLAIN_PATCH[1],
        PLAIN_PATCH[2],
        PLAIN_PATCH[3],
    ];
    args.extend(specs.iter().map(String::as_str));
    args.extend([super::END_OPTS, hash]);
    let out = run(Some(repo), &args).map_err(|e| line_log_error(&e, path))?;
    Ok(out.trim_start_matches('\n').to_owned())
}

/// `-L start,end:path`. git takes everything after the range's `:` as the name, so a name
/// with a `:` (or a `,`) in it is read whole.
fn line_spec((start, end): LineRange, path: &str) -> String {
    format!("-L{start},{end}:{path}")
}

fn ordered((a, b): LineRange) -> LineRange {
    (a.min(b).max(1), a.max(b).max(1))
}

/// git's error for a range it can't follow, in words that say what to do.
fn line_log_error(err: &str, path: &str) -> String {
    if err.contains("has only") && err.contains("lines") {
        format!("Those lines aren't in {path} at that commit: {err}")
    } else if err.contains("no path") || err.contains("There is no path") {
        format!("{path} isn't in that commit, so its lines have no history there")
    } else if err.contains("unknown revision") || err.contains("bad revision") {
        "The repository has no commits yet, so these lines have no history".to_owned()
    } else {
        format!("Could not read the history of these lines: {err}")
    }
}

/// The name `path` had in HEAD, when a rename of it is staged.
fn name_in_head(repo: &Path, path: &str) -> Option<String> {
    let out = run(
        Some(repo),
        &["diff", "--cached", "-M", "--name-status", "-z", "HEAD"],
    )
    .ok()?;
    let mut fields = out.split('\0');
    while let Some(status) = fields.next() {
        if status.starts_with('R') || status.starts_with('C') {
            let (from, to) = (fields.next()?, fields.next()?);
            if to == path && status.starts_with('R') {
                return Some(from.to_owned());
            }
        } else {
            fields.next()?;
        }
    }
    None
}

/// Lines `range` of `path` as it is on disk or in the index, found in HEAD, with the name the
/// file has there (it may be renamed since): the lines added since have no history and are
/// left out; `Err` when every one of them is new.
fn committed_range(
    repo: &Path,
    at: &LinesAt,
    path: &str,
    range: LineRange,
) -> Result<(String, LineRange), String> {
    let old = name_in_head(repo, path);
    let mut args = vec![
        super::LITERAL_PATHS,
        "diff",
        "-M",
        "--unified=0",
        PLAIN_PATCH[0],
        PLAIN_PATCH[1],
        PLAIN_PATCH[2],
        PLAIN_PATCH[3],
    ];
    if *at == LinesAt::Index {
        args.push("--cached");
    }
    args.extend(["HEAD", "--", path]);
    // A rename limited to its new name would look like a new file.
    args.extend(old.as_deref());
    let diff = run(Some(repo), &args).map_err(|e| line_log_error(&e, path))?;
    if diff
        .lines()
        .take_while(|l| !l.starts_with("@@"))
        .any(|l| l.starts_with("new file mode") || l == "--- /dev/null")
    {
        return Err(format!(
            "{path} isn't committed yet, so its lines have no history"
        ));
    }
    let range = map_to_old(&diff, ordered(range)).ok_or_else(|| {
        "These lines aren't committed yet, so they have no history. Commit them, or pick lines \
         that are already committed."
            .to_owned()
    })?;
    Ok((old.unwrap_or_else(|| path.to_owned()), range))
}

/// The lines of the old side that lines `range` of the new side of a `--unified=0` diff were,
/// leaving out those the diff added. `None` when every line in `range` was added.
pub(crate) fn map_to_old(diff: &str, (start, end): LineRange) -> Option<LineRange> {
    // Each hunk: where its new lines start, how many there are, and how many old ones they
    // replace.
    let hunks: Vec<(u32, u32, u32)> = diff
        .lines()
        .filter_map(hunk_ranges)
        .map(|((_, old_n), (new_start, new_n))| (new_start, new_n, old_n))
        .collect();
    let mut found: Option<LineRange> = None;
    for line in start..=end {
        let mut shift: i64 = 0;
        let mut added = false;
        for &(new_start, new_n, old_n) in &hunks {
            // A hunk adding nothing (`+12,0`) sits after line 12.
            let new_end = new_start + new_n;
            if new_n > 0 && (new_start..new_end).contains(&line) {
                added = true;
                break;
            }
            let before = if new_n == 0 {
                new_start < line
            } else {
                new_end <= line
            };
            if before {
                shift += i64::from(new_n) - i64::from(old_n);
            }
        }
        if added {
            continue;
        }
        let Ok(old) = u32::try_from(i64::from(line) - shift) else {
            continue;
        };
        if old == 0 {
            continue;
        }
        found = Some(match found {
            None => (old, old),
            Some((a, b)) => (a.min(old), b.max(old)),
        });
    }
    found
}

/// `@@ -a,b +c,d @@…` → ((a, b), (c, d)), a missing count being 1.
fn hunk_ranges(header: &str) -> Option<((u32, u32), (u32, u32))> {
    let rest = header.strip_prefix("@@ -")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let parse = |r: &str| -> Option<(u32, u32)> {
        let mut it = r.split(',');
        let start = it.next()?.parse().ok()?;
        let count = match it.next() {
            Some(n) => n.parse().ok()?,
            None => 1,
        };
        Some((start, count))
    };
    Some((parse(old)?, parse(new)?))
}

/// `git log -L` output: for each commit, its fields and the patch of the lines it changed.
/// A commit with no patch (a merge that took the lines from one side as they were) keeps the
/// path of the commit listed before it, `path` for the newest.
pub(crate) fn parse_line_log(out: &str, path: &str) -> Vec<FileCommit> {
    let mut commits: Vec<FileCommit> = Vec::new();
    let mut in_header = false;
    for line in out.split('\n') {
        // Only at the start of a line: a line of the patch starts with its mark, so a record
        // separator in the file's text can't start a commit.
        if let Some(fields) = line.strip_prefix(RS) {
            let mut f = fields.split(FS);
            let hash = f.next().unwrap_or_default().to_owned();
            if hash.is_empty() {
                continue;
            }
            in_header = false;
            let last_path = commits.last().map_or(path, |c| c.path.as_str()).to_owned();
            commits.push(FileCommit {
                hash,
                author: f.next().unwrap_or_default().to_owned(),
                email: f.next().unwrap_or_default().to_owned(),
                time: f.next().and_then(|t| t.parse().ok()).unwrap_or(0),
                summary: f.next().unwrap_or_default().to_owned(),
                path: last_path,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(commit) = commits.last_mut() else {
            continue;
        };
        // The file's name only in a diff's header: an added line `++ b/x` reads `+++ b/x`.
        if line.starts_with("diff ") {
            in_header = true;
        } else if in_header && let Some(name) = line.strip_prefix("+++ ") {
            // Some versions of git end a name with a space in it with a tab, as GNU diff does.
            // A tab of the name itself would be quoted (`\t`).
            let name = name.strip_suffix('\t').unwrap_or(name);
            if let Some(name) = unquote_path(name).strip_prefix("b/") {
                commit.path = name.to_owned();
            }
        } else if let Some(((_, _), (start, count))) = hunk_ranges(line) {
            in_header = false;
            if count > 0 {
                commit.lines.push((start, start + count - 1));
            }
        }
    }
    commits
}

/// A file name as git prints it in a patch: in double quotes, with C escapes (`\"`, `\t`,
/// `\303\251` for the bytes of `é`), when it has characters git quotes.
pub fn unquote_path(name: &str) -> String {
    let Some(inner) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) else {
        return name.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('r') => bytes.push(b'\r'),
            Some('a') => bytes.push(7),
            Some('b') => bytes.push(8),
            Some('f') => bytes.push(12),
            Some('v') => bytes.push(11),
            Some(d @ '0'..='7') => {
                let mut value = d.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    if let Some(next) = chars.peek().and_then(|c| c.to_digit(8)) {
                        value = value * 8 + next;
                        chars.next();
                    }
                }
                bytes.push(u8::try_from(value).unwrap_or(b'?'));
            }
            Some(other) => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_log_output_is_read_commit_by_commit() {
        let out = format!(
            "{RS}aaa{FS}Ann{FS}ann@x{FS}300{FS}Edit\n\ndiff --git a/new name.txt b/new name.txt\n\
             --- a/new name.txt\n+++ b/new name.txt\n@@ -4,3 +5,3 @@\n 4\n-6\n+six\n\
             {RS}mmm{FS}Max{FS}max@x{FS}250{FS}Merge\n\n\
             {RS}bbb{FS}Bob{FS}bob@x{FS}200{FS}Two hunks\n\ndiff --git a/old.txt b/old.txt\n\
             --- a/old.txt\n+++ b/old.txt\n@@ -2,1 +2,1 @@\n-x\n+y\n@@ -9 +9,2 @@\n-z\n+z\n+w\n\
             {RS}ccc{FS}Cy{FS}cy@x{FS}100{FS}Add\n\ndiff --git a/old.txt b/old.txt\n\
             --- /dev/null\n+++ b/old.txt\n@@ -0,0 +1,3 @@\n+a\n+b\n+c\n"
        );
        let h = parse_line_log(&out, "new name.txt");
        let summary: Vec<(&str, &str, Vec<LineRange>)> = h
            .iter()
            .map(|c| (c.hash.as_str(), c.path.as_str(), c.lines.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("aaa", "new name.txt", vec![(5, 7)]),
                // No patch: the path of the commit after it.
                ("mmm", "new name.txt", vec![]),
                ("bbb", "old.txt", vec![(2, 2), (9, 10)]),
                ("ccc", "old.txt", vec![(1, 3)]),
            ]
        );
        assert_eq!((h[0].author.as_str(), h[0].time), ("Ann", 300));
        assert_eq!(h[2].summary, "Two hunks");
    }

    #[test]
    fn a_record_separator_in_the_file_does_not_start_a_commit() {
        let out = format!(
            "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/f b/f\n--- a/f\n+++ b/f\n\
             @@ -1 +1 @@\n-{RS}old\n+{RS}new{FS}x\n"
        );
        let h = parse_line_log(&out, "f");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].lines, vec![(1, 1)]);
    }

    #[test]
    fn an_added_line_that_looks_like_a_file_header_is_not_one() {
        // The file's text has a line `++ b/evil`: added, git prints it `+++ b/evil`.
        let out = format!(
            "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/real.txt b/real.txt\n--- a/real.txt\n\
             +++ b/real.txt\n@@ -1,0 +2,1 @@\n+++ b/evil\n"
        );
        let h = parse_line_log(&out, "real.txt");
        assert_eq!(h[0].path, "real.txt");
        assert_eq!(h[0].lines, vec![(2, 2)]);
    }

    #[test]
    fn the_tab_after_a_name_with_a_space_is_not_part_of_it() {
        let out = format!(
            "{RS}aaa{FS}Ann{FS}a@x{FS}1{FS}S\n\ndiff --git a/big: file.txt b/big: file.txt\n\
             --- a/big: file.txt\t\n+++ b/big: file.txt\t\n@@ -1 +1 @@\n-a\n+b\n"
        );
        assert_eq!(parse_line_log(&out, "x")[0].path, "big: file.txt");
    }

    #[test]
    fn nothing_printed_is_no_commits() {
        assert!(parse_line_log("", "f").is_empty());
        assert!(parse_line_log("\n\n", "f").is_empty());
    }

    #[test]
    fn quoted_names_are_read_back() {
        assert_eq!(unquote_path("b/plain name.txt"), "b/plain name.txt");
        assert_eq!(unquote_path("\"b/q\\\"t\\tx.txt\""), "b/q\"t\tx.txt");
        assert_eq!(unquote_path("\"b/\\303\\274ber.txt\""), "b/über.txt");
        assert_eq!(unquote_path("\"b/back\\\\slash\""), "b/back\\slash");
    }

    #[test]
    fn lines_on_disk_are_found_in_the_commit() {
        // Line 1 added at the top, line 6 (old 5) changed, two lines added after old 8.
        let diff = "@@ -0,0 +1 @@\n+top\n@@ -5 +6 @@\n-five\n+FIVE\n@@ -8,0 +10,2 @@\n+n1\n+n2\n";
        // Lines below everything shift by the three added.
        assert_eq!(map_to_old(diff, (12, 13)), Some((9, 10)));
        // Line 2 was line 1.
        assert_eq!(map_to_old(diff, (2, 3)), Some((1, 2)));
        // The added top line is left out; the changed line 6 too.
        assert_eq!(map_to_old(diff, (1, 6)), Some((1, 4)));
        // Only added lines: nothing to follow.
        assert_eq!(map_to_old(diff, (1, 1)), None);
        assert_eq!(map_to_old(diff, (10, 11)), None);
        // Line 9 is old 8, the lines added after it don't move it.
        assert_eq!(map_to_old(diff, (9, 9)), Some((8, 8)));
        // No changes: the same lines.
        assert_eq!(map_to_old("", (4, 9)), Some((4, 9)));
    }

    #[test]
    fn removed_lines_move_the_ones_after_them_up() {
        // Old lines 2–4 removed.
        let diff = "@@ -2,3 +1,0 @@\n-a\n-b\n-c\n";
        assert_eq!(map_to_old(diff, (1, 1)), Some((1, 1)));
        assert_eq!(map_to_old(diff, (2, 3)), Some((5, 6)));
    }

    #[test]
    fn ranges_are_put_in_order() {
        assert_eq!(ordered((9, 3)), (3, 9));
        assert_eq!(ordered((0, 0)), (1, 1));
        assert_eq!(line_spec((3, 9), "a:b, c.txt"), "-L3,9:a:b, c.txt");
    }

    #[test]
    fn hunk_headers_with_and_without_counts() {
        assert_eq!(hunk_ranges("@@ -4,3 +5,3 @@"), Some(((4, 3), (5, 3))));
        assert_eq!(hunk_ranges("@@ -9 +9,2 @@ fn x"), Some(((9, 1), (9, 2))));
        assert_eq!(hunk_ranges("@@ -0,0 +1 @@"), Some(((0, 0), (1, 1))));
        assert_eq!(hunk_ranges(" 4"), None);
    }
}
