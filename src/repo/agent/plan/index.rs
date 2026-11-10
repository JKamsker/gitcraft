//! Repository guards and reversible staging plans.

use super::*;

/// The full hash of the commit `rev` names.
pub(super) fn commit_of(repo: &Path, rev: &str) -> Result<String, String> {
    let rev = args::revision(rev)?;
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .map(|s| s.trim().to_owned())
    .ok()
    .filter(|h| !h.is_empty())
    .ok_or_else(|| {
        crate::i18n::message(
            "repo-workflows-agent-commit-of-8c23b76d",
            &[("rev", (rev).into())],
        )
    })
}

/// The commit the local branch `name` is at, if there is such a branch.
pub(super) fn branch_at(repo: &Path, name: &str) -> Option<String> {
    git::resolve(repo, &format!("refs/heads/{name}"))
        .ok()
        .filter(|h| !h.is_empty())
}

/// A commit's summary, for the card.
pub(super) fn summary(repo: &Path, hash: &str) -> String {
    git::run(
        Some(repo),
        &["log", "-1", "--no-show-signature", "--format=%s", hash],
    )
    .map(|s| s.trim().to_owned())
    .unwrap_or_default()
}

pub(super) fn checked_paths(paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err(crate::i18n::text("repo-workflows-agent-checked-paths-0e0e013b").to_owned());
    }
    paths
        .iter()
        .map(|p| args::repo_path(p).map(str::to_owned))
        .collect()
}

/// "a.txt", or "3 paths".
pub(super) fn paths_label(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => format!("{} paths", many.len()),
    }
}

/// The conflicted files among `paths` (a path names a file, or a folder of them), as git
/// itself reads the paths: however they are spelled, git is the one that matches them.
pub(super) fn conflicted_among(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut args: Vec<&str> = vec!["--literal-pathspecs", "ls-files", "-u", "-z", "--"];
    args.extend(paths.iter().map(String::as_str));
    // `<mode> <hash> <stage>\t<path>`, a line per stage.
    let out = git::run(Some(repo), &args)?;
    let mut files: Vec<String> = out
        .split('\0')
        .filter_map(|line| line.split_once('\t').map(|(_, path)| path.to_owned()))
        .collect();
    files.dedup();
    Ok(files)
}

/// Refuses while a merge, rebase, cherry-pick or revert is in progress: `what` would end it, and
/// undo can't start it again.
pub(super) fn not_in_progress(repo: &Path, what: &str) -> Result<(), String> {
    match git::status(repo)?.in_progress {
        Some(state) => Err(crate::i18n::message(
            "repo-workflows-agent-not-in-progress-1049b227",
            &[
                (
                    "state-label-to-lowercase-1",
                    (state.label().to_lowercase()).clone().into(),
                ),
                ("what", (what).into()),
            ],
        )),
        None => Ok(()),
    }
}

/// The index entries under `paths` that differ from HEAD (staged changes): `mode,hash,path`, as
/// `update-index --cacheinfo` takes them back.
pub(super) fn staged_entries(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let with_paths = |args: &[&str]| -> Vec<String> {
        let mut all: Vec<String> = ["--literal-pathspecs"]
            .iter()
            .chain(args)
            .map(|s| (*s).to_owned())
            .collect();
        all.push("--".to_owned());
        all.extend(paths.iter().cloned());
        all
    };
    let run = |args: Vec<String>| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        git::run(Some(repo), &args)
    };
    // `<mode> <hash> <stage>\t<path>`.
    let index = run(with_paths(&["ls-files", "-s", "-z"]))?;
    // `<mode> <type> <hash>\t<path>`; nothing before the first commit.
    let head = run(with_paths(&["ls-tree", "-r", "-z", "HEAD"])).unwrap_or_default();
    let in_head: std::collections::HashSet<(String, String, String)> = head
        .split('\0')
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let mut m = meta.split(' ');
            let (mode, _, hash) = (m.next()?, m.next()?, m.next()?);
            Some((mode.to_owned(), hash.to_owned(), path.to_owned()))
        })
        .collect();
    Ok(index
        .split('\0')
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let mut m = meta.split(' ');
            let (mode, hash, stage) = (m.next()?, m.next()?, m.next()?);
            let entry = (mode.to_owned(), hash.to_owned(), path.to_owned());
            (stage == "0" && !in_head.contains(&entry)).then(|| format!("{mode},{hash},{path}"))
        })
        .collect())
}

/// Puts `paths` in the index back to HEAD's (nothing before the first commit).
pub(super) fn reset_index(here: &Here, paths: &[String]) -> Vec<String> {
    let mut step: Vec<String> = if here.head.is_empty() {
        [
            "--literal-pathspecs",
            "rm",
            "--cached",
            "-r",
            "-q",
            "-f",
            "--ignore-unmatch",
        ]
        .map(str::to_owned)
        .to_vec()
    } else {
        // With paths: a bare `reset` would also end a merge or cherry-pick in progress.
        ["--literal-pathspecs", "reset", "-q"]
            .map(str::to_owned)
            .to_vec()
    };
    step.push("--".to_owned());
    step.extend(paths.iter().cloned());
    step
}

/// Puts `paths` in the index back as they were: HEAD's, with the `staged` entries on top.
pub(super) fn restore_index(here: &Here, paths: &[String], staged: &[String]) -> Vec<Vec<String>> {
    let mut steps = vec![reset_index(here, paths)];
    if !staged.is_empty() {
        let mut put = vec!["update-index".to_owned(), "--add".to_owned()];
        for entry in staged {
            put.extend(["--cacheinfo".to_owned(), entry.clone()]);
        }
        steps.push(put);
    }
    steps
}

pub(super) fn stage(repo: &Path, here: &Here, paths: &[String]) -> Result<Plan, String> {
    let paths = checked_paths(paths)?;
    let mut add: Vec<String> = ["--literal-pathspecs", "add", "-A", "--"]
        .map(str::to_owned)
        .to_vec();
    add.extend(paths.iter().cloned());
    let label = format!("Stage {}", paths_label(&paths));
    let conflicted = conflicted_among(repo, &paths)?;
    if !conflicted.is_empty() {
        // Staging marks them resolved, and the conflict can't be brought back after.
        return Ok(Plan {
            label,
            ops: vec![add],
            refresh: Refresh::All,
            undo: Undo::None,
            ask: Some(Ask::from_presentation(AskPresentation::Stage {
                conflicted: conflicted.clone(),
            })),
        });
    }
    let staged = staged_entries(repo, &paths)?;
    Ok(Plan {
        label,
        ops: vec![add.clone()],
        refresh: Refresh::Status,
        undo: Undo::Custom {
            undo: restore_index(here, &paths, &staged),
            redo: vec![add],
        },
        ask: None,
    })
}

pub(super) fn unstage(repo: &Path, here: &Here, paths: &[String]) -> Result<Plan, String> {
    let paths = checked_paths(paths)?;
    let conflicted = conflicted_among(repo, &paths)?;
    if !conflicted.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-unstage-9d879ab6",
            &[("conflicted-join-1", (conflicted.join(", ")).clone().into())],
        ));
    }
    let staged = staged_entries(repo, &paths)?;
    if staged.is_empty() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-unstage-a79ec226",
            &[("paths-join-1", (paths.join(", ")).clone().into())],
        ));
    }
    let step = reset_index(here, &paths);
    Ok(Plan {
        label: format!("Unstage {}", paths_label(&paths)),
        ops: vec![step.clone()],
        refresh: Refresh::Status,
        undo: Undo::Custom {
            undo: restore_index(here, &paths, &staged),
            redo: vec![step],
        },
        ask: None,
    })
}
