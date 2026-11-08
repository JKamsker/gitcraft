//! What an agent's request becomes: the same steps, guards and undo the window's own buttons
//! use for that operation, worked out from the repository as it is now. Run on a background
//! thread (it reads the repository with git), never on the window's.
//!
//! Everything the request names is checked again here, whatever `gitcraft mcp` checked: the
//! window trusts nothing that came over the channel but the token.

use std::path::Path;

use super::super::dialogs::branch::create_branch_undo;
use super::super::operations::{expect_ref, expect_ref_at, expect_symref, stash_push_args};
use super::super::quick_edit::{self, Edit};
use super::super::sidebar::branch_delete::redo_delete;
use super::super::{Refresh, Undo, ops, short};
use crate::git;
use crate::mcp::args;
use crate::mcp::wire::{ResetMode, WriteOp};

/// What the window runs for a request, and what it asks the user first, if anything.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    /// The operation's name in the status line and the undo list.
    pub label: String,
    pub ops: Vec<Vec<String>>,
    pub refresh: Refresh,
    pub undo: Undo,
    /// Shown on the Approve / Deny card, for an operation that can't be undone or loses work.
    pub ask: Option<Ask>,
}

/// What the approval card says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    /// What and on what: "Delete branch feature/x".
    pub title: String,
    /// Exactly what will happen, in words.
    pub body: String,
    /// What undo can and can't do about it afterwards, said once under the commands.
    pub undo_note: String,
}

/// Said under a card's commands for what can't be undone from gitcraft.
const NO_UNDO: &str = "repo-workflows-agent--14b0fc42";

/// The repository as a plan needs it: HEAD's commit (empty before the first commit) and the
/// branch checked out (`None` when detached).
struct Here {
    head: String,
    branch: Option<String>,
}

impl Here {
    fn read(repo: &Path) -> Self {
        let branch = git::run_ok_codes(
            Some(repo),
            &["symbolic-ref", "-q", "--short", "HEAD"],
            &[0, 1],
        )
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty());
        Self {
            head: git::head(repo),
            branch,
        }
    }

    /// What is checked out, as undo goes back to it: the branch, or the commit when detached.
    fn checked_out(&self) -> String {
        self.branch.clone().unwrap_or_else(|| self.head.clone())
    }

    /// The branch checked out, or why an operation on it can't be done.
    fn branch(&self) -> Result<&str, String> {
        self.branch
            .as_deref()
            .ok_or_else(|| crate::i18n::text("repo-workflows-agent-branch-7776bacf").to_owned())
    }

    /// The guard that the same branch (or a detached HEAD) is still checked out.
    fn same_checkout(&self) -> Vec<String> {
        let full = self
            .branch
            .as_ref()
            .map(|b| format!("refs/heads/{b}"))
            .unwrap_or_default();
        expect_symref("HEAD", &full)
    }
}

/// The plan for `op` in `repo`, or why it can't be done.
pub(crate) fn plan(repo: &Path, op: &WriteOp) -> Result<Plan, String> {
    let here = Here::read(repo);
    match op {
        WriteOp::Stage { paths } => stage(repo, &here, paths),
        WriteOp::Unstage { paths } => unstage(repo, &here, paths),
        WriteOp::Commit { message, amend } => commit(repo, &here, message, *amend),
        WriteOp::CreateBranch {
            name,
            start,
            checkout,
        } => create_branch(repo, &here, name, start.as_deref(), *checkout),
        WriteOp::RenameBranch { from, to } => rename_branch(repo, from, to),
        WriteOp::Checkout { branch } => checkout(repo, &here, branch),
        WriteOp::Stash { message } => stash(repo, message),
        WriteOp::CherryPick { commit } => replay(
            repo,
            commit,
            crate::i18n::text("repo-workflows-agent-plan-4c49fdb6"),
            &["cherry-pick"],
        ),
        WriteOp::Revert { commit } => replay(
            repo,
            commit,
            crate::i18n::text("repo-workflows-agent-plan-0026c505"),
            &["revert", "--no-edit"],
        ),
        WriteOp::Reset { commit, mode } => reset(repo, &here, commit, *mode),
        WriteOp::MoveCommit { commit, up } => {
            let edit = if *up { Edit::MoveUp } else { Edit::MoveDown };
            rewrite(repo, &here, commit, edit)
        }
        WriteOp::DropCommit { commit } => rewrite(repo, &here, commit, Edit::Drop),
        WriteOp::DeleteBranch { name, force } => delete_branch(repo, &here, name, *force),
        WriteOp::Push { force } => push(repo, &here, *force),
    }
}

/// The full hash of the commit `rev` names.
fn commit_of(repo: &Path, rev: &str) -> Result<String, String> {
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
fn branch_at(repo: &Path, name: &str) -> Option<String> {
    git::resolve(repo, &format!("refs/heads/{name}"))
        .ok()
        .filter(|h| !h.is_empty())
}

/// A commit's summary, for the card.
fn summary(repo: &Path, hash: &str) -> String {
    git::run(
        Some(repo),
        &["log", "-1", "--no-show-signature", "--format=%s", hash],
    )
    .map(|s| s.trim().to_owned())
    .unwrap_or_default()
}

fn checked_paths(paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err(crate::i18n::text("repo-workflows-agent-checked-paths-0e0e013b").to_owned());
    }
    paths
        .iter()
        .map(|p| args::repo_path(p).map(str::to_owned))
        .collect()
}

/// "a.txt", or "3 paths".
fn paths_label(paths: &[String]) -> String {
    match paths {
        [one] => one.clone(),
        many => crate::i18n::message(
            "repo-workflows-agent-paths-label-26d36d06",
            &[("many-len-1", (many.len()).clone().into())],
        ),
    }
}

/// The conflicted files among `paths` (a path names a file, or a folder of them), as git
/// itself reads the paths: however they are spelled, git is the one that matches them.
fn conflicted_among(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
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
fn not_in_progress(repo: &Path, what: &str) -> Result<(), String> {
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
fn staged_entries(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
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
fn reset_index(here: &Here, paths: &[String]) -> Vec<String> {
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
fn restore_index(here: &Here, paths: &[String], staged: &[String]) -> Vec<Vec<String>> {
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

fn stage(repo: &Path, here: &Here, paths: &[String]) -> Result<Plan, String> {
    let paths = checked_paths(paths)?;
    let mut add: Vec<String> = ["--literal-pathspecs", "add", "-A", "--"]
        .map(str::to_owned)
        .to_vec();
    add.extend(paths.iter().cloned());
    let label = crate::i18n::message(
        "repo-workflows-agent-stage-3b4ddea7",
        &[("paths-label-paths-1", (paths_label(&paths)).clone().into())],
    );
    let conflicted = conflicted_among(repo, &paths)?;
    if !conflicted.is_empty() {
        // Staging marks them resolved, and the conflict can't be brought back after.
        return Ok(Plan {
            label,
            ops: vec![add],
            refresh: Refresh::All,
            undo: Undo::None,
            ask: Some(Ask {
                title: crate::i18n::text("repo-workflows-agent-stage-17e7b517").to_owned(),
                undo_note: crate::i18n::text(NO_UNDO).to_owned(),
                body: crate::i18n::message(
                    "repo-workflows-agent-stage-conflicts",
                    &[
                        ("count", (conflicted.len()).into()),
                        ("paths", (conflicted.join(", ")).into()),
                    ],
                ),
            }),
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

fn unstage(repo: &Path, here: &Here, paths: &[String]) -> Result<Plan, String> {
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
        label: crate::i18n::message(
            "repo-workflows-agent-unstage-eb003390",
            &[("paths-label-paths-1", (paths_label(&paths)).clone().into())],
        ),
        ops: vec![step.clone()],
        refresh: Refresh::Status,
        undo: Undo::Custom {
            undo: restore_index(here, &paths, &staged),
            redo: vec![step],
        },
        ask: None,
    })
}

/// The first line of a message, cut to fit a label.
fn headline(message: &str) -> String {
    let first = message.lines().next().unwrap_or("").trim();
    match first.char_indices().nth(50) {
        Some((i, _)) => format!("{}…", &first[..i]),
        None => first.to_owned(),
    }
}

fn commit(repo: &Path, here: &Here, message: &str, amend: bool) -> Result<Plan, String> {
    let message = args::message(message)?;
    if amend && here.head.is_empty() {
        return Err(crate::i18n::text("repo-workflows-agent-commit-abf5ac58").to_owned());
    }
    let staged = git::run(Some(repo), &["diff", "--cached", "--name-only", "-z"])?;
    if staged.is_empty() && !amend {
        return Err(crate::i18n::text("repo-workflows-agent-commit-d5a3b056").to_owned());
    }
    let mut step = vec!["commit".to_owned()];
    if amend {
        step.push("--amend".to_owned());
    }
    step.extend(["-m".to_owned(), message.to_owned()]);
    // Not plain "Commit": the window takes that label for its own commit box, and would clear
    // the message the user is writing there.
    let what = if amend {
        crate::i18n::text("repo-workflows-agent-commit-4475f515")
    } else {
        crate::i18n::text("repo-workflows-agent-commit-82a9c46f")
    };
    Ok(Plan {
        label: format!("{what} “{}”", headline(message)),
        ops: vec![step],
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--soft" },
        ask: None,
    })
}

/// Whether git takes `name` for a new branch.
fn check_branch_name(repo: &Path, name: &str) -> Result<(), String> {
    let name = args::branch_name(name)?;
    git::run(Some(repo), &["check-ref-format", "--branch", name])
        .map(|_| ())
        .map_err(|_| {
            crate::i18n::message(
                "repo-workflows-agent-check-branch-name-89d905bf",
                &[("name", (name).into())],
            )
        })
}

fn create_branch(
    repo: &Path,
    here: &Here,
    name: &str,
    start: Option<&str>,
    checkout: bool,
) -> Result<Plan, String> {
    check_branch_name(repo, name)?;
    if branch_at(repo, name).is_some() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-create-branch-8464d926",
            &[("name", (name).into())],
        ));
    }
    let at = commit_of(repo, start.unwrap_or("HEAD"))?;
    let step: Vec<String> = if checkout {
        ops(&[&["switch", "-c", name, &at]])
    } else {
        ops(&[&["branch", name, &at]])
    }
    .concat();
    Ok(Plan {
        label: crate::i18n::message(
            "repo-workflows-agent-create-branch-d2a49cac",
            &[("name", (name).into())],
        ),
        ops: vec![step.clone()],
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: create_branch_undo(name, &at, checkout, &here.checked_out()),
            redo: vec![step],
        },
        ask: None,
    })
}

fn rename_branch(repo: &Path, from: &str, to: &str) -> Result<Plan, String> {
    args::branch_name(from)?;
    check_branch_name(repo, to)?;
    if branch_at(repo, from).is_none() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-rename-branch-00b004f0",
            &[("from", (from).into())],
        ));
    }
    if branch_at(repo, to).is_some() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-rename-branch-4fe447cd",
            &[("to", (to).into())],
        ));
    }
    Ok(Plan {
        label: crate::i18n::message(
            "repo-workflows-agent-rename-branch-5de8c7a7",
            &[("from", (from).into()), ("to", (to).into())],
        ),
        ops: ops(&[&["branch", "-m", from, to]]),
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: ops(&[&["branch", "-m", to, from]]),
            redo: ops(&[&["branch", "-m", from, to]]),
        },
        ask: None,
    })
}

fn checkout(repo: &Path, here: &Here, branch: &str) -> Result<Plan, String> {
    args::branch_name(branch)?;
    if branch_at(repo, branch).is_none() {
        return Err(crate::i18n::message(
            "repo-workflows-agent-checkout-994822a7",
            &[("branch", (branch).into())],
        ));
    }
    if here.branch.as_deref() == Some(branch) {
        return Err(crate::i18n::message(
            "repo-workflows-agent-checkout-31e4be52",
            &[("branch", (branch).into())],
        ));
    }
    not_in_progress(
        repo,
        crate::i18n::text("repo-workflows-agent-checkout-7a3686d6"),
    )?;
    let previous = here.checked_out();
    // `--`: the name is a branch, never a file to throw the changes of away.
    let run = ops(&[&["checkout", branch, "--"]]);
    let undo = if previous.is_empty() {
        Undo::None
    } else {
        Undo::Custom {
            undo: ops(&[&["checkout", &previous, "--"]]),
            redo: run.clone(),
        }
    };
    Ok(Plan {
        label: crate::i18n::message(
            "repo-workflows-agent-checkout-734452f6",
            &[("branch", (branch).into())],
        ),
        ops: run,
        refresh: Refresh::All,
        undo,
        ask: None,
    })
}

fn stash(repo: &Path, message: &str) -> Result<Plan, String> {
    if git::status(repo)?.is_clean() {
        return Err(crate::i18n::text("repo-workflows-agent-stash-a7f01d16").to_owned());
    }
    not_in_progress(
        repo,
        crate::i18n::text("repo-workflows-agent-stash-4fce16d0"),
    )?;
    let message = message.trim();
    if message.chars().any(char::is_control) || message.len() > 1000 {
        return Err(crate::i18n::text("repo-workflows-agent-stash-ee431f75").to_owned());
    }
    let mut args: Vec<&str> = stash_push_args().to_vec();
    if !message.is_empty() {
        args.extend(["-m", message]);
    }
    Ok(Plan {
        label: crate::i18n::text("repo-workflows-agent-stash-38bdfeaa").to_owned(),
        ops: ops(&[&args]),
        refresh: Refresh::All,
        undo: Undo::Custom {
            // This stash, not one pushed since.
            undo: [expect_ref("refs/stash")]
                .into_iter()
                .chain(ops(&[&["stash", "pop", "--index"]]))
                .collect(),
            redo: ops(&[&args]),
        },
        ask: None,
    })
}

/// A cherry-pick or revert of `commit`: `command` and the commit.
fn replay(repo: &Path, commit: &str, what: &str, command: &[&str]) -> Result<Plan, String> {
    let hash = commit_of(repo, commit)?;
    let mut step: Vec<&str> = command.to_vec();
    step.push(&hash);
    Ok(Plan {
        label: format!("{what} {}", short(&hash)),
        ops: ops(&[&step]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
        ask: None,
    })
}

/// How many tracked files have uncommitted changes, staged or not (untracked ones aside: a hard
/// reset keeps them).
fn tracked_changes(repo: &Path) -> usize {
    let Ok(work) = git::status(repo) else {
        return 0;
    };
    let mut paths: Vec<&str> = work
        .staged
        .iter()
        .chain(
            work.unstaged
                .iter()
                .filter(|f| f.area != git::Area::Untracked),
        )
        .chain(&work.conflicted)
        .map(|f| f.path.as_str())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths.len()
}

/// How many commits `to` has that `from` doesn't.
fn commits_between(repo: &Path, from: &str, to: &str) -> usize {
    git::run(
        Some(repo),
        &["rev-list", "--count", &format!("{from}..{to}")],
    )
    .ok()
    .and_then(|n| n.trim().parse().ok())
    .unwrap_or(0)
}

/// The card of a hard reset of `target` (at `here`) to `hash`.
fn hard_ask(repo: &Path, here: &Here, hash: &str, target: &str) -> Ask {
    let lost = tracked_changes(repo);
    let left = commits_between(repo, hash, &here.head);
    Ask {
        title: crate::i18n::message(
            "repo-workflows-agent-hard-ask-e720ce31",
            &[
                ("target", (target).into()),
                ("short-hash-1", (short(hash)).into()),
            ],
        ),
        body: crate::i18n::message(
            "repo-workflows-agent-hard-reset-body",
            &[
                ("target", (target).into()),
                ("from", (short(&here.head)).into()),
                ("to", (short(hash)).into()),
                ("subject", (summary(repo, hash)).into()),
                ("lost", (lost).into()),
                ("left", (left).into()),
            ],
        ),
        undo_note: crate::i18n::message(
            "repo-workflows-agent-hard-ask-8de2bf44",
            &[("target", (target).into())],
        ),
    }
}

/// The card of a mixed reset when something is staged: unstaging it isn't undone (undo moves
/// the branch only). Nothing staged: no card.
fn mixed_ask(repo: &Path, here: &Here, hash: &str, target: &str) -> Result<Option<Ask>, String> {
    let staged = git::run(Some(repo), &["diff", "--cached", "--name-only", "-z"])?;
    let files = staged.split('\0').filter(|f| !f.is_empty()).count();
    if files == 0 {
        return Ok(None);
    }
    Ok(Some(Ask {
        title: crate::i18n::message(
            "repo-workflows-agent-mixed-ask-2c1fd48b",
            &[
                ("target", (target).into()),
                ("short-hash-1", (short(hash)).into()),
            ],
        ),
        body: crate::i18n::message(
            "repo-workflows-agent-mixed-reset-body",
            &[
                ("target", (target).into()),
                ("from", (short(&here.head)).into()),
                ("to", (short(hash)).into()),
                ("subject", (summary(repo, hash)).into()),
                ("files", (files).into()),
            ],
        ),
        undo_note: crate::i18n::message(
            "repo-workflows-agent-mixed-ask-2ddd09f5",
            &[("target", (target).into())],
        ),
    }))
}

fn reset(repo: &Path, here: &Here, commit: &str, mode: ResetMode) -> Result<Plan, String> {
    if here.head.is_empty() {
        return Err(crate::i18n::text("repo-workflows-agent-reset-7c10af6a").to_owned());
    }
    let hash = commit_of(repo, commit)?;
    let target = here.branch.clone().unwrap_or_else(|| "HEAD".to_owned());
    if hash == here.head {
        // Nothing would move, so nothing would go on the undo list, while the index (and a
        // hard reset's working tree) would still change.
        return Err(crate::i18n::message(
            "repo-workflows-agent-reset-5247bb50",
            &[
                ("target", (target).clone().into()),
                ("short-hash-1", (short(&hash)).into()),
            ],
        ));
    }
    not_in_progress(
        repo,
        crate::i18n::text("repo-workflows-agent-reset-c6ebf3c0"),
    )?;
    let word = match mode {
        ResetMode::Soft => "soft",
        ResetMode::Mixed => "mixed",
        ResetMode::Hard => "hard",
    };
    // Only while what was read here is still checked out, where it was.
    let steps = vec![
        here.same_checkout(),
        expect_ref_at("HEAD", &here.head),
        ops(&[&["reset", mode.flag(), &hash]]).concat(),
    ];
    let ask = match mode {
        ResetMode::Soft => None,
        ResetMode::Mixed => mixed_ask(repo, here, &hash, &target)?,
        ResetMode::Hard => Some(hard_ask(repo, here, &hash, &target)),
    };
    Ok(Plan {
        label: format!("Reset {target} to {} ({word})", short(&hash)),
        ops: steps,
        refresh: Refresh::All,
        // A hard reset is undone with `--keep`, which refuses rather than throw away what was
        // changed since.
        undo: Undo::HeadMove {
            mode: match mode {
                ResetMode::Soft => "--soft",
                ResetMode::Mixed => "--mixed",
                ResetMode::Hard => "--keep",
            },
        },
        ask,
    })
}

/// Moving or dropping `commit` of the checked-out branch: the rebase the commit's menu runs.
fn rewrite(repo: &Path, here: &Here, commit: &str, edit: Edit) -> Result<Plan, String> {
    let branch = here.branch()?.to_owned();
    if let Some(state) = git::status(repo)?.in_progress {
        return Err(crate::i18n::message(
            "repo-workflows-agent-rewrite-7d959a5b",
            &[(
                "state-label-to-lowercase-1",
                (state.label().to_lowercase()).clone().into(),
            )],
        ));
    }
    let hash = commit_of(repo, commit)?;
    let commits = git::rebase_commits(repo, &edit.base(&hash))?;
    let after = commits.iter().rev().take_while(|c| c.hash != hash).count();
    let editor = quick_edit::plan(edit, &hash, commits)?;
    let command = editor.command(repo)?;
    let what = match edit {
        Edit::Drop => crate::i18n::text("repo-workflows-agent-rewrite-601e2a07"),
        Edit::MoveUp => crate::i18n::text("repo-workflows-agent-rewrite-3d1ccb79"),
        Edit::MoveDown => crate::i18n::text("repo-workflows-agent-rewrite-2630fe5c"),
        Edit::Message => {
            return Err(crate::i18n::text("repo-workflows-agent-rewrite-ff973443").to_owned());
        }
    };
    let ask = (edit == Edit::Drop).then(|| Ask {
        title: crate::i18n::message(
            "repo-workflows-agent-rewrite-fc4198eb",
            &[("short-hash-1", (short(&hash)).into())],
        ),
        body: crate::i18n::message(
            "repo-workflows-agent-drop-body",
            &[
                ("commit", (short(&hash)).into()),
                ("subject", (summary(repo, &hash)).into()),
                ("branch", (branch.as_str()).into()),
                ("after", (after).into()),
            ],
        ),
        undo_note: crate::i18n::message(
            "repo-workflows-agent-rewrite-177f568d",
            &[("branch", (branch).clone().into())],
        ),
    });
    Ok(Plan {
        label: format!("{what} {}", short(&hash)),
        ops: vec![
            here.same_checkout(),
            expect_ref_at("HEAD", &editor.head),
            command,
        ],
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
        ask,
    })
}

fn delete_branch(repo: &Path, here: &Here, name: &str, force: bool) -> Result<Plan, String> {
    args::branch_name(name)?;
    let Some(target) = branch_at(repo, name) else {
        return Err(crate::i18n::message(
            "repo-workflows-agent-delete-branch-55a975e8",
            &[("name", (name).into())],
        ));
    };
    if here.branch.as_deref() == Some(name) {
        return Err(crate::i18n::message(
            "repo-workflows-agent-delete-branch-94d21a83",
            &[("name", (name).into())],
        ));
    }
    let flag = if force { "-D" } else { "-d" };
    // Only while it is where it was read: commits it got since are not what was approved.
    let steps = vec![
        expect_ref_at(&format!("refs/heads/{name}"), &target),
        ops(&[&["branch", flag, "--", name]]).concat(),
    ];

    Ok(Plan {
        label: crate::i18n::message(
            "repo-workflows-agent-delete-branch-d32d800a",
            &[("name", (name).into())],
        ),
        ops: steps,
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: ops(&[&["branch", "--", name, &target]]),
            redo: redo_delete(name, ops(&[&["branch", "-D", "--", name]])),
        },
        ask: Some(Ask {
            title: crate::i18n::message(
                "repo-workflows-agent-delete-branch-d32d800a",
                &[("name", (name).into())],
            ),
            body: crate::i18n::message(
                "repo-workflows-agent-delete-body",
                &[
                    ("name", (name).into()),
                    ("commit", (short(&target)).into()),
                    ("subject", (summary(repo, &target)).into()),
                    ("force", (if force { "yes" } else { "no" }).into()),
                ],
            ),
            undo_note: crate::i18n::text("repo-workflows-agent-delete-branch-7ef8245e").to_owned(),
        }),
    })
}

/// The remote and branch there that `upstream` (`origin/main`) names.
fn split_upstream(remotes: &[git::Remote], upstream: &str) -> Option<(String, String)> {
    remotes
        .iter()
        .filter_map(|r| {
            let rest = upstream.strip_prefix(&format!("{}/", r.name))?;
            Some((r.name.clone(), rest.to_owned()))
        })
        // `my/fork/x` is of the remote `my/fork` rather than `my`.
        .max_by_key(|(remote, _)| remote.len())
}

fn push(repo: &Path, here: &Here, force: bool) -> Result<Plan, String> {
    let branch = here.branch()?.to_owned();
    let refs = git::refs(repo)?;
    let remotes = git::remotes(repo)?;
    let local = refs
        .local
        .iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-agent-push-e5edacbd",
                &[("branch", (branch).clone().into())],
            )
        })?;
    let tracked = (!local.upstream.is_empty() && !local.upstream_gone())
        .then(|| split_upstream(&remotes, &local.upstream))
        .flatten();
    let (remote, there, publish) = match tracked {
        Some((remote, name)) => (remote, name, false),
        None => {
            let remote = remotes
                .iter()
                .map(|r| r.name.as_str())
                .find(|r| *r == "origin")
                .or_else(|| remotes.first().map(|r| r.name.as_str()))
                .ok_or(crate::i18n::text("repo-workflows-agent-push-f86f9688"))?
                .to_owned();
            (remote, branch.clone(), true)
        }
    };
    let (ahead, behind) = local.ahead_behind();
    if !publish && !force && ahead == 0 {
        return Err(crate::i18n::message(
            "repo-workflows-agent-push-2d7f2003",
            &[
                ("branch", (branch).clone().into()),
                ("remote", (remote).clone().into()),
                ("there", (there).clone().into()),
            ],
        ));
    }
    let refspec = format!("refs/heads/{branch}:refs/heads/{there}");
    let mut step = vec!["push".to_owned()];
    if force {
        // The lease is what the card shows: the remote branch where it was last fetched. A
        // fetch while the card waits doesn't widen what may be overwritten.
        let seen =
            git::resolve(repo, &format!("refs/remotes/{remote}/{there}")).unwrap_or_default();
        step.push(format!("--force-with-lease=refs/heads/{there}:{seen}"));
    }
    if publish {
        step.push("-u".to_owned());
    }
    step.extend([remote.clone(), refspec]);
    let to = format!("{remote}/{there}");
    let (title, body) = if publish {
        (
            crate::i18n::message(
                "repo-workflows-agent-push-7d6400fd",
                &[
                    ("branch", (branch).clone().into()),
                    ("remote", (remote).clone().into()),
                ],
            ),
            crate::i18n::message(
                "repo-workflows-agent-push-747bd6a7",
                &[
                    ("branch", (branch).clone().into()),
                    ("remote", (remote).clone().into()),
                    ("there", (there).clone().into()),
                ],
            ),
        )
    } else if force {
        let gone = match behind {
            0 => crate::i18n::message(
                "repo-workflows-agent-push-a74bf451",
                &[
                    ("to", (to).clone().into()),
                    ("branch", (branch).clone().into()),
                ],
            ),
            n => crate::i18n::message(
                "repo-workflows-agent-force-lost",
                &[
                    ("count", (n).into()),
                    ("to", (to.as_str()).into()),
                    ("branch", (branch.as_str()).into()),
                ],
            ),
        };
        (
            crate::i18n::message(
                "repo-workflows-agent-push-ccafdcaa",
                &[
                    ("branch", (branch).clone().into()),
                    ("to", (to).clone().into()),
                ],
            ),
            crate::i18n::message(
                "repo-workflows-agent-push-56d04849",
                &[
                    ("to", (to).clone().into()),
                    ("branch", (branch).clone().into()),
                    ("gone", (gone).clone().into()),
                ],
            ),
        )
    } else {
        (
            crate::i18n::message(
                "repo-workflows-agent-push-cb055d06",
                &[
                    ("branch", (branch).clone().into()),
                    ("to", (to).clone().into()),
                ],
            ),
            crate::i18n::message(
                "repo-workflows-agent-push-body",
                &[
                    ("branch", (branch.as_str()).into()),
                    ("count", (ahead).into()),
                    ("to", (to.as_str()).into()),
                ],
            ),
        )
    };
    // What is sent is the branch as the card describes it: not commits made since.
    let at = local.target.clone();
    Ok(Plan {
        label: format!(
            "{} {branch}",
            if force {
                crate::i18n::text("repo-workflows-agent-push-1d502fc3")
            } else {
                crate::i18n::text("repo-workflows-agent-push-731ce7ed")
            }
        ),
        ops: vec![expect_ref_at(&format!("refs/heads/{branch}"), &at), step],
        refresh: Refresh::All,
        undo: Undo::None,
        ask: Some(Ask {
            title,
            body,
            undo_note: crate::i18n::text(NO_UNDO).to_owned(),
        }),
    })
}

#[cfg(test)]
mod tests;
