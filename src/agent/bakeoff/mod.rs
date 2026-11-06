//! Bake-off: 2–4 attempts at the same task (worktrees or branches) side by side. Each is taken
//! as a tree (a worktree's uncommitted changes included, through a temporary index, nothing of
//! its own touched) and compared with the commit they all start from: the files each touched,
//! its size, the dependencies it adds and whether it builds. _Assemble_ makes a branch from
//! whole files and single hunks of any of them.

pub(crate) mod assemble;
pub(crate) mod build;
pub(crate) mod deps;
pub(crate) mod hunks;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::git::{self, ChangedFile};

/// How many attempts can be compared.
pub(crate) const MIN: usize = 2;
pub(crate) const MAX: usize = 4;

/// One attempt at the task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Attempt {
    /// What it is called in the grid: its branch, or its worktree's folder.
    pub name: String,
    /// The branch (or commit) it is on.
    pub rev: String,
    /// Its worktree, when it has one: what is compared is then its files as they are now.
    pub worktree: Option<PathBuf>,
}

/// An attempt as compared: its tree and how it differs from the base.
#[derive(Clone, Debug, Default)]
pub(crate) struct Compared {
    pub tree: String,
    pub files: Vec<ChangedFile>,
    pub added: u32,
    pub removed: u32,
    /// Dependencies its manifests gain, as `file: name`.
    pub new_deps: Vec<String>,
    /// It has uncommitted changes, compared too.
    pub uncommitted: bool,
    /// It doesn't fork from the others at the base (it started from another commit).
    pub forked_elsewhere: bool,
}

/// What comparing the attempts found.
#[derive(Clone, Debug, Default)]
pub(crate) struct Comparison {
    /// The commit they all start from.
    pub base: String,
    /// Whether each attempt's commit has `base` as its own fork point with the others (`false`
    /// means one started elsewhere, and the comparison says more than that one did).
    pub same_base: bool,
    pub attempts: Vec<Compared>,
}

impl Comparison {
    /// Every file any attempt touched, sorted, with which attempts touched it.
    pub fn files(&self) -> Vec<(String, Vec<bool>)> {
        let mut paths: Vec<&str> = self
            .attempts
            .iter()
            .flat_map(|a| a.files.iter().map(|f| f.path.as_str()))
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
            .into_iter()
            .map(|path| {
                let touched = self
                    .attempts
                    .iter()
                    .map(|a| a.files.iter().any(|f| f.path == path))
                    .collect();
                (path.to_owned(), touched)
            })
            .collect()
    }
}

/// Compares `attempts` (in the repository of `repo`). Runs git: never on the UI thread.
pub(crate) fn compare(repo: &Path, attempts: &[Attempt]) -> Result<Comparison, String> {
    if attempts.len() < MIN || attempts.len() > MAX {
        return Err(format!(
            "Pick {MIN} to {MAX} attempts to compare ({} picked)",
            attempts.len()
        ));
    }
    let commits = attempts
        .iter()
        .map(|a| commit_of(repo, &a.rev))
        .collect::<Result<Vec<_>, _>>()?;
    let mut args = vec!["merge-base", "--octopus"];
    args.extend(commits.iter().map(String::as_str));
    let base = git::run(Some(repo), &args)
        .map_err(|_| {
            "These attempts share no history: there is no base to compare them with".to_owned()
        })?
        .trim()
        .to_owned();
    // Started together: every two of them fork at the base. One whose fork with any other is
    // later (it started from a newer commit) shows more than it did.
    let mut elsewhere = vec![false; commits.len()];
    for i in 0..commits.len() {
        for j in i + 1..commits.len() {
            let fork = git::run(Some(repo), &["merge-base", &commits[i], &commits[j]]);
            if !fork.is_ok_and(|b| b.trim() == base) {
                elsewhere[i] = true;
                elsewhere[j] = true;
            }
        }
    }
    // One that started from a newer commit of another branch (`main` moved on): some of its
    // commits since the base are on a branch that is none of the attempts.
    let excludes: Vec<String> = attempts
        .iter()
        .map(|a| format!("--exclude={}", a.rev))
        .collect();
    for (i, commit) in commits.iter().enumerate() {
        let count = |extra: &[&str]| {
            let mut args = vec![
                "rev-list",
                "--count",
                commit.as_str(),
                "--not",
                base.as_str(),
            ];
            args.extend_from_slice(extra);
            git::run(Some(repo), &args).map(|n| n.trim().to_owned())
        };
        let own: Vec<&str> = excludes
            .iter()
            .map(String::as_str)
            .chain(["--branches"])
            .collect();
        if let (Ok(all), Ok(only)) = (count(&[]), count(&own))
            && all != only
        {
            elsewhere[i] = true;
        }
    }
    let same_base = !elsewhere.contains(&true);
    let mut compared = Vec::new();
    for ((attempt, commit), elsewhere) in attempts.iter().zip(&commits).zip(elsewhere) {
        let mut one = compare_one(repo, attempt, commit, &base)?;
        one.forked_elsewhere = elsewhere;
        compared.push(one);
    }
    Ok(Comparison {
        base,
        same_base,
        attempts: compared,
    })
}

fn commit_of(repo: &Path, rev: &str) -> Result<String, String> {
    if rev.starts_with('-') {
        return Err(format!("{rev} is not a branch"));
    }
    git::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| format!("{rev} isn't a commit of this repository"))
}

fn compare_one(
    repo: &Path,
    attempt: &Attempt,
    commit: &str,
    base: &str,
) -> Result<Compared, String> {
    let committed = tree_of(repo, commit)?;
    let tree = match &attempt.worktree {
        Some(wt) => {
            crate::agent::snapshot::take(wt).map_err(|e| format!("{}: {e}", attempt.name))?
        }
        None => committed.clone(),
    };
    let files = changed_files(repo, base, &tree)?;
    let added = files.iter().filter_map(|f| f.added).sum();
    let removed = files.iter().filter_map(|f| f.removed).sum();
    let mut new_deps = Vec::new();
    for file in files.iter().filter(|f| deps::is_manifest(&f.path)) {
        let patch = diff(repo, &["-U0", base, &tree, "--", &file.path])?;
        for name in deps::added(&file.path, &patch) {
            new_deps.push(format!("{}: {name}", file.path));
        }
    }
    Ok(Compared {
        uncommitted: tree != committed,
        tree,
        files,
        added,
        removed,
        new_deps,
        forked_elsewhere: false,
    })
}

/// `git diff` of the bake-off: paths taken as they are (`:x`, `*.rs` are names), no renames
/// (a file moved is the old one deleted and the new one added, which is what assembling
/// takes), no hunks joined across unchanged lines, no textconv or external diff (the hunks go
/// back into files as they are).
pub(crate) fn diff(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec![
        "--literal-pathspecs",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--inter-hunk-context=0",
    ];
    all.extend_from_slice(args);
    git::run(Some(repo), &all)
}

/// The files that differ from `base` to `tree`, with their line counts.
fn changed_files(repo: &Path, base: &str, tree: &str) -> Result<Vec<ChangedFile>, String> {
    let status = diff(repo, &["--name-status", "-z", base, tree, "--"])?;
    let counts = diff(repo, &["--numstat", "-z", base, tree, "--"])?;
    let mut files = Vec::new();
    let mut words = status.split('\0').filter(|w| !w.is_empty());
    while let (Some(kind), Some(path)) = (words.next(), words.next()) {
        files.push(ChangedFile {
            status: kind.chars().next().unwrap_or('M'),
            path: path.to_owned(),
            old_path: None,
            added: None,
            removed: None,
        });
    }
    // `added\tremoved\tpath\0`; `-` for a binary file.
    for record in counts.split('\0').filter(|r| !r.is_empty()) {
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if let Some(file) = files.iter_mut().find(|f| f.path == path) {
            file.added = added.parse().ok();
            file.removed = removed.parse().ok();
        }
    }
    Ok(files)
}

pub(crate) fn tree_of(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| format!("{rev} has no tree"))
}

/// A file as it is in `tree`, `None` when it isn't there.
pub(crate) fn file_in(repo: &Path, tree: &str, path: &str) -> Result<Option<Vec<u8>>, String> {
    let spec = format!("{tree}:{path}");
    match git::run(Some(repo), &["cat-file", "-e", &spec]) {
        Ok(_) => git::run_bytes(Some(repo), &["cat-file", "blob", &spec]).map(Some),
        Err(_) => Ok(None),
    }
}

/// The patch of `path` from the base to an attempt's tree.
pub(crate) fn file_patch(
    repo: &Path,
    base: &str,
    tree: &str,
    path: &str,
) -> Result<String, String> {
    diff(repo, &[base, tree, "--", path])
}
