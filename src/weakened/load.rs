//! Reading the patch the checks look at, through the git command layer (so a repository in
//! WSL or over SSH is read there).

use std::path::Path;

use super::patch::{FilePatch, Hunk, Line, Mark};
use super::{FileFlags, scan_patch};

/// Which part of the uncommitted changes, as the changes pane lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    /// The index against HEAD (or against nothing before the first commit).
    Staged,
    /// The working tree against the index.
    Unstaged,
    /// Files git doesn't track yet (and doesn't ignore).
    Untracked,
}

/// What a list of changed files is of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// A commit, against its first parent.
    Commit(String),
    /// From one commit to another, or to the working tree (`to: None`).
    Range {
        from: String,
        to: Option<String>,
    },
    Work(Area),
}

/// The patch as git prints it the same way whatever the user's config says: no colours, no
/// external diff or text conversion, `a/` and `b/`, three lines of context.
const PATCH_ARGS: [&str; 7] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
    "-M",
];

/// Largest patch looked through, in bytes: past it, the scan says so rather than hold it.
const MAX_PATCH: usize = 64 << 20;

/// Most untracked files read, and the largest of them.
const MAX_UNTRACKED: usize = 200;
const MAX_UNTRACKED_BYTES: u64 = 512 << 10;

/// The arguments that print `scope`'s patch (`None` for the untracked files, read one by one).
fn args(scope: &Scope) -> Option<Vec<&str>> {
    // Unicode names as they are, not as `\303\251` (both are read, this is shorter).
    let mut args = vec!["-c", "core.quotePath=false"];
    match scope {
        Scope::Commit(hash) => {
            args.extend([
                "show",
                "--no-show-signature",
                "--format=",
                "--diff-merges=first-parent",
                "--root",
            ]);
            args.extend(PATCH_ARGS);
            args.extend(["--end-of-options", hash.as_str()]);
        }
        Scope::Range { from, to } => {
            args.push("diff");
            args.extend(PATCH_ARGS);
            args.extend(["--end-of-options", from.as_str()]);
            args.extend(to.as_deref());
        }
        Scope::Work(Area::Staged) => {
            args.extend(["diff", "--cached"]);
            args.extend(PATCH_ARGS);
        }
        Scope::Work(Area::Unstaged) => {
            args.push("diff");
            args.extend(PATCH_ARGS);
        }
        Scope::Work(Area::Untracked) => return None,
    }
    args.push("--");
    Some(args)
}

/// The files of `scope` that weaken the checks.
pub fn scan(repo: &Path, scope: &Scope) -> Result<Vec<FileFlags>, String> {
    let Some(args) = args(scope) else {
        return Ok(untracked(repo)?
            .iter()
            .filter_map(|file| {
                let flags = super::scan_file(file, &Default::default());
                (!flags.is_empty()).then(|| FileFlags {
                    path: file.path.clone(),
                    flags,
                })
            })
            .collect());
    };
    let text = crate::git::run(Some(repo), &args)
        .map_err(|e| format!("Could not read the changes to look for weakened checks: {e}"))?;
    if text.len() > MAX_PATCH {
        return Err(format!(
            "These changes are too large to look for weakened checks ({} MB)",
            text.len() >> 20
        ));
    }
    Ok(scan_patch(&text))
}

/// The untracked files, each as a patch adding all its lines (binary ones, those too large
/// and those past the first [`MAX_UNTRACKED`] left out).
fn untracked(repo: &Path) -> Result<Vec<FilePatch>, String> {
    let list = crate::git::run(
        Some(repo),
        &["ls-files", "--others", "--exclude-standard", "-z", "--"],
    )
    .map_err(|e| format!("Could not list the untracked files to look for weakened checks: {e}"))?;
    Ok(list
        .split('\0')
        .filter(|p| !p.is_empty())
        .take(MAX_UNTRACKED)
        .filter_map(|path| {
            let bytes = read_untracked(repo, path)?;
            let text = String::from_utf8(bytes).ok()?;
            Some(added_file(path, &text))
        })
        .collect())
}

/// The file `rel` of the working tree at `repo`, if it is a plain file small enough.
fn read_untracked(repo: &Path, rel: &str) -> Option<Vec<u8>> {
    let bytes = match crate::remote::read_within(repo, rel, MAX_UNTRACKED_BYTES) {
        Some(read) => read.ok()??,
        None => {
            let file = repo.join(rel);
            let meta = std::fs::symlink_metadata(&file).ok()?;
            if !meta.is_file() || meta.len() > MAX_UNTRACKED_BYTES {
                return None;
            }
            std::fs::read(file).ok()?
        }
    };
    (!bytes.contains(&0)).then_some(bytes)
}

/// `text` as a new file `path`: every line added.
fn added_file(path: &str, text: &str) -> FilePatch {
    let lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| Line {
            mark: Mark::Added,
            number: u32::try_from(i + 1).unwrap_or(u32::MAX),
            text: l.to_owned(),
        })
        .collect();
    FilePatch {
        path: path.to_owned(),
        deleted: false,
        added: true,
        hunks: vec![Hunk {
            old_start: 0,
            old_count: 0,
            lines,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Scope, args};

    #[test]
    fn revisions_come_after_the_end_of_options() {
        let scope = Scope::Range {
            from: "--output=x".into(),
            to: None,
        };
        let a = args(&scope).unwrap_or_default();
        let end = a.iter().position(|x| *x == "--end-of-options");
        let rev = a.iter().position(|x| *x == "--output=x");
        assert!(end.is_some() && end < rev);
        assert_eq!(a.last(), Some(&"--"));
        assert!(args(&Scope::Work(Area::Untracked)).is_none());
        let staged = args(&Scope::Work(Area::Staged)).unwrap_or_default();
        assert!(staged.contains(&"--cached") && staged.contains(&"--unified=3"));
    }
}
