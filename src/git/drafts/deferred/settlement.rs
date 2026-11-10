//! Deferred replay resume, acceptance, reconciliation, and close transitions.

use super::*;

pub(super) fn acceptance_allowed(job: &Job, undo: bool) -> Result<(), String> {
    if job.busy {
        return Err(crate::i18n::text(
            "backend-acceptance-outcome-is-uncertain-inspect-its-recovery-evidence-and-823070",
        )
        .into());
    }
    if job.cursor != job.steps.len() || (job.cancelled && !undo) {
        return Err(crate::i18n::text(
            "backend-complete-every-candidate-before-accepting-closed-proposals-can-on-6478dd",
        )
        .into());
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Replay {
    Continue,
    Reconcile,
    Start,
}

pub(super) fn replay(pending: bool, head_is_old: bool) -> Replay {
    if pending {
        Replay::Continue
    } else if head_is_old {
        Replay::Start
    } else {
        Replay::Reconcile
    }
}

pub(super) fn rebasing(path: &Path) -> Result<bool, String> {
    for name in ["rebase-merge", "rebase-apply"] {
        let p = git::run(
            Some(path),
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )?;
        if std::fs::symlink_metadata(p.trim()).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn conflicts(path: &Path) -> Result<Vec<String>, String> {
    let raw = git::run(
        Some(path),
        &[
            "diff",
            "--name-only",
            "--diff-filter=U",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
    )?;
    let paths = untracked_paths(raw.into_bytes())?;
    Ok(paths.into_iter().collect())
}

pub fn resume(r: &Runner<'_>, expected: &View, id: &str) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    let mut view = expected.clone();
    let j = view
        .jobs
        .jobs
        .iter()
        .position(|j| j.id == id)
        .ok_or(crate::i18n::text("backend-job-disappeared-b3757e"))?;
    if view.jobs.jobs[j].cancelled {
        return Err(crate::i18n::text(
            "backend-this-proposal-is-closed-its-worktrees-and-results-were-retained-bb7c5e",
        )
        .into());
    }
    while view.jobs.jobs[j].cursor < view.jobs.jobs[j].steps.len() {
        let i = view.jobs.jobs[j].cursor;
        if i > 0 {
            let parent = view.jobs.jobs[j].steps[i].parent.clone();
            if let Some(onto) = view.jobs.jobs[j].steps[..i]
                .iter()
                .find(|s| s.branch == parent)
                .and_then(|s| s.result.clone())
            {
                view.jobs.jobs[j].steps[i].onto = onto;
            }
        }
        view.jobs.jobs[j].busy = true;
        view.jobs.jobs[j].notice = crate::i18n::text(
            "backend-replay-is-in-flight-or-its-outcome-needs-reconciliation-after-int-1ce19b",
        )
        .into();
        save(r.repo, &mut view, &active)?;
        let mut s = view.jobs.jobs[j].steps[i].clone();
        if !s.made {
            safe_dir(s.worktree.parent().ok_or(crate::i18n::text(
                "backend-missing-owned-workspace-parent-421bdd",
            ))?)?;
            if s.worktree.exists() {
                return Err(
                    crate::i18n::text("backend-owned-path-already-exists-it-was-retained-rather-than-adopted-or-78089b")
                        .into(),
                );
            }
            let result = run(
                r,
                r.repo,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    "--lock",
                    "--reason=gitcraft deferred replay",
                    s.worktree.to_str().ok_or(crate::i18n::text(
                        "backend-worktree-path-must-be-utf-8-20c4a0",
                    ))?,
                    &s.old,
                ],
            );
            if let Err(e) = result {
                view.jobs.jobs[j].notice = crate::i18n::message(
                    "backend-worktree-creation-stopped-inspect-the-retained-path-before-retryi-32e9f2",
                    &[("detail", (e).to_string().into())],
                );
                save(r.repo, &mut view, &active)?;
                return Err(e);
            }
            s.root_identity = Some(root_identity(&s.worktree)?);
            s.made = true;
            view.jobs.jobs[j].steps[i] = s.clone();
            save(r.repo, &mut view, &active)?;
        }
        owned(r.repo, &view.common, &s)?;
        let pending = rebasing(&s.worktree)?;
        let head = git::resolve(&s.worktree, "HEAD")?;
        let result = match replay(pending, head == s.old) {
            Replay::Continue => owned_run(
                r,
                &view.common,
                &s,
                &["-c", "core.editor=true", "rebase", "--continue"],
            ),
            Replay::Reconcile => {
                // Explicit continuation reconciles a completed detached replay after a lost
                // acknowledgement. It does not rerun or reset that user's resolution work.
                Ok(
                    crate::i18n::text("backend-reconciled-the-current-detached-candidate-b853a7")
                        .into(),
                )
            }
            Replay::Start => owned_run(
                r,
                &view.common,
                &s,
                &[
                    "rebase",
                    "--no-update-refs",
                    "--no-autostash",
                    "--rebase-merges",
                    "--onto",
                    &s.onto,
                    &s.old_base,
                ],
            ),
        };
        let remaining = conflicts(&s.worktree)?;
        view.jobs.jobs[j].steps[i].conflicts = remaining.clone();
        if result.is_err() || !remaining.is_empty() || rebasing(&s.worktree)? {
            view.jobs.jobs[j].busy = false;
            view.jobs.jobs[j].notice = if remaining.is_empty() {
                let detail = result
                    .as_ref()
                    .err()
                    .map(|error| {
                        git::redact(error, None)
                            .chars()
                            .take(4000)
                            .collect::<String>()
                    })
                    .unwrap_or_else(|| {
                        crate::i18n::text("backend-deferred-replay-in-progress").into()
                    });
                crate::i18n::message(
                    "backend-deferred-replay-stopped",
                    &[("detail", detail.into())],
                )
            } else {
                crate::i18n::message(
                    "backend-conflicts-waiting-in-resolve-and-stage-them-there-then-continue-b65c65",
                    &[("branch", (s.branch).to_string().into())],
                )
            };
            save(r.repo, &mut view, &active)?;
            return result
                .map(|_| view.jobs.jobs[j].notice.clone())
                .or_else(|_| Ok(view.jobs.jobs[j].notice.clone()));
        }
        if !git::run(
            Some(&s.worktree),
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
        {
            return Err(crate::i18n::text(
                "backend-candidate-has-uncommitted-untracked-work-retain-or-commit-it-befo-cf9313",
            )
            .into());
        }
        let candidate = git::resolve(&s.worktree, "HEAD")?;
        git::run(
            Some(&s.worktree),
            &["merge-base", "--is-ancestor", &s.onto, &candidate],
        )
        .map_err(|_| {
            crate::i18n::text(
                "backend-candidate-no-longer-contains-its-reviewed-new-base-inspect-it-bef-8e0115",
            )
        })?;
        view.jobs.jobs[j].steps[i].result = Some(candidate);
        view.jobs.jobs[j].cursor += 1;
        view.jobs.jobs[j].busy = false;
        view.jobs.jobs[j].notice =
            crate::i18n::text("backend-candidate-layer-saved-original-branch-retained-21f878")
                .into();
        save(r.repo, &mut view, &active)?;
    }
    Ok(crate::i18n::text(
        "backend-every-candidate-is-ready-for-explicit-guarded-acceptance-original-63dc66",
    )
    .into())
}

pub fn close(
    repo: &Path,
    expected: &View,
    id: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    check(repo, expected, active)?;
    let mut next = expected.clone();
    let j = next
        .jobs
        .jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or(crate::i18n::text("backend-job-disappeared-b3757e"))?;
    j.cancelled = true;
    j.notice = crate::i18n::text(
        "backend-proposal-closed-owned-worktrees-resolution-work-and-immutable-res-1ccf73",
    )
    .into();
    save(repo, &mut next, active)?;
    Ok(crate::i18n::text("backend-proposal-closed-nothing-was-deleted-or-reset-3f9c8e").into())
}

pub(super) fn acceptance_place(
    r: &Runner<'_>,
    view: &View,
    id: &str,
    i: usize,
    branch: &str,
) -> Result<PathBuf, String> {
    let worktrees = git::worktree_list(r.repo, None)?;
    let places = worktrees
        .into_iter()
        .filter(|w| w.branch.as_deref() == Some(branch))
        .collect::<Vec<_>>();
    if places.len() > 1 {
        return Err(crate::i18n::text(
            "backend-branch-is-checked-out-in-multiple-worktrees-make-its-acceptance-t-68b867",
        )
        .into());
    }
    if let Some(w) = places.first() {
        return Ok(w.path.clone());
    }
    let p = view
        .common
        .join("gitcraft/deferred-rewrites")
        .join(id)
        .join(format!("accept-{i}"));
    if p.exists() {
        return Err(crate::i18n::text(
            "backend-an-earlier-acceptance-workspace-exists-but-is-not-attached-to-thi-715aa8",
        )
        .into());
    }
    safe_dir(p.parent().ok_or(crate::i18n::text(
        "backend-missing-acceptance-parent-152c38",
    ))?)?;
    run(
        r,
        r.repo,
        &[
            "worktree",
            "add",
            "--lock",
            "--reason=gitcraft guarded acceptance",
            p.to_str()
                .ok_or(crate::i18n::text("backend-invalid-acceptance-path-46b7dd"))?,
            branch,
        ],
    )?;
    Ok(p)
}

pub fn accept(r: &Runner<'_>, expected: &View, id: &str, undo: bool) -> Result<String, String> {
    let active = || !r.control.is_cancelled();
    check(r.repo, expected, &active)?;
    let mut view = expected.clone();
    let j = view
        .jobs
        .jobs
        .iter()
        .position(|j| j.id == id)
        .ok_or(crate::i18n::text("backend-job-disappeared-b3757e"))?;
    acceptance_allowed(&view.jobs.jobs[j], undo)?;
    let indices = if undo {
        (0..view.jobs.jobs[j].steps.len()).rev().collect::<Vec<_>>()
    } else {
        (0..view.jobs.jobs[j].steps.len()).collect()
    };
    for i in indices {
        let s = view.jobs.jobs[j].steps[i].clone();
        if s.accepted != undo {
            continue;
        }
        let result = s.result.as_deref().ok_or(crate::i18n::text(
            "backend-no-immutable-candidate-result-5fa445",
        ))?;
        let (old, new) = if undo {
            (result, s.old.as_str())
        } else {
            (s.old.as_str(), result)
        };
        if git::resolve(r.repo, &format!("refs/heads/{}", s.branch))? != old {
            return Err(crate::i18n::message(
                "backend-changed-since-this-proposal-no-update-of-that-branch-was-attempte-4c03d4",
                &[("branch", (s.branch).to_string().into())],
            ));
        }
        owned(r.repo, &view.common, &s)?;
        if !undo
            && (git::resolve(&s.worktree, "HEAD")? != result
                || rebasing(&s.worktree)?
                || !git::run(
                    Some(&s.worktree),
                    &["status", "--porcelain=v1", "--untracked-files=all"],
                )?
                .is_empty())
        {
            return Err(crate::i18n::text(
                "backend-candidate-changed-or-contains-unresolved-work-refresh-and-review-c12d9a",
            )
            .into());
        }
        let place = acceptance_place(r, &view, id, i, &s.branch)?;
        view.jobs.jobs[j].busy = true;
        view.jobs.jobs[j].notice = guarded_in_flight(&s.branch, undo);
        save(r.repo, &mut view, &active)?;
        let result = crate::push_preflight::accept_rewrite(
            &place,
            &format!("refs/heads/{}", s.branch),
            old,
            new,
        );
        if result.is_ok() {
            view.jobs.jobs[j].steps[i].accepted = !undo;
            view.jobs.jobs[j].busy = false;
            view.jobs.jobs[j].notice = accepted_tip(&s.branch, new, undo);
            save(r.repo, &mut view, &active)?;
        } else {
            view.jobs.jobs[j].notice = crate::i18n::text(
                "backend-guarded-acceptance-stopped-earlier-successful-layers-retain-their-61ef3b",
            )
            .into();
            save(r.repo, &mut view, &active)?;
            return result.map(|_| String::new());
        }
    }
    Ok(if undo {
        crate::i18n::text(
            "backend-restored-accepted-original-tips-through-guarded-transactions-2bec9f",
        )
    } else {
        crate::i18n::text(
            "backend-accepted-candidate-layers-through-guarded-transactions-per-layer-8a4829",
        )
    }
    .into())
}

pub fn reconcile(
    repo: &Path,
    expected: &View,
    id: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    check(repo, expected, active)?;
    let mut next = expected.clone();
    let j = next
        .jobs
        .jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or(crate::i18n::text("backend-job-disappeared-b3757e"))?;
    for s in &mut j.steps {
        let Some(candidate) = &s.result else {
            continue;
        };
        let actual = git::resolve(repo, &format!("refs/heads/{}", s.branch))?;
        if actual != s.old && actual != *candidate {
            return Err(crate::i18n::text(
                "backend-an-original-branch-has-another-tip-preserve-and-review-that-new-w-f7dd61",
            )
            .into());
        }
        for w in git::worktree_list(repo, None)?
            .iter()
            .filter(|w| w.branch.as_deref() == Some(&s.branch))
        {
            if !git::run(
                Some(&w.path),
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?
            .is_empty()
            {
                return Err(crate::i18n::text("backend-an-original-worktree-has-edits-or-an-incomplete-checkout-inspect-b7498f").into());
            }
        }
        if s.old != *candidate {
            s.accepted = actual == *candidate;
        }
    }
    j.busy = false;
    j.notice = crate::i18n::text(
        "backend-acceptance-receipts-reconciled-to-exact-original-candidate-oids-a-16087e",
    )
    .into();
    save(repo, &mut next, active)?;
    Ok(crate::i18n::text(
        "backend-reconciled-exact-branch-tips-no-files-or-branches-were-changed-2c67be",
    )
    .into())
}

pub(super) fn guarded_in_flight(branch: &str, undo: bool) -> String {
    crate::i18n::message(
        if undo {
            "backend-deferred-undo-in-flight"
        } else {
            "backend-deferred-acceptance-in-flight"
        },
        &[("branch", branch.into())],
    )
}

pub(super) fn accepted_tip(branch: &str, oid: &str, undo: bool) -> String {
    crate::i18n::message(
        if undo {
            "backend-deferred-restored-tip"
        } else {
            "backend-deferred-accepted-tip"
        },
        &[("branch", branch.into()), ("oid", oid.into())],
    )
}
