//! Stash, replay, reset and history rewrite plans.

use super::*;

pub(super) fn stash(repo: &Path, message: &str) -> Result<Plan, String> {
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
pub(super) fn replay(
    repo: &Path,
    commit: &str,
    what: &str,
    command: &[&str],
) -> Result<Plan, String> {
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
pub(super) fn tracked_changes(repo: &Path) -> usize {
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
pub(super) fn commits_between(repo: &Path, from: &str, to: &str) -> usize {
    git::run(
        Some(repo),
        &["rev-list", "--count", &format!("{from}..{to}")],
    )
    .ok()
    .and_then(|n| n.trim().parse().ok())
    .unwrap_or(0)
}

/// The card of a hard reset of `target` (at `here`) to `hash`.
pub(super) fn hard_ask(repo: &Path, here: &Here, hash: &str, target: &str) -> Ask {
    let lost = tracked_changes(repo);
    let left = commits_between(repo, hash, &here.head);
    Ask::from_presentation(AskPresentation::HardReset {
        target: target.to_owned(),
        from: short(&here.head).to_owned(),
        to: short(hash).to_owned(),
        subject: summary(repo, hash),
        lost,
        left,
    })
}

/// The card of a mixed reset when something is staged: unstaging it isn't undone (undo moves
/// the branch only). Nothing staged: no card.
pub(super) fn mixed_ask(
    repo: &Path,
    here: &Here,
    hash: &str,
    target: &str,
) -> Result<Option<Ask>, String> {
    let staged = git::run(Some(repo), &["diff", "--cached", "--name-only", "-z"])?;
    let files = staged.split('\0').filter(|f| !f.is_empty()).count();
    if files == 0 {
        return Ok(None);
    }
    Ok(Some(Ask::from_presentation(AskPresentation::MixedReset {
        target: target.to_owned(),
        from: short(&here.head).to_owned(),
        to: short(hash).to_owned(),
        subject: summary(repo, hash),
        files,
    })))
}

pub(super) fn reset(
    repo: &Path,
    here: &Here,
    commit: &str,
    mode: ResetMode,
) -> Result<Plan, String> {
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
pub(super) fn rewrite(repo: &Path, here: &Here, commit: &str, edit: Edit) -> Result<Plan, String> {
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
        Edit::Drop => "Drop commit",
        Edit::MoveUp => "Move commit up",
        Edit::MoveDown => "Move commit down",
        Edit::Message => {
            return Err(crate::i18n::text("repo-workflows-agent-rewrite-ff973443").to_owned());
        }
    };
    let ask = (edit == Edit::Drop).then(|| {
        Ask::from_presentation(AskPresentation::Drop {
            commit: short(&hash).to_owned(),
            subject: summary(repo, &hash),
            branch: branch.clone(),
            after,
        })
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

pub(super) fn delete_branch(
    repo: &Path,
    here: &Here,
    name: &str,
    force: bool,
) -> Result<Plan, String> {
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
        label: format!("Delete branch {name}"),
        ops: steps,
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: ops(&[&["branch", "--", name, &target]]),
            redo: redo_delete(name, ops(&[&["branch", "-D", "--", name]])),
        },
        ask: Some(Ask::from_presentation(AskPresentation::DeleteBranch {
            name: name.to_owned(),
            commit: short(&target).to_owned(),
            subject: summary(repo, &target),
            force,
        })),
    })
}
