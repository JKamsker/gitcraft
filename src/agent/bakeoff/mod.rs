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
    // Each attempt's own fork point with the first one: all the same when they started together.
    let same_base = commits.iter().skip(1).all(|c| {
        git::run(Some(repo), &["merge-base", &commits[0], c]).is_ok_and(|b| b.trim() == base)
    });
    let mut compared = Vec::new();
    for (attempt, commit) in attempts.iter().zip(&commits) {
        compared.push(compare_one(repo, attempt, commit, &base)?);
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
        Some(wt) => snapshot(wt).map_err(|e| format!("{}: {e}", attempt.name))?,
        None => committed.clone(),
    };
    let files = git::diff_files(repo, base, Some(&tree))?;
    let added = files.iter().filter_map(|f| f.added).sum();
    let removed = files.iter().filter_map(|f| f.removed).sum();
    let mut new_deps = Vec::new();
    for file in files.iter().filter(|f| deps::is_manifest(&f.path)) {
        let patch = git::run(
            Some(repo),
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "-U0",
                base,
                &tree,
                "--",
                &file.path,
            ],
        )?;
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
    })
}

pub(crate) fn tree_of(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| format!("{rev} has no tree"))
}

/// The tree of the worktree at `wt` as it is now, untracked files git doesn't ignore included,
/// written through an index of gitcraft's own: the worktree's own index and files stay as they
/// are.
pub(crate) fn snapshot(wt: &Path) -> Result<String, String> {
    let index = git::run(
        Some(wt),
        &["rev-parse", "--git-path", "gitcraft-snapshot.index"],
    )?;
    let index = index.trim_end_matches(['\n', '\r']).to_owned();
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let step = |args: &[&str]| git::run_env(Some(wt), args, &env);
    step(&["read-tree", "HEAD"])?;
    step(&["add", "-A", "--", "."])?;
    let tree = step(&["write-tree"])?.trim().to_owned();
    // Not left behind where it is on this machine (elsewhere it is overwritten the next time).
    if !crate::remote::is_remote(wt) {
        let _ = std::fs::remove_file(crate::remote::join(wt, &index));
    }
    Ok(tree)
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
    git::run(
        Some(repo),
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            base,
            tree,
            "--",
            path,
        ],
    )
}
