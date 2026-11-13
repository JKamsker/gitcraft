//! Reading the diff whose edits are compared: which files changed (`--numstat`), then their
//! hunks without context (`-U0`), within limits.

use std::path::Path;

use super::Limits;
use crate::git;

/// A changed file as `--numstat -z` lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub path: String,
    pub old_path: Option<String>,
    /// Lines added and deleted; `None` for a binary file.
    pub lines: Option<u32>,
}

/// What `git diff --numstat -z -M` printed.
pub fn parse_numstat(out: &str) -> Vec<Listed> {
    let mut files = Vec::new();
    let mut tokens = out.split('\0');
    while let Some(token) = tokens.next() {
        let token = token.trim_start_matches('\n');
        if token.is_empty() {
            continue;
        }
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let lines = match (added.parse::<u32>(), deleted.parse::<u32>()) {
            (Ok(a), Ok(d)) => Some(a + d),
            _ => None,
        };
        let (path, old_path) = if path.is_empty() {
            let old = tokens.next().unwrap_or("").to_owned();
            let new = tokens.next().unwrap_or("").to_owned();
            (new, Some(old))
        } else {
            (path.to_owned(), None)
        };
        files.push(Listed {
            path,
            old_path,
            lines,
        });
    }
    files
}

/// A hunk of a `-U0` diff: the lines removed and added, where.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hunk {
    /// The file it is in (its new name; its old one when it was deleted).
    pub path: String,
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub removed: Vec<String>,
    pub added: Vec<String>,
    /// What Git prints after the second `@@`: the function or section it is in, if any.
    pub section: String,
}

/// The hunks of a `-U0` patch (with `a/` and `b/` prefixes), at most `max` of them: the hunks,
/// and whether there were more.
pub fn parse_patch(text: &str, max: usize) -> (Vec<Hunk>, bool) {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut old_path = String::new();
    let mut path = String::new();
    let mut current: Option<Hunk> = None;
    let mut cut = false;
    let finish = |current: &mut Option<Hunk>, hunks: &mut Vec<Hunk>| {
        if let Some(h) = current.take() {
            hunks.push(h);
        }
    };
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(hunk) = current.as_mut() {
            let want = (hunk.removed.len() as u32) < hunk.old_len
                || (hunk.added.len() as u32) < hunk.new_len;
            if want {
                if let Some(rest) = line.strip_prefix('-') {
                    hunk.removed.push(rest.to_owned());
                    continue;
                }
                if let Some(rest) = line.strip_prefix('+') {
                    hunk.added.push(rest.to_owned());
                    continue;
                }
            }
            if line.starts_with('\\') {
                continue;
            }
            finish(&mut current, &mut hunks);
        }
        if line.starts_with("diff --git ") {
            old_path.clear();
            path.clear();
        } else if let Some(name) = line.strip_prefix("--- ") {
            old_path = side(name, "a/");
        } else if let Some(name) = line.strip_prefix("+++ ") {
            path = side(name, "b/");
            if path.is_empty() {
                path = old_path.clone();
            }
        } else if let Some(header) = line.strip_prefix("@@ ") {
            if hunks.len() >= max {
                cut = true;
                break;
            }
            let Some((old_start, old_len, new_start, new_len)) = parse_header(header) else {
                continue;
            };
            current = Some(Hunk {
                path: path.clone(),
                old_start,
                old_len,
                new_start,
                new_len,
                section: header
                    .split_once("@@")
                    .map_or("", |(_, s)| s.trim())
                    .to_owned(),
                ..Hunk::default()
            });
        }
    }
    finish(&mut current, &mut hunks);
    (hunks, cut)
}

/// A file name of a `---` or `+++` line: without its prefix, `""` for `/dev/null`.
fn side(name: &str, prefix: &str) -> String {
    let name = name.split('\t').next().unwrap_or(name);
    if name == "/dev/null" {
        return String::new();
    }
    let name = git::unquote_path(name);
    name.strip_prefix(prefix).unwrap_or(&name).to_owned()
}

/// `-a,b +c,d @@`: (a, b, c, d), a missing length being 1.
fn parse_header(header: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = header.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (a, b) = range(old)?;
    let (c, d) = range(new)?;
    Some((a, b, c, d))
}

/// What reading a diff gave: its hunks, and what was left out of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Read {
    pub hunks: Vec<Hunk>,
    /// Files listed, binaries included.
    pub listed: usize,
    pub binary: usize,
    /// Files not read: too many lines changed, or beyond the file limit.
    pub too_large: Vec<String>,
    pub beyond: usize,
    /// More hunks than the limit.
    pub hunks_cut: bool,
}

/// What makes git print a plain patch whatever its configuration says.
const PLAIN: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-M",
];

/// The hunks from `base` to `head`, read within `limits`.
pub fn read(repo: &Path, base: &str, head: &str, limits: &Limits) -> Result<Read, String> {
    for rev in [base, head] {
        if rev.starts_with('-') {
            return Err("not a revision".to_owned());
        }
    }
    let mut args = vec!["-c", "core.quotePath=false", "diff", "--numstat", "-z"];
    args.extend(PLAIN);
    args.extend([base, head, "--"]);
    let listed = parse_numstat(&git::run(Some(repo), &args)?);
    let mut out = Read {
        listed: listed.len(),
        ..Read::default()
    };
    // Each file read, with its old name: a rename is read as one, never split across commands.
    let mut entries: Vec<Vec<&str>> = Vec::new();
    let mut taken = 0;
    for file in &listed {
        let Some(lines) = file.lines else {
            out.binary += 1;
            continue;
        };
        if lines > limits.max_file_lines {
            out.too_large.push(file.path.clone());
            continue;
        }
        if taken >= limits.max_files {
            out.beyond += 1;
            continue;
        }
        taken += 1;
        entries.push([Some(file.path.as_str()), file.old_path.as_deref()].into_iter().flatten().collect());
    }
    // A few files at a time, so that the command line stays short.
    for chunk in chunks(&entries, 80) {
        let left = limits.max_hunks.saturating_sub(out.hunks.len());
        if left == 0 {
            out.hunks_cut = true;
            break;
        }
        let mut args = vec![
            "-c",
            "core.quotePath=false",
            "--literal-pathspecs",
            "diff",
            "-U0",
        ];
        args.extend(PLAIN);
        args.extend([base, head, "--"]);
        args.extend(chunk);
        let (hunks, cut) = parse_patch(&git::run(Some(repo), &args)?, left);
        out.hunks.extend(hunks);
        out.hunks_cut |= cut;
    }
    Ok(out)
}

/// The paths of `entries` in runs of at most `max`, each entry whole in one run.
fn chunks<'a>(entries: &[Vec<&'a str>], max: usize) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&'a str>> = Vec::new();
    for entry in entries {
        match out.last_mut() {
            Some(run) if run.len() + entry.len() <= max => run.extend(entry.iter().copied()),
            _ => out.push(entry.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_lists_renames_and_binaries() {
        let out = "1\t2\ta.rs\0-\t-\tlogo.png\0\n3\t0\t\0old.rs\0new.rs\0";
        let files = parse_numstat(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].lines, Some(3));
        assert_eq!(files[1].lines, None);
        assert_eq!(files[2].path, "new.rs");
        assert_eq!(files[2].old_path.as_deref(), Some("old.rs"));
    }

    #[test]
    fn a_patch_without_context_is_split_into_its_hunks() {
        let patch = "diff --git a/x.rs b/x.rs\n\
                     index 1..2 100644\n\
                     --- a/x.rs\n\
                     +++ b/x.rs\n\
                     @@ -3 +3 @@ fn main() {\n\
                     -    foo(a);\n\
                     +    foo(a, ctx);\n\
                     @@ -10,2 +10,0 @@\n\
                     -gone\n\
                     -also gone\n\
                     \\ No newline at end of file\n\
                     diff --git a/old.rs b/old.rs\n\
                     deleted file mode 100644\n\
                     --- a/old.rs\n\
                     +++ /dev/null\n\
                     @@ -1 +0,0 @@\n\
                     --- a comment\n\
                     diff --git a/new.rs b/new.rs\n\
                     --- /dev/null\n\
                     +++ b/new.rs\n\
                     @@ -0,0 +1 @@\n\
                     ++++ x\n";
        let (hunks, cut) = parse_patch(patch, 10);
        assert!(!cut);
        assert_eq!(hunks.len(), 4);
        assert_eq!(hunks[0].path, "x.rs");
        assert_eq!((hunks[0].old_start, hunks[0].new_start), (3, 3));
        assert_eq!(hunks[0].removed, ["    foo(a);"]);
        assert_eq!(hunks[0].added, ["    foo(a, ctx);"]);
        assert_eq!(hunks[0].section, "fn main() {");
        assert_eq!(hunks[1].section, "");
        assert_eq!(hunks[1].removed.len(), 2);
        assert_eq!(hunks[1].new_len, 0);
        // A deleted file's hunk is under its old name; a removed line may start with `--`.
        assert_eq!(hunks[2].path, "old.rs");
        assert_eq!(hunks[2].removed, ["-- a comment"]);
        assert_eq!(hunks[3].path, "new.rs");
        assert_eq!(hunks[3].added, ["+++ x"]);
        let (few, cut) = parse_patch(patch, 2);
        assert_eq!(few.len(), 2);
        assert!(cut);
    }

    #[test]
    fn a_rename_and_its_old_name_are_never_read_apart() {
        let entries: Vec<Vec<&str>> = (0..79)
            .map(|_| vec!["m.rs"])
            .chain([vec!["new.rs", "old.rs"], vec!["z.rs"]])
            .collect();
        let runs = chunks(&entries, 80);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].len(), 79);
        assert_eq!(runs[1], ["new.rs", "old.rs", "z.rs"]);
        assert!(chunks(&[], 80).is_empty());
    }

    #[test]
    fn quoted_names_are_unquoted() {
        assert_eq!(side("\"b/a \\\"q\\\".rs\"", "b/"), "a \"q\".rs");
        assert_eq!(side("/dev/null", "b/"), "");
        assert_eq!(parse_header("-5,0 +6,2 @@ x"), Some((5, 0, 6, 2)));
        assert_eq!(parse_header("nonsense"), None);
    }
}
