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
    pub title: String,
    /// Exactly what will happen, in words.
    pub body: String,
}

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
            .ok_or_else(|| "HEAD is detached: check out a branch first".to_owned())
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
        WriteOp::CherryPick { commit } => replay(repo, commit, "Cherry-pick", &["cherry-pick"]),
        WriteOp::Revert { commit } => replay(repo, commit, "Revert", &["revert", "--no-edit"]),
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
    .ok_or_else(|| format!("`{rev}` is not a commit in this repository"))
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
        return Err("name at least one path".to_owned());
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
        many => format!("{} paths", many.len()),
    }
}

/// The conflicted files among `paths` (a path names a file, or a folder of them).
fn conflicted_among(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let work = git::status(repo)?;
    let inside = |file: &str| {
        paths.iter().any(|p| {
            let p = p.trim_end_matches('/');
            p == "." || file == p || file.starts_with(&format!("{p}/"))
        })
    };
    Ok(work
        .conflicted
        .into_iter()
        .map(|f| f.path)
        .filter(|f| inside(f))
        .collect())
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
    let label = format!("Stage {}", paths_label(&paths));
    let conflicted = conflicted_among(repo, &paths)?;
    if !conflicted.is_empty() {
        // Staging marks them resolved, and the conflict can't be brought back after.
        return Ok(Plan {
            label,
            ops: vec![add],
            refresh: Refresh::All,
            undo: Undo::None,
            ask: Some(Ask {
                title: "Mark conflicts resolved".to_owned(),
                body: format!(
                    "Stage {} as resolved: {}. git then forgets the conflict, so this can't be \
                     undone.",
                    if conflicted.len() == 1 {
                        "this conflicted file"
                    } else {
                        "these conflicted files"
                    },
                    conflicted.join(", ")
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
        return Err(format!(
            "{} conflicted: resolve the conflict first (unstaging would end it)",
            conflicted.join(", ")
        ));
    }
    let staged = staged_entries(repo, &paths)?;
    if staged.is_empty() {
        return Err(format!("nothing is staged in {}", paths.join(", ")));
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
        return Err("there is no commit to amend yet".to_owned());
    }
    let staged = git::run(Some(repo), &["diff", "--cached", "--name-only", "-z"])?;
    if staged.is_empty() && !amend {
        return Err(
            "nothing is staged: stage the changes to commit first (tool `stage`)".to_owned(),
        );
    }
    let mut step = vec!["commit".to_owned()];
    if amend {
        step.push("--amend".to_owned());
    }
    step.extend(["-m".to_owned(), message.to_owned()]);
    // Not plain "Commit": the window takes that label for its own commit box, and would clear
    // the message the user is writing there.
    let what = if amend { "Amend" } else { "Commit" };
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
        .map_err(|_| format!("`{name}` can't be a branch name"))
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
        return Err(format!("a branch called {name} already exists"));
    }
    let at = commit_of(repo, start.unwrap_or("HEAD"))?;
    let step: Vec<String> = if checkout {
        ops(&[&["switch", "-c", name, &at]])
    } else {
        ops(&[&["branch", name, &at]])
    }
    .concat();
    Ok(Plan {
        label: format!("Create branch {name}"),
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
        return Err(format!("there is no local branch called {from}"));
    }
    if branch_at(repo, to).is_some() {
        return Err(format!("a branch called {to} already exists"));
    }
    Ok(Plan {
        label: format!("Rename {from} to {to}"),
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
        return Err(format!(
            "there is no local branch called {branch} (create it with `create_branch`)"
        ));
    }
    if here.branch.as_deref() == Some(branch) {
        return Err(format!("{branch} is checked out already"));
    }
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
        label: format!("Checkout {branch}"),
        ops: run,
        refresh: Refresh::All,
        undo,
        ask: None,
    })
}

fn stash(repo: &Path, message: &str) -> Result<Plan, String> {
    if git::status(repo)?.is_clean() {
        return Err("there are no changes to stash".to_owned());
    }
    let message = message.trim();
    if message.chars().any(char::is_control) || message.len() > 1000 {
        return Err("the stash message must be one line of at most 1000 bytes".to_owned());
    }
    let mut args: Vec<&str> = stash_push_args().to_vec();
    if !message.is_empty() {
        args.extend(["-m", message]);
    }
    Ok(Plan {
        label: "Stash".to_owned(),
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

fn reset(repo: &Path, here: &Here, commit: &str, mode: ResetMode) -> Result<Plan, String> {
    if here.head.is_empty() {
        return Err("there are no commits to reset to yet".to_owned());
    }
    let hash = commit_of(repo, commit)?;
    let target = here.branch.clone().unwrap_or_else(|| "HEAD".to_owned());
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
    let ask = (mode == ResetMode::Hard).then(|| {
        let changed = git::status(repo).map(|w| w.changed_files()).unwrap_or(0);
        let lost = match changed {
            0 => "There are no uncommitted changes now.".to_owned(),
            1 => "The uncommitted changes to 1 file are thrown away for good.".to_owned(),
            n => format!("The uncommitted changes to {n} files are thrown away for good."),
        };
        Ask {
            title: "Hard reset".to_owned(),
            body: format!(
                "Move {target} from {} to {} (“{}”) and make the working tree match it. {lost} \
                 Undo moves {target} back, but can't bring those changes back.",
                short(&here.head),
                short(&hash),
                summary(repo, &hash)
            ),
        }
    });
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
        return Err(format!(
            "a {} is in progress: finish or abort it first",
            state.label().to_lowercase()
        ));
    }
    let hash = commit_of(repo, commit)?;
    let commits = git::rebase_commits(repo, &edit.base(&hash))?;
    let after = commits.iter().rev().take_while(|c| c.hash != hash).count();
    let editor = quick_edit::plan(edit, &hash, commits)?;
    let command = editor.command(repo)?;
    let what = match edit {
        Edit::Drop => "Drop commit",
        Edit::MoveUp => "Move commit up",
        Edit::MoveDown => "Move commit down",
        Edit::Message => return Err("the message is changed with `commit` and `amend`".to_owned()),
    };
    let ask = (edit == Edit::Drop).then(|| Ask {
        title: "Drop commit".to_owned(),
        body: format!(
            "Remove {} (“{}”) and its changes from {branch}, and replay the {after} commit{} \
             after it. Undo puts {branch} back as it is now.",
            short(&hash),
            summary(repo, &hash),
            if after == 1 { "" } else { "s" },
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
        return Err(format!("there is no local branch called {name}"));
    };
    if here.branch.as_deref() == Some(name) {
        return Err(format!(
            "{name} is checked out: check out another branch first"
        ));
    }
    let flag = if force { "-D" } else { "-d" };
    // Only while it is where it was read: commits it got since are not what was approved.
    let steps = vec![
        expect_ref_at(&format!("refs/heads/{name}"), &target),
        ops(&[&["branch", flag, "--", name]]).concat(),
    ];
    let merged_note = if force {
        " even if it isn't merged"
    } else {
        " (git refuses if it isn't merged)"
    };
    Ok(Plan {
        label: format!("Delete branch {name}"),
        ops: steps,
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: ops(&[&["branch", "--", name, &target]]),
            redo: redo_delete(name, ops(&[&["branch", "-D", "--", name]])),
        },
        ask: Some(Ask {
            title: "Delete branch".to_owned(),
            body: format!(
                "Delete the local branch “{name}”, at {} (“{}”),{merged_note}. Its copy on a \
                 remote stays. Undo makes it again at that commit.",
                short(&target),
                summary(repo, &target)
            ),
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
        .ok_or_else(|| format!("{branch} has no commits to push yet"))?;
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
                .ok_or("this repository has no remote to push to")?
                .to_owned();
            (remote, branch.clone(), true)
        }
    };
    let refspec = format!("refs/heads/{branch}:refs/heads/{there}");
    let mut step = vec!["push".to_owned()];
    if force {
        step.push("--force-with-lease".to_owned());
    }
    if publish {
        step.push("-u".to_owned());
    }
    step.extend([remote.clone(), refspec]);
    let (ahead, behind) = local.ahead_behind();
    let to = format!("{remote}/{there}");
    let body = if publish {
        format!("Publish {branch} on {remote} as {there}, and track it. A push can't be undone.")
    } else if force {
        format!(
            "Overwrite {to} with {branch} ({ahead} ahead, {behind} behind). The {behind} \
             commit{} only {to} has are gone from it, for everyone who uses it; with \
             --force-with-lease git refuses if {to} moved since it was last fetched. A push \
             can't be undone.",
            if behind == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "Send {branch}'s {ahead} new commit{} to {to}. A push can't be undone.",
            if ahead == 1 { "" } else { "s" }
        )
    };
    Ok(Plan {
        label: format!("{} {branch}", if force { "Force push" } else { "Push" }),
        ops: vec![step],
        refresh: Refresh::All,
        undo: Undo::None,
        ask: Some(Ask {
            title: if force { "Force push" } else { "Push" }.to_owned(),
            body,
        }),
    })
}

#[cfg(test)]
mod tests;
