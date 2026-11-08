//! The scope drift meter: what a worktree changed, set against what it was asked to do (its
//! task, or its linked issue). Exact figures first: files and the folders they are in (those
//! the task doesn't mention apart), new dependencies, public signatures changed, the share of
//! the change that is formatting only. Matching with an LLM is optional (see [`llm`]).

pub mod display;

mod deps;
pub mod llm;
mod signatures;
pub mod task;

#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeMap;
use std::path::Path;

pub use signatures::Signature;

/// A folder the change touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    /// `/` between names; empty for the top of the repository.
    pub path: String,
    pub files: Vec<String>,
    /// The task talks about it (always, with no task to go by).
    pub related: bool,
}

/// A dependency a manifest gained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub manifest: String,
    pub name: String,
}

/// What a worktree changed since it left its base.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Where it left the base (the commit compared with), and its HEAD.
    pub fork: String,
    pub head: String,
    /// Every file changed: committed, uncommitted or new.
    pub files: Vec<String>,
    /// Those of them with changes not committed (or not tracked at all), which only the
    /// worktree's own working tree shows.
    pub uncommitted: Vec<String>,
    /// Those of them git doesn't track.
    pub untracked: Vec<String>,
    /// Those of them its commits changed.
    pub committed: Vec<String>,
    pub folders: Vec<Folder>,
    pub dependencies: Vec<Dependency>,
    pub signatures: Vec<Signature>,
    /// Lines changed in all, and those of them that change only whitespace.
    pub changed_lines: u64,
    pub formatting_lines: u64,
    /// Files whose every change is whitespace.
    pub formatting_files: Vec<String>,
    /// The task had words to match folders with.
    pub judged: bool,
}

impl Report {
    pub fn unrelated(&self) -> impl Iterator<Item = &Folder> {
        self.folders.iter().filter(|f| !f.related)
    }

    /// The share of changed lines that are formatting only, in percent.
    pub fn formatting_percent(&self) -> u64 {
        (self.formatting_lines * 100)
            .checked_div(self.changed_lines)
            .unwrap_or(0)
    }

    /// The figures in one line: _also touched 14 files in 5 unrelated folders, added 2
    /// dependencies, changed 3 public signatures, 40% formatting only_.
    pub fn headline(&self) -> String {
        let unrelated: Vec<&Folder> = self.unrelated().collect();
        let in_unrelated: usize = unrelated.iter().map(|f| f.files.len()).sum();
        let mut parts = vec![plural(self.files.len(), "file", "files") + " changed"];
        if self.judged && !unrelated.is_empty() {
            parts.push(format!(
                "{} in {} the task doesn't mention",
                plural(in_unrelated, "file", "files"),
                plural(unrelated.len(), "folder", "folders")
            ));
        }
        parts.push(format!(
            "added {}",
            plural(self.dependencies.len(), "dependency", "dependencies")
        ));
        parts.push(format!(
            "changed {}",
            plural(
                self.signatures.len(),
                "public signature",
                "public signatures"
            )
        ));
        parts.push(format!("{}% formatting only", self.formatting_percent()));
        parts.join(", ")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Most untracked files counted.
const MAX_UNTRACKED: usize = 2000;

/// What the worktree at `worktree` changed since it left `base` (the main worktree's
/// commit), set against `task`.
pub fn measure(worktree: &Path, base: &str, task: &str) -> Result<Report, String> {
    let git = |args: &[&str]| {
        crate::git::run(Some(worktree), args)
            .map_err(|e| format!("Could not measure the worktree's changes: {e}"))
    };
    // A bare layout's main entry has no commit: the default branch instead.
    let base = if base.is_empty() {
        default_branch(worktree)
            .ok_or("There is no main branch commit to compare the worktree with")?
    } else {
        base.to_owned()
    };
    if base.starts_with('-') {
        return Err(format!("{base} is not a revision"));
    }
    let fork = git(&["merge-base", "--end-of-options", &base, "HEAD"])?
        .trim()
        .to_owned();
    let head = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let q = ["-c", "core.quotePath=false"];
    let diff = |extra: &[&str]| -> Result<String, String> {
        let mut args: Vec<&str> = q.to_vec();
        // `a/` and `b/` whatever `diff.noprefix` / `diff.mnemonicPrefix` say.
        args.extend([
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ]);
        args.extend(extra);
        args.extend(["--end-of-options", fork.as_str(), "--"]);
        git(&args)
    };
    let mut files: Vec<String> = split_nul(&diff(&["--name-only", "-z"])?);
    let untracked = git(&[
        "-c",
        "core.quotePath=false",
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
    ])?;
    let untracked: Vec<String> = split_nul(&untracked)
        .into_iter()
        .take(MAX_UNTRACKED)
        .collect();
    files.extend(untracked.iter().cloned());
    files.sort();
    files.dedup();
    let mut uncommitted = split_nul(&git(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--name-only",
        "-z",
        "--no-renames",
        "HEAD",
        "--",
    ])?);
    uncommitted.extend(untracked.iter().cloned());
    uncommitted.sort();
    let committed = split_nul(&git(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--name-only",
        "-z",
        "--no-renames",
        "--end-of-options",
        &fork,
        "HEAD",
        "--",
    ])?);

    let all = numstat(&diff(&["--numstat", "-z"])?);
    let content = numstat(&diff(&["--numstat", "-z", "-w", "--ignore-blank-lines"])?);
    let changed_lines: u64 = all.values().sum();
    let content_lines: u64 = content.values().sum();
    let formatting_files: Vec<String> = all
        .iter()
        .filter(|(path, n)| **n > 0 && content.get(*path).copied().unwrap_or(0) == 0)
        .map(|(p, _)| p.clone())
        .collect();

    let signatures = signatures::changed(&diff(&["-U0"])?);
    let dependencies = new_dependencies(worktree, &fork, &files, &untracked);
    let keywords = task::keywords(task);
    let judged = !keywords.is_empty();
    let folders = folders(&files, &keywords, judged);
    Ok(Report {
        fork,
        head,
        files,
        uncommitted,
        untracked,
        committed,
        folders,
        dependencies,
        signatures,
        changed_lines,
        formatting_lines: changed_lines.saturating_sub(content_lines),
        formatting_files,
        judged,
    })
}

/// The remote's default branch, or `main` / `master`.
fn default_branch(worktree: &Path) -> Option<String> {
    [
        "refs/remotes/origin/HEAD",
        "refs/heads/main",
        "refs/heads/master",
    ]
    .iter()
    .find_map(|r| {
        crate::git::run(Some(worktree), &["rev-parse", "--verify", "--quiet", r])
            .ok()
            .map(|h| h.trim().to_owned())
            .filter(|h| !h.is_empty())
    })
}

fn split_nul(out: &str) -> Vec<String> {
    out.split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `--numstat -z` (no renames): lines added and removed by file; binary files count none.
fn numstat(out: &str) -> BTreeMap<String, u64> {
    split_nul(out)
        .iter()
        .filter_map(|entry| {
            let mut parts = entry.splitn(3, '\t');
            let added = parts.next()?.parse::<u64>().unwrap_or(0);
            let removed = parts.next()?.parse::<u64>().unwrap_or(0);
            Some((parts.next()?.to_owned(), added + removed))
        })
        .collect()
}

/// The files grouped by the folder they are in, each marked whether the task talks about it.
fn folders(files: &[String], keywords: &[String], judged: bool) -> Vec<Folder> {
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in files {
        let dir = f.rsplit_once('/').map(|(d, _)| d).unwrap_or_default();
        by_dir.entry(dir.to_owned()).or_default().push(f.clone());
    }
    by_dir
        .into_iter()
        .map(|(path, files)| {
            // A file's own name counts too: `login.rs` at the top is about login.
            let related = !judged
                || task::related(&path, keywords)
                || files.iter().any(|f| task::related(f, keywords));
            Folder {
                path,
                files,
                related,
            }
        })
        .collect()
}

/// The dependencies the changed manifests gained since `fork`.
fn new_dependencies(
    worktree: &Path,
    fork: &str,
    files: &[String],
    untracked: &[String],
) -> Vec<Dependency> {
    files
        .iter()
        .filter(|f| deps::is_manifest(f))
        .flat_map(|manifest| {
            let before = if untracked.contains(manifest) {
                String::new()
            } else {
                crate::git::run(Some(worktree), &["show", &format!("{fork}:{manifest}")])
                    .unwrap_or_default()
            };
            let after = read_file(worktree, manifest).unwrap_or_default();
            deps::added(manifest, &before, &after)
                .into_iter()
                .map(|name| Dependency {
                    manifest: manifest.clone(),
                    name,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A file of the worktree as it is now (on whichever machine it is).
fn read_file(worktree: &Path, rel: &str) -> Option<String> {
    const MAX: u64 = 4 << 20;
    let bytes = match crate::remote::read_within(worktree, rel, MAX) {
        Some(read) => read.ok()??,
        None => std::fs::read(worktree.join(rel)).ok()?,
    };
    String::from_utf8(bytes).ok()
}
