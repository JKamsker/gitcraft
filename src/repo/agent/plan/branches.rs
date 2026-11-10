//! Commit creation and local branch plans.

use super::*;

/// The first line of a message, cut to fit a label.
pub(super) fn headline(message: &str) -> String {
    let first = message.lines().next().unwrap_or("").trim();
    match first.char_indices().nth(50) {
        Some((i, _)) => format!("{}…", &first[..i]),
        None => first.to_owned(),
    }
}

pub(super) fn commit(repo: &Path, here: &Here, message: &str, amend: bool) -> Result<Plan, String> {
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
pub(super) fn check_branch_name(repo: &Path, name: &str) -> Result<(), String> {
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

pub(super) fn create_branch(
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

pub(super) fn rename_branch(repo: &Path, from: &str, to: &str) -> Result<Plan, String> {
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

pub(super) fn checkout(repo: &Path, here: &Here, branch: &str) -> Result<Plan, String> {
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
        label: format!("Checkout {branch}"),
        ops: run,
        refresh: Refresh::All,
        undo,
        ask: None,
    })
}
