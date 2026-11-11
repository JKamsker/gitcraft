//! Execution.

use super::*;

/// Runs the steps of `pending` from the first not done.
pub(super) fn run_from(repo: &Path, mut pending: Pending) -> Result<Outcome, String> {
    // Which branch is checked out where doesn't change while it runs: listed once.
    let worktrees = git::worktree_list(repo, None)?;
    while let Some(step) = pending.current().cloned() {
        let new_base = resolve(repo, &local_ref(&step.parent)).map_err(|_| {
            crate::i18n::message(
                "backend-which-is-based-on-is-gone-e39c7a",
                &[
                    ("parent", (step.parent).to_string().into()),
                    ("branch", (step.branch).to_string().into()),
                ],
            )
        })?;
        let tip = local_ref(&step.branch);
        // On its parent's tip already, from the base it was on (a parent rewound to an older
        // commit is not: the commits it lost are dropped from the layer too).
        let done =
            is_ancestor(repo, &new_base, &tip) && is_ancestor(repo, &step.old_base, &new_base);
        if !done {
            let at = match place(&worktrees, &step.branch) {
                Place::Worktree(path) => Some(path),
                _ => None,
            };
            // A merge or the like left half done there isn't this restack's to continue or
            // abort.
            if let Some(dir) = &at
                && let Some(state) = git::status(dir)?.in_progress
            {
                return Err(crate::i18n::message(
                    "backend-the-worktree-at-has-a-in-progress-finish-or-abort-it-first-032e5e",
                    &[
                        ("path", (dir.display()).to_string().into()),
                        ("state", (state.label().to_lowercase()).to_string().into()),
                    ],
                ));
            }
            keep_at(repo, at.as_deref())?;
            if let Err(e) = rebase(repo, at.as_deref(), &step, &new_base) {
                let stopped = rebase_stopped(repo, at.as_deref())?;
                if stopped {
                    return Ok(Outcome::Stopped {
                        branch: step.branch,
                        worktree: at,
                        message: e,
                    });
                }
                return Err(crate::i18n::message(
                    "backend-restacking-failed-e-27d710",
                    &[
                        ("branch", (step.branch).to_string().into()),
                        ("detail", (e).to_string().into()),
                    ],
                ));
            }
        }
        finish_step(repo, &mut pending, &new_base)?;
    }
    finish(repo, &pending)
}

/// Keeps which worktree the current step runs in.
pub(super) fn keep_at(repo: &Path, at: Option<&Path>) -> Result<(), String> {
    let key = format!("{SECTION}.at");
    match at {
        Some(path) => run_op(repo, &set_op(&key, &path.to_string_lossy())),
        None => run_ok_codes(
            Some(repo),
            &["config", "--local", "--unset-all", &key],
            &[0, 5],
        )
        .map(drop),
    }
}

/// Replays `step` onto `new_base`: here, or in the worktree `at` that has it checked out.
pub(super) fn rebase(
    repo: &Path,
    at: Option<&Path>,
    step: &Step,
    new_base: &str,
) -> Result<String, String> {
    // A `rebase.updateRefs` of the user's would move other branches along.
    let mut args = vec![
        "-c",
        "rebase.updateRefs=false",
        "rebase",
        "--onto",
        new_base,
    ];
    match at {
        Some(dir) => {
            args.extend(["--autostash", "--end-of-options", &step.old_base]);
            run(Some(dir), &args)
        }
        None => {
            args.extend(["--end-of-options", &step.old_base, &step.branch]);
            run(Some(repo), &args)
        }
    }
}

/// Whether a rebase is under way (stopped) in this worktree or in `at`.
pub(super) fn rebase_stopped(repo: &Path, at: Option<&Path>) -> Result<bool, String> {
    Ok(git::status(at.unwrap_or(repo))?.in_progress.is_some())
}

/// Records the step at the top of `pending` done: its branch is based on `new_base` now.
pub(super) fn finish_step(
    repo: &Path,
    pending: &mut Pending,
    new_base: &str,
) -> Result<(), String> {
    let Some(step) = pending.current() else {
        return Ok(());
    };
    run_op(repo, &set_op(&config_key(&step.branch, BASE_KEY), new_base))?;
    let tip = resolve(repo, &local_ref(&step.branch))?;
    let after = format!("{}\t{tip}", step.branch);
    run_op(repo, &add_op(&format!("{SECTION}.after"), &after))?;
    pending.after.push((step.branch.clone(), tip));
    pending.done += 1;
    let done = pending.done.to_string();
    run(
        Some(repo),
        &["config", "--local", &format!("{SECTION}.done"), &done],
    )
    .map(drop)
}

/// Checks out again what was checked out here, forgets the restack, and says what moved.
pub(super) fn finish(repo: &Path, pending: &Pending) -> Result<Outcome, String> {
    checkout_back(repo, &pending.head)?;
    let moved = moved(repo, pending)?;
    clear(repo)?;
    Ok(Outcome::Done(moved))
}

/// Checks `head` (`refs/heads/<branch>` or a commit) out here, unless it is already.
pub(super) fn checkout_back(repo: &Path, head: &str) -> Result<(), String> {
    let now = run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let now = now.trim();
    if let Some(branch) = head.strip_prefix("refs/heads/") {
        if now != head {
            // A branch's name never starts with `-`.
            run(Some(repo), &["checkout", "-q", branch])?;
        }
    } else if !head.is_empty() && (!now.is_empty() || git::head(repo) != head) {
        run(Some(repo), &["checkout", "-q", "--detach", head])?;
    }
    Ok(())
}
