//! The lines a diff changes, file by file, as `git diff -U0` gives them: where each hunk is on
//! both sides, without its text. For views that map changed lines onto something else (the
//! items of a source file, a coverage report).

use std::path::Path;

use super::{END_OPTS, run, unquote_path};

/// One hunk: `count` lines from `start` on each side (a count of 0 is a hunk that only adds, or
/// only removes, there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
}

impl Hunk {
    /// The lines it has after the change (first and last, 1 being the first), if any.
    pub fn new_lines(&self) -> Option<(u32, u32)> {
        (self.new_count > 0).then(|| (self.new_start, self.new_start + self.new_count - 1))
    }

    /// The lines it had before the change, if any.
    pub fn old_lines(&self) -> Option<(u32, u32)> {
        (self.old_count > 0).then(|| (self.old_start, self.old_start + self.old_count - 1))
    }
}

/// A file a diff changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileHunks {
    /// Its path after the change (before it, for a file the change deletes).
    pub path: String,
    /// Its path before the change, when that is another (a rename) or it was deleted.
    pub old_path: Option<String>,
    pub added: bool,
    pub deleted: bool,
    /// Git says only that it differs.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

/// The hunks of `from` against `to` (`None`: the working tree), with renames followed, for the
/// files of `paths` (pathspecs; all when empty).
pub fn read(
    repo: &Path,
    from: &str,
    to: Option<&str>,
    paths: &[&str],
) -> Result<Vec<FileHunks>, String> {
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "diff",
        "-U0",
        "-M",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        END_OPTS,
        from,
    ];
    args.extend(to);
    args.push("--");
    args.extend(paths);
    Ok(parse(&run(Some(repo), &args)?))
}

/// What `git diff -U0` printed, with the prefixes `a/` and `b/`.
pub fn parse(out: &str) -> Vec<FileHunks> {
    let mut files: Vec<FileHunks> = Vec::new();
    // Lines of the current hunk's text still to come, which are never headers.
    let mut text_left = 0u32;
    // The current file's header is read (`+++` seen); until its next `diff --git`, `---` and
    // `+++` lines are hunk text.
    let mut in_hunks = false;
    for line in out.lines() {
        if text_left > 0 && matches!(line.as_bytes().first(), Some(b'+' | b'-' | b' ')) {
            text_left -= 1;
            continue;
        }
        if line.starts_with('\\') {
            continue;
        }
        text_left = 0;
        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_hunks = false;
            files.push(FileHunks {
                path: header_path(rest),
                ..FileHunks::default()
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if let Some(header) = line.strip_prefix("@@ ") {
            in_hunks = true;
            if let Some(hunk) = hunk_header(header) {
                text_left = hunk.old_count + hunk.new_count;
                file.hunks.push(hunk);
            }
        } else if in_hunks {
            continue;
        } else if let Some(name) = line.strip_prefix("--- ") {
            match side(name, "a/") {
                Some(old) => file.old_path = Some(old),
                None => file.added = true,
            }
        } else if let Some(name) = line.strip_prefix("+++ ") {
            match side(name, "b/") {
                Some(new) => file.path = new,
                None => file.deleted = true,
            }
            in_hunks = true;
        } else if line.starts_with("Binary files ") {
            file.binary = true;
        }
    }
    for file in &mut files {
        if file.deleted
            && let Some(old) = &file.old_path
        {
            file.path = old.clone();
        }
        if file.old_path.as_deref() == Some(file.path.as_str()) && !file.deleted {
            file.old_path = None;
        }
    }
    files
}

/// The name on a `---` / `+++` line without its prefix; `None` for `/dev/null`.
fn side(name: &str, prefix: &str) -> Option<String> {
    let name = name.strip_suffix('\t').unwrap_or(name);
    if name == "/dev/null" {
        return None;
    }
    let name = unquote_path(name);
    Some(name.strip_prefix(prefix).unwrap_or(&name).to_owned())
}

/// The new name in `a/x b/x` (a guess when it has spaces: the `+++` line says it surely).
fn header_path(rest: &str) -> String {
    let rest = unquote_path(rest);
    match rest.rfind(" b/") {
        Some(i) => rest[i + 3..].to_owned(),
        None => rest,
    }
}

/// `-a,b +c,d @@…` → the hunk, a missing count being 1.
fn hunk_header(header: &str) -> Option<Hunk> {
    let rest = header.strip_prefix('-')?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let ((old_start, old_count), (new_start, new_count)) = (range(old)?, range(new)?);
    Some(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunks_are_read_file_by_file_with_renames_additions_and_deletions() {
        let out = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n\
                   +++ b/src/a.rs\n@@ -3 +3,2 @@ fn x\n-old\n+new\n+++ b/evil\n@@ -9,2 +10,0 @@\n\
                   -a\n--- a/fake\n\
                   diff --git a/old name.rs b/new name.rs\nsimilarity index 90%\n\
                   rename from old name.rs\nrename to new name.rs\n--- a/old name.rs\t\n\
                   +++ b/new name.rs\t\n@@ -1,0 +2 @@\n+x\n\
                   diff --git a/gone.rs b/gone.rs\ndeleted file mode 100644\n--- a/gone.rs\n\
                   +++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n\
                   diff --git a/new.rs b/new.rs\nnew file mode 100644\n--- /dev/null\n\
                   +++ b/new.rs\n@@ -0,0 +1 @@\n+a\n\\ No newline at end of file\n\
                   diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = parse(out);
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].old_path, None);
        assert_eq!(
            files[0].hunks,
            [
                Hunk {
                    old_start: 3,
                    old_count: 1,
                    new_start: 3,
                    new_count: 2
                },
                Hunk {
                    old_start: 9,
                    old_count: 2,
                    new_start: 10,
                    new_count: 0
                }
            ]
        );
        assert_eq!(files[0].hunks[0].new_lines(), Some((3, 4)));
        assert_eq!(files[0].hunks[1].new_lines(), None);
        assert_eq!(files[0].hunks[1].old_lines(), Some((9, 10)));
        assert_eq!(files[1].path, "new name.rs");
        assert_eq!(files[1].old_path.as_deref(), Some("old name.rs"));
        assert!(files[2].deleted);
        assert_eq!(files[2].path, "gone.rs");
        assert!(files[3].added);
        assert_eq!(files[3].hunks.len(), 1);
        assert!(files[4].binary);
        assert!(files[4].hunks.is_empty());
    }

    #[test]
    fn a_real_diff_and_the_working_tree_are_read() -> Result<(), String> {
        let repo = crate::git::fixture::Repo::new("changed-lines")?;
        let one = repo.commit("one", &[("a.rs", Some("1\n2\n3\n"))])?;
        let two = repo.commit("two", &[("a.rs", Some("1\nb\n3\n4\n"))])?;
        let files = read(repo.path(), &one, Some(&two), &["*.rs"])?;
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0]
                .hunks
                .iter()
                .filter_map(Hunk::new_lines)
                .collect::<Vec<_>>(),
            [(2, 2), (4, 4)]
        );
        std::fs::write(repo.path().join("a.rs"), "1\nb\n3\n4\n5\n").map_err(|e| e.to_string())?;
        let work = read(repo.path(), &two, None, &[])?;
        assert_eq!(work[0].hunks[0].new_lines(), Some((5, 5)));
        Ok(())
    }
}
