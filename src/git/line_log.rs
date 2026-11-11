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
const PLAIN_PATCH: [&str; 5] = [
    "--no-textconv",
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// Resolve precisely the commit, historical path and range consumed by a line-history read.
/// Status alone cannot detect edits that retain the same porcelain status code.
pub(crate) fn line_history_key(
    repo: &Path,
    at: &LinesAt,
    path: &str,
    range: LineRange,
) -> Result<(String, String, LineRange), String> {
    let oid = resolve_line_commit(at, |args, codes| {
        super::run_code(Some(repo), args, codes)
            .map(|(code, out)| (code, String::from_utf8_lossy(&out).into_owned()))
    })?;
    line_history_key_from_commit(repo, at, path, range, oid.trim())
}

fn resolve_line_commit(
    at: &LinesAt,
    mut run: impl FnMut(&[&str], &[i32]) -> Result<(i32, String), String>,
) -> Result<String, String> {
    let rev = match at {
        LinesAt::Rev(rev) => rev.as_str(),
        _ => "HEAD",
    };
    let resolved = run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
        &[0],
    );
    match resolved {
        Ok((_, oid)) => Ok(oid),
        Err(error) => {
            // Only a confirmed absent branch ref is an unborn HEAD. Preserve
            // corruption, transport, permission and explicit-revision errors.
            if !matches!(at, LinesAt::Rev(_))
                && let Ok((_, branch)) = run(&["symbolic-ref", "--quiet", "HEAD"], &[0])
                && branch.trim().starts_with("refs/heads/")
                && matches!(
                    run(&["show-ref", "--verify", "--quiet", branch.trim()], &[0, 1]),
                    Ok((1, _))
                )
            {
                return Err(crate::i18n::text(
                    "backend-the-repository-has-no-commits-yet-so-these-lines-have-no-history-87624b",
                )
                .to_owned());
            }
            Err(error)
        }
    }
}

/// All rename and range mapping commands use the same resolved commit, even if HEAD moves.
pub(crate) fn line_history_key_from_commit(
    repo: &Path,
    at: &LinesAt,
    path: &str,
    range: LineRange,
    oid: &str,
) -> Result<(String, String, LineRange), String> {
    let (path, range) = match at {
        LinesAt::Rev(_) => (path.to_owned(), ordered(range)),
        LinesAt::Index | LinesAt::WorkTree => committed_range(repo, at, path, range, oid)?,
    };
    Ok((oid.to_owned(), path, range))
}

/// Every commit that touched lines `range` of `path` as it is `at`, newest first. Lines not
/// committed yet (picked on disk or in the index) are first found in HEAD.
pub fn line_history(
    repo: &Path,
    at: &LinesAt,
    path: &str,
    range: LineRange,
) -> Result<Vec<FileCommit>, String> {
    let (rev, path, range) = line_history_key(repo, at, path, range)?;
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
        PLAIN_PATCH[4],
        LINE_LOG_FORMAT,
        &spec,
        super::END_OPTS,
        rev.as_str(),
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
        return Err(crate::i18n::message(
            "backend-no-lines-of-path-picked-1b9f1e",
            &[("path", (path).to_string().into())],
        ));
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
        PLAIN_PATCH[4],
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
        crate::i18n::message(
            "backend-those-lines-aren-t-in-path-at-that-commit-err-dca525",
            &[
                ("path", (path).to_string().into()),
                ("detail", (err).to_string().into()),
            ],
        )
    } else if err.contains("no path") || err.contains("There is no path") {
        crate::i18n::message(
            "backend-path-isn-t-in-that-commit-so-its-lines-have-no-history-there-e1e1f3",
            &[("path", (path).to_string().into())],
        )
    } else if err.contains("unknown revision") || err.contains("bad revision") {
        crate::i18n::text(
            "backend-the-repository-has-no-commits-yet-so-these-lines-have-no-history-87624b",
        )
        .to_owned()
    } else {
        crate::i18n::message(
            "backend-could-not-read-the-history-of-these-lines-err-e0dc46",
            &[("detail", (err).to_string().into())],
        )
    }
}

/// The name `path` had in HEAD, when a rename of it is staged.
fn name_in_head(repo: &Path, path: &str, oid: &str) -> Option<String> {
    let out = run(
        Some(repo),
        &["diff", "--cached", "-M", "--name-status", "-z", oid],
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
    oid: &str,
) -> Result<(String, LineRange), String> {
    let old = name_in_head(repo, path, oid);
    let mut args = vec![
        super::LITERAL_PATHS,
        "diff",
        "-M",
        "--unified=0",
        PLAIN_PATCH[0],
        PLAIN_PATCH[1],
        PLAIN_PATCH[2],
        PLAIN_PATCH[3],
        PLAIN_PATCH[4],
    ];
    if *at == LinesAt::Index {
        args.push("--cached");
    }
    args.extend([oid, "--", path]);
    // A rename limited to its new name would look like a new file.
    args.extend(old.as_deref());
    let diff = run(Some(repo), &args).map_err(|e| line_log_error(&e, path))?;
    if diff
        .lines()
        .take_while(|l| !l.starts_with("@@"))
        .any(|l| l.starts_with("new file mode") || l == "--- /dev/null")
    {
        return Err(crate::i18n::message(
            "backend-path-isn-t-committed-yet-so-its-lines-have-no-history-1cb9ba",
            &[("path", (path).to_string().into())],
        ));
    }
    let range = map_to_old(&diff, ordered(range)).ok_or_else(|| {
        crate::i18n::text(
            "backend-these-lines-aren-t-committed-yet-so-they-have-no-history-commit-t-f54ae1",
        )
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
mod tests;
