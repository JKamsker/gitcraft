//! Tracked files missing from Changes: the paths marked `assume-unchanged` or `skip-worktree`
//! (`git ls-files -v`), told apart from those sparse-checkout leaves out, which git marks the
//! same way.

use std::path::Path;

use crate::git::{LITERAL_PATHS, run};

/// Why git doesn't look at a tracked file's changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    /// `update-index --assume-unchanged`: a promise that the file doesn't change, for speed.
    AssumeUnchanged,
    /// `update-index --skip-worktree`: keep the working tree's version, whatever comes in.
    SkipWorktree,
    /// Both of them.
    Both,
    /// assume-unchanged on a file sparse-checkout leaves out: only the assume-unchanged half
    /// is anyone's to clear.
    AssumeOnSparse,
    /// Left out by sparse-checkout, which marks it skip-worktree: not a manual flag.
    Sparse,
}

impl Flag {
    pub fn label(self) -> &'static str {
        match self {
            Flag::AssumeUnchanged => "assume-unchanged",
            Flag::SkipWorktree => "skip-worktree",
            Flag::Both => "assume-unchanged, skip-worktree",
            Flag::AssumeOnSparse => "assume-unchanged (sparse-checkout)",
            Flag::Sparse => "sparse-checkout",
        }
    }

    /// Whether clearing `option` (`--no-assume-unchanged`, `--no-skip-worktree`) is for it.
    /// `update-index` takes one of the two per run, so a file with both needs two.
    fn cleared_by(self, option: &str) -> bool {
        match self {
            Flag::AssumeUnchanged | Flag::AssumeOnSparse => option == ASSUME,
            Flag::SkipWorktree => option == SKIP,
            Flag::Both => true,
            Flag::Sparse => false,
        }
    }
}

const ASSUME: &str = "--no-assume-unchanged";
const SKIP: &str = "--no-skip-worktree";

/// A tracked file git doesn't look at for changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flagged {
    pub path: String,
    pub flag: Flag,
}

/// How sparse-checkout is set up, which decides which skip-worktree paths are its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Sparse {
    #[default]
    Off,
    /// Cone mode: the folders checked out.
    Cone(Vec<String>),
    /// Patterns: which paths they leave out isn't worked out here; every skip-worktree path
    /// is taken as theirs.
    Patterns,
}

/// The flagged files of `repo`, and how sparse-checkout is set up.
pub fn flagged(repo: &Path) -> Result<(Vec<Flagged>, Sparse), String> {
    let listed = run(Some(repo), &["ls-files", "-v", "-z"])
        .map_err(|e| format!("Could not list the tracked files: {e}"))?;
    let sparse = sparse(repo);
    let marked = parse_ls_files_v(&listed);
    let list = marked
        .into_iter()
        .map(|(path, flag)| {
            let flag = match flag {
                Flag::SkipWorktree if sparse_managed(&path, &sparse) => Flag::Sparse,
                Flag::Both if sparse_managed(&path, &sparse) => Flag::AssumeOnSparse,
                other => other,
            };
            Flagged { path, flag }
        })
        .collect();
    Ok((list, sparse))
}

/// Reads `git ls-files -v -z`: a tag, a space and the path per entry. A lowercase tag is
/// assume-unchanged, `S` (or `s`) skip-worktree; other entries are left out.
pub fn parse_ls_files_v(out: &str) -> Vec<(String, Flag)> {
    out.split('\0')
        .filter_map(|entry| {
            let (tag, path) = entry.split_once(' ')?;
            let tag = tag.chars().next()?;
            let flag = match tag {
                's' => Flag::Both,
                'S' => Flag::SkipWorktree,
                t if t.is_ascii_lowercase() => Flag::AssumeUnchanged,
                _ => return None,
            };
            (!path.is_empty()).then(|| (path.to_owned(), flag))
        })
        .collect()
}

fn sparse(repo: &Path) -> Sparse {
    let on = run(Some(repo), &["config", "--bool", "core.sparseCheckout"])
        .is_ok_and(|v| v.trim() == "true");
    if !on {
        return Sparse::Off;
    }
    let cone = run(Some(repo), &["config", "--bool", "core.sparseCheckoutCone"])
        .is_ok_and(|v| v.trim() == "true");
    if !cone {
        return Sparse::Patterns;
    }
    match run(Some(repo), &["sparse-checkout", "list"]) {
        Ok(out) => Sparse::Cone(
            out.lines()
                .map(|l| unquote(l).trim_end_matches('/').to_owned())
                .filter(|l| !l.is_empty())
                .collect(),
        ),
        Err(_) => Sparse::Patterns,
    }
}

/// Whether sparse-checkout is what leaves `path` out.
fn sparse_managed(path: &str, sparse: &Sparse) -> bool {
    match sparse {
        Sparse::Off => false,
        Sparse::Cone(dirs) => !in_cone(path, dirs),
        // Patterns can't be told apart from manual flags without matching every path against
        // them: every skip-worktree path counts as sparse-checkout's, so none is cleared by
        // mistake (which would show the files it left out as deleted).
        Sparse::Patterns => true,
    }
}

/// Whether cone-mode sparse-checkout with the folders `dirs` checks `path` out: files at the
/// top, files directly in a folder above one of `dirs`, and everything inside them.
pub fn in_cone(path: &str, dirs: &[String]) -> bool {
    let parent = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    if parent.is_empty() {
        return true;
    }
    dirs.iter().any(|dir| {
        path.starts_with(&format!("{dir}/"))
            || dir == parent
            || dir.starts_with(&format!("{parent}/"))
    })
}

/// A path as git prints it in a list: as it is, or C-quoted (`"a b/\303\251"`) when it holds
/// spaces, quotes or letters outside ASCII.
pub fn unquote(printed: &str) -> String {
    let Some(inner) = printed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return printed.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'a') => bytes.push(0x07),
            Some(b'b') => bytes.push(0x08),
            Some(b'f') => bytes.push(0x0c),
            Some(b'v') => bytes.push(0x0b),
            Some(d @ b'0'..=b'7') => {
                let mut value = u32::from(d - b'0');
                for _ in 0..2 {
                    match chars.peek() {
                        Some(&o @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(o - b'0');
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push((value & 0xff) as u8);
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether `path` can be named to git as read: a name that isn't UTF-8 was read with
/// replacement characters, and naming it so would fail the whole batch.
pub fn clearable(path: &str) -> bool {
    !path.contains('\u{FFFD}')
}

/// Longest command line the paths of one `update-index` go into, well under Windows' limit.
const MAX_ARGS_LEN: usize = 8000;

/// The `git update-index` commands that clear the flags of `files`, in batches that fit a
/// command line. Sparse-checkout's are left alone.
pub fn clear_ops(files: &[Flagged]) -> Vec<Vec<String>> {
    let mut ops = Vec::new();
    for option in [ASSUME, SKIP] {
        let mut op: Vec<String> = Vec::new();
        let mut len = 0;
        let start = |op: &mut Vec<String>| {
            op.extend([LITERAL_PATHS, "update-index", option, "--"].map(str::to_owned));
        };
        for file in files
            .iter()
            .filter(|f| f.flag.cleared_by(option) && clearable(&f.path))
        {
            if op.is_empty() {
                start(&mut op);
            } else if len + file.path.len() > MAX_ARGS_LEN {
                ops.push(std::mem::take(&mut op));
                len = 0;
                start(&mut op);
            }
            len += file.path.len() + 1;
            op.push(file.path.clone());
        }
        if !op.is_empty() {
            ops.push(op);
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_tags() {
        let out = "H a.txt\0h b c.txt\0S d/é.txt\0s e.txt\0M f.txt\0H \0";
        assert_eq!(
            parse_ls_files_v(out),
            [
                ("b c.txt".to_owned(), Flag::AssumeUnchanged),
                ("d/é.txt".to_owned(), Flag::SkipWorktree),
                ("e.txt".to_owned(), Flag::Both),
            ]
        );
        assert!(parse_ls_files_v("").is_empty());
    }

    #[test]
    fn quoted_folder_names_are_read() {
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("\"sub dir/\\303\\251\""), "sub dir/é");
        assert_eq!(unquote("\"a\\\"b\\\\c\\td\""), "a\"b\\c\td");
        assert_eq!(unquote("\""), "\"");
    }

    #[test]
    fn cone_mode_keeps_the_top_and_the_folders_above() {
        let dirs = ["src/app".to_owned()];
        assert!(in_cone("README.md", &dirs));
        assert!(in_cone("src/main.rs", &dirs));
        assert!(in_cone("src/app/deep/x.rs", &dirs));
        assert!(!in_cone("src/other/x.rs", &dirs));
        assert!(!in_cone("docs/a.md", &dirs));
        // A folder whose name only starts the same isn't inside.
        assert!(!in_cone("src/apple/x.rs", &dirs));
    }

    #[test]
    fn clearing_goes_in_batches_and_leaves_sparse_alone() {
        let files: Vec<Flagged> = (0..2000)
            .map(|i| Flagged {
                path: format!("folder/file {i}.txt"),
                flag: Flag::AssumeUnchanged,
            })
            .chain([
                Flagged {
                    path: "s.txt".into(),
                    flag: Flag::Sparse,
                },
                Flagged {
                    path: "bad \u{FFFD}.txt".into(),
                    flag: Flag::AssumeUnchanged,
                },
                Flagged {
                    path: "on-sparse.txt".into(),
                    flag: Flag::AssumeOnSparse,
                },
                Flagged {
                    path: "b.txt".into(),
                    flag: Flag::Both,
                },
            ])
            .collect();
        let ops = clear_ops(&files);
        assert!(ops.len() > 2);
        let paths: usize = ops
            .iter()
            .map(|op| op.iter().skip_while(|a| *a != "--").count() - 1)
            .sum();
        // The file with both flags is cleared twice, once for each; the one sparse-checkout
        // leaves out only of assume-unchanged; the unreadable name not at all.
        assert_eq!(paths, 2003);
        assert!(!ops.iter().flatten().any(|a| a.contains('\u{FFFD}')));
        let on_sparse: Vec<_> = ops
            .iter()
            .filter(|op| op.iter().any(|a| a == "on-sparse.txt"))
            .collect();
        assert_eq!(on_sparse.len(), 1);
        assert!(on_sparse[0].iter().any(|a| a == "--no-assume-unchanged"));
        assert!(ops.iter().all(|op| op.join(" ").len() < MAX_ARGS_LEN + 200));
        assert!(!ops.iter().flatten().any(|a| a == "s.txt"));
        let last = ops.last().map(|op| op.join(" ")).unwrap_or_default();
        assert!(
            last.ends_with("update-index --no-skip-worktree -- b.txt"),
            "{last}"
        );
        assert!(clear_ops(&[]).is_empty());
    }
}
