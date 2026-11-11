//! _Assemble_: a new branch made from the base and what is picked of the attempts (a file
//! whole, or some of its hunks), written through an index of gitcraft's own: no checkout, no
//! worktree touched.

use std::path::Path;

use super::hunks::{self, Hunk};
use crate::git;

/// What the assembled branch takes of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Take {
    /// The file as attempt `n` has it (gone, if that attempt deleted it).
    File(usize),
    /// The base's file with these hunks: (attempt, its hunk's place among that attempt's hunks
    /// of the file, see [`file_hunks`]).
    Hunks(Vec<(usize, usize)>),
}

/// What to assemble.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub base: String,
    /// Each attempt's tree, in the order the picks number them.
    pub trees: Vec<String>,
    /// Their names, for the commit message.
    pub names: Vec<String>,
    pub picks: Vec<(String, Take)>,
    pub branch: String,
}

impl Plan {
    /// The commit message: which attempts each file came from.
    fn message(&self) -> String {
        let mut msg = format!("Assemble {} from the bake-off\n\n", self.branch);
        for (path, take) in &self.picks {
            let from = match take {
                Take::File(n) => format!("whole file from {}", self.name(*n)),
                Take::Hunks(list) => {
                    let mut names: Vec<&str> = list.iter().map(|(n, _)| self.name(*n)).collect();
                    names.dedup();
                    format!("{} hunk(s) from {}", list.len(), names.join(", "))
                }
            };
            msg.push_str(&format!("- {path}: {from}\n"));
        }
        msg
    }

    fn name(&self, n: usize) -> &str {
        self.names.get(n).map_or("?", String::as_str)
    }
}

/// The hunks of attempt `tree`'s change to `path`, as [`Take::Hunks`] numbers them.
pub(crate) fn file_hunks(
    repo: &Path,
    base: &str,
    tree: &str,
    path: &str,
) -> Result<Vec<Hunk>, String> {
    Ok(hunks::parse(&super::diff(
        repo,
        &["-U0", base, tree, "--", path],
    )?))
}

/// Makes the branch of `plan` and returns its commit.
pub(crate) fn assemble(repo: &Path, plan: &Plan) -> Result<String, String> {
    let branch = plan.branch.trim();
    git::run(Some(repo), &["check-ref-format", "--branch", branch])
        .map_err(|_| format!("{branch} can't be a branch name"))?;
    if git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
    {
        return Err(format!(
            "A branch named {branch} exists already: pick another name"
        ));
    }
    if plan.picks.is_empty() {
        return Err("Pick at least one file or hunk to assemble".to_owned());
    }
    let index = crate::agent::snapshot::temporary_index(repo, "assemble")?;
    let result = write(repo, plan, branch, &index);
    crate::agent::snapshot::remove(repo, &index);
    result
}

fn write(repo: &Path, plan: &Plan, branch: &str, index: &str) -> Result<String, String> {
    let env = [("GIT_INDEX_FILE", index)];
    let step = |args: &[&str]| git::run_env(Some(repo), args, &env);
    step(&["read-tree", &plan.base])?;
    for (path, take) in &plan.picks {
        let entry = match take {
            Take::File(n) => {
                let tree = plan.trees.get(*n).ok_or("no such attempt")?;
                entry(repo, tree, path)?
            }
            Take::Hunks(list) => merged(repo, plan, path, list)?,
        };
        match entry {
            Some((mode, blob)) => step(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("{mode},{blob},{path}"),
            ])?,
            None => step(&["update-index", "--force-remove", "--", path])?,
        };
    }
    let tree = step(&["write-tree"])?.trim().to_owned();
    let commit = git::run(
        Some(repo),
        &[
            "commit-tree",
            &tree,
            "-p",
            &plan.base,
            "-m",
            &plan.message(),
        ],
    )?
    .trim()
    .to_owned();
    // The empty old value: made only if no such branch came meanwhile.
    git::run(
        Some(repo),
        &["update-ref", &format!("refs/heads/{branch}"), &commit, ""],
    )?;
    Ok(commit)
}

/// The mode and blob of `path` in `tree`, `None` when it isn't there.
fn entry(repo: &Path, tree: &str, path: &str) -> Result<Option<(String, String)>, String> {
    let out = git::run(
        Some(repo),
        &["--literal-pathspecs", "ls-tree", "-z", tree, "--", path],
    )?;
    let Some(line) = out
        .split('\0')
        .find(|l| l.split_once('\t').is_some_and(|(_, p)| p == path))
    else {
        return Ok(None);
    };
    let mut words = line.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some(mode), Some("blob"), Some(blob)) => Ok(Some((mode.to_owned(), blob.to_owned()))),
        _ => Err(format!("{path} isn't a file in the attempt")),
    }
}

/// The base's `path` with the hunks `list` applied, written as a blob: (mode, blob).
fn merged(
    repo: &Path,
    plan: &Plan,
    path: &str,
    list: &[(usize, usize)],
) -> Result<Option<(String, String)>, String> {
    let base_entry = entry(repo, &plan.base, path)?;
    // Only text files go hunk by hunk: a link or a submodule is taken whole.
    let not_text = |mode: &str| mode == "120000" || mode == "160000";
    let attempt_entry = |n: usize| -> Result<Option<(String, String)>, String> {
        let tree = plan.trees.get(n).ok_or("no such attempt")?;
        entry(repo, tree, path)
    };
    let first = list.first().map_or(0, |(n, _)| *n);
    let first_entry = attempt_entry(first)?;
    if [&base_entry, &first_entry]
        .iter()
        .any(|e| e.as_ref().is_some_and(|(mode, _)| not_text(mode)))
    {
        return Err(format!(
            "{path} is a link or a submodule in an attempt or the base: take it whole"
        ));
    }
    let base_text = match &base_entry {
        Some(_) => {
            let bytes = super::file_in(repo, &plan.base, path)?.unwrap_or_default();
            String::from_utf8(bytes)
                .map_err(|_| format!("{path} isn't text: take it whole from one attempt"))?
        }
        None => String::new(),
    };
    let mut by_attempt: Vec<(usize, Vec<Hunk>)> = Vec::new();
    for (n, _) in list {
        if by_attempt.iter().all(|(m, _)| m != n) {
            let tree = plan.trees.get(*n).ok_or("no such attempt")?;
            by_attempt.push((*n, file_hunks(repo, &plan.base, tree, path)?));
        }
    }
    let mut picked: Vec<&Hunk> = Vec::new();
    for (n, i) in list {
        let hunk = by_attempt
            .iter()
            .find(|(m, _)| m == n)
            .and_then(|(_, hunks)| hunks.get(*i))
            .ok_or_else(|| format!("{path}: a hunk picked is no longer there: compare again"))?;
        picked.push(hunk);
    }
    // Every hunk of the one attempt that deleted the file: the file goes, as it did there.
    let one_attempt = list.iter().all(|(n, _)| *n == first);
    let all_of_it = by_attempt
        .first()
        .is_some_and(|(_, hunks)| hunks.len() == list.len());
    if one_attempt && all_of_it && first_entry.is_none() {
        return Ok(None);
    }
    let text = hunks::apply(&base_text, &picked).map_err(|e| format!("{path}: {e}"))?;
    let blob = write_blob(repo, text.as_bytes())?;
    // A new file gets the mode the attempt gave it (an executable script stays one).
    let mode = base_entry
        .or(first_entry)
        .map_or_else(|| "100644".to_owned(), |(mode, _)| mode);
    Ok(Some((mode, blob)))
}

/// Stores `bytes` in the repository as they are (no filters: no line endings changed).
fn write_blob(repo: &Path, bytes: &[u8]) -> Result<String, String> {
    let file = match crate::remote::parse(repo) {
        Some(target) => crate::remote::host_temp_file(&target, "assemble", bytes)?,
        None => {
            static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "gitcraft-assemble-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::write(&path, bytes)
                .map_err(|e| format!("could not write a file to hash: {e}"))?;
            path.display().to_string()
        }
    };
    let blob = git::run(
        Some(repo),
        &["hash-object", "-w", "--no-filters", "--", &file],
    );
    if !crate::remote::is_remote(repo) {
        let _ = std::fs::remove_file(&file);
    }
    Ok(blob?.trim().to_owned())
}
