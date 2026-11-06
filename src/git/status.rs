//! The working tree: which files changed, on which side of the index, and what git left half
//! done.

use std::path::Path;

use super::run;

/// Which side of the index a working-tree file is listed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Staged,
    Unstaged,
    Untracked,
    Conflicted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusFile {
    pub path: String,
    pub old_path: Option<String>,
    /// Porcelain status letter for this side (`M`, `A`, `D`, `R`, `?`, `U`…).
    pub code: char,
    pub area: Area,
}

/// An operation git left half done (conflicts to resolve).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InProgress {
    Merge,
    CherryPick,
    Revert,
    Rebase,
    /// `git am` applying patches.
    Am,
}

impl InProgress {
    pub fn label(self) -> &'static str {
        match self {
            InProgress::Merge => "Merge",
            InProgress::CherryPick => "Cherry-pick",
            InProgress::Revert => "Revert",
            InProgress::Rebase => "Rebase",
            InProgress::Am => "Patch",
        }
    }

    /// The git subcommand whose `--abort` cancels it.
    pub fn command(self) -> &'static str {
        match self {
            InProgress::Merge => "merge",
            InProgress::CherryPick => "cherry-pick",
            InProgress::Revert => "revert",
            InProgress::Rebase => "rebase",
            InProgress::Am => "am",
        }
    }

    /// Whether it finishes with `--continue` rather than with a commit.
    pub fn continues(self) -> bool {
        matches!(self, InProgress::Rebase | InProgress::Am)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorkStatus {
    pub staged: Vec<StatusFile>,
    pub unstaged: Vec<StatusFile>,
    pub conflicted: Vec<StatusFile>,
    pub in_progress: Option<InProgress>,
    /// Message git prepared for the pending merge / cherry-pick / revert commit.
    pub prepared_message: Option<String>,
}

impl WorkStatus {
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.conflicted.is_empty()
    }

    /// How many files have uncommitted changes; one both staged and changed again counts once.
    pub fn changed_files(&self) -> usize {
        let mut paths: Vec<&str> = (self.staged.iter())
            .chain(&self.unstaged)
            .chain(&self.conflicted)
            .map(|f| f.path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths.len()
    }
}

pub fn status(repo: &Path) -> Result<WorkStatus, String> {
    let out = run(
        Some(repo),
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut status = parse_status(&out);
    status.in_progress = in_progress(repo)?;
    if status.in_progress.is_some() {
        status.prepared_message = prepared_message(repo);
    }
    Ok(status)
}

/// The operation git left half done in `repo`, if any.
/// Fails when that couldn't be asked (a repository on another machine not reached): no
/// operation in progress would be an answer, and a wrong one.
fn in_progress(repo: &Path) -> Result<Option<InProgress>, String> {
    if let Some(target) = crate::remote::parse(repo) {
        return in_progress_there(&target);
    }
    let asked = |e: std::io::Error| format!("could not read the repository's state: {e}");
    for dir in ["rebase-merge", "rebase-apply"] {
        let Ok(path) = run(Some(repo), &["rev-parse", "--git-path", dir]) else {
            continue;
        };
        let state = crate::remote::resolve(repo, path.trim());
        if crate::remote::try_is_dir(&state).map_err(asked)? {
            // `git am` keeps its state there too, marked by this file.
            let am = crate::remote::try_is_file(&state.join("applying")).map_err(asked)?;
            return Ok(Some(if am {
                InProgress::Am
            } else {
                InProgress::Rebase
            }));
        }
    }
    Ok([
        (InProgress::Merge, "MERGE_HEAD"),
        (InProgress::CherryPick, "CHERRY_PICK_HEAD"),
        (InProgress::Revert, "REVERT_HEAD"),
    ]
    .into_iter()
    .find(|(_, head)| run(Some(repo), &["rev-parse", "-q", "--verify", head]).is_ok())
    .map(|(state, _)| state))
}

/// [`in_progress`] for a repository on another machine: asked there in one go, not in six
/// round trips on every read of the status.
fn in_progress_there(target: &crate::remote::Target) -> Result<Option<InProgress>, String> {
    const SCRIPT: &str = concat!(
        "g=$(git rev-parse --absolute-git-dir) || exit 1
",
        "for d in rebase-merge rebase-apply; do
",
        "  if [ -d \"$g/$d\" ]; then
",
        "    if [ -f \"$g/$d/applying\" ]; then echo am; else echo rebase; fi; exit 0
",
        "  fi
",
        "done
",
        "for h in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
",
        "  git rev-parse -q --verify \"$h\" >/dev/null && { echo \"$h\"; exit 0; }
",
        "done
",
        "echo none
",
    );
    let out = crate::remote::exec(
        &target.host,
        &crate::remote::Request {
            cwd: Some(target.path.clone()),
            script: SCRIPT.to_owned(),
            ..crate::remote::Request::default()
        },
    )?;
    let said = String::from_utf8_lossy(&out.stdout);
    match (out.code, said.trim()) {
        (0, "am") => Ok(Some(InProgress::Am)),
        (0, "rebase") => Ok(Some(InProgress::Rebase)),
        (0, "MERGE_HEAD") => Ok(Some(InProgress::Merge)),
        (0, "CHERRY_PICK_HEAD") => Ok(Some(InProgress::CherryPick)),
        (0, "REVERT_HEAD") => Ok(Some(InProgress::Revert)),
        (0, "none") => Ok(None),
        _ => Err(format!(
            "could not read the repository's state: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The message git prepared for the commit that ends the operation in progress.
fn prepared_message(repo: &Path) -> Option<String> {
    let path = run(Some(repo), &["rev-parse", "--git-path", "MERGE_MSG"]).ok()?;
    let message = crate::remote::read_to_string(&crate::remote::resolve(repo, path.trim())).ok()?;
    // Drop git's `#` comment lines.
    Some(
        message
            .lines()
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned(),
    )
}

/// `status --porcelain=v1 -z`: `XY path\0`, renames and copies `XY new\0old\0`.
pub(super) fn parse_status(out: &str) -> WorkStatus {
    let mut status = WorkStatus::default();
    let mut it = out.split('\0');
    while let Some(entry) = it.next() {
        let mut chars = entry.chars();
        let (Some(x), Some(y), Some(' ')) = (chars.next(), chars.next(), chars.next()) else {
            continue;
        };
        let path = chars.as_str().to_owned();
        // A rename in the working tree (of a file added with `--intent-to-add`) is `Y`'s.
        let old_path = if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            it.next().map(str::to_owned)
        } else {
            None
        };
        let file = |code, area| StatusFile {
            path: path.clone(),
            old_path: old_path.clone(),
            code,
            area,
        };
        let conflicted = x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D');
        if conflicted {
            status.conflicted.push(file('U', Area::Conflicted));
        } else if x == '?' {
            status.unstaged.push(file('?', Area::Untracked));
        } else if x == '!' {
            continue;
        } else {
            if x != ' ' {
                status.staged.push(file(x, Area::Staged));
            }
            if y != ' ' {
                status.unstaged.push(file(y, Area::Unstaged));
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_sides_renames_and_conflicts() {
        let s =
            parse_status("MM both.rs\0R  new.rs\0old.rs\0?? fresh.txt\0UU clash.rs\0 D gone.rs\0");
        let staged: Vec<_> = s.staged.iter().map(|f| (f.code, f.path.as_str())).collect();
        assert_eq!(staged, vec![('M', "both.rs"), ('R', "new.rs")]);
        assert_eq!(s.staged[1].old_path.as_deref(), Some("old.rs"));
        let unstaged: Vec<_> = s
            .unstaged
            .iter()
            .map(|f| (f.code, f.path.as_str()))
            .collect();
        assert_eq!(
            unstaged,
            vec![('M', "both.rs"), ('?', "fresh.txt"), ('D', "gone.rs")]
        );
        assert_eq!(s.conflicted.len(), 1);
    }

    /// A file added with `--intent-to-add` under a new name is renamed on the working tree's
    /// side: its old name follows, and isn't an entry of its own.
    #[test]
    fn status_rename_in_the_working_tree() {
        let s = parse_status(" R new.txt\0ab c\0M  other.rs\0");
        let unstaged: Vec<_> = s
            .unstaged
            .iter()
            .map(|f| (f.code, f.path.as_str(), f.old_path.as_deref()))
            .collect();
        assert_eq!(unstaged, vec![('R', "new.txt", Some("ab c"))]);
        let staged: Vec<_> = s.staged.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(staged, vec!["other.rs"]);
    }

    #[test]
    fn a_file_staged_and_changed_again_counts_once() {
        let out = " M a.txt\0MM b.txt\0?? new.txt\0R  c.txt\0old-c.txt\0UU d.txt\0";
        assert_eq!(parse_status(out).changed_files(), 5);
        assert_eq!(WorkStatus::default().changed_files(), 0);
    }
}
