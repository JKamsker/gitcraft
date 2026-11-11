//! Subtree association, completion, acceptance, and recovery transitions.

use super::*;

pub(super) fn association_message(prefix: &str, old: &str, base: &str) -> String {
    format!(
        "Associate vendored folder with explicitly reviewed upstream baseline; keep all folder contents\n\ngit-subtree-dir: {prefix}\ngit-subtree-mainline: {old}\ngit-subtree-split: {base}\n"
    )
}

pub(super) fn baseline_words(s: &Source, base: &str) -> Option<Vec<String>> {
    (s.history == History::Squash).then(|| {
        vec![
            "commit-tree".into(),
            format!("{base}^{{tree}}"),
            "-m".into(),
            format!(
                "Squashed baseline for {}\n\ngit-subtree-dir: {}\ngit-subtree-split: {base}\n",
                s.prefix, s.prefix
            ),
        ]
    })
}

pub(super) fn association_words(
    c: &Candidate,
    s: &Source,
    parent: &str,
    base: &str,
) -> Vec<String> {
    vec![
        "commit-tree".into(),
        format!("{}^{{tree}}", c.old),
        "-p".into(),
        c.old.clone(),
        "-p".into(),
        parent.into(),
        "-m".into(),
        association_message(&s.prefix, &c.old, base),
    ]
}

pub(super) fn associate(
    r: &Runner<'_>,
    v: &View,
    c: &Candidate,
    s: &Source,
) -> Result<String, String> {
    let base = c.upstream.as_deref().ok_or(crate::i18n::text(
        "backend-no-immutable-association-baseline-301ce5",
    ))?;
    let parent = if let Some(args) = baseline_words(s, base) {
        owned(
            r,
            v,
            c,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?
        .trim()
        .to_owned()
    } else {
        base.to_owned()
    };
    if !oid(&parent) {
        return Err(crate::i18n::text(
            "backend-invalid-association-parent-retained-without-updating-candidate-he-6819eb",
        )
        .into());
    }
    let args = association_words(c, s, &parent, base);
    let result = owned(
        r,
        v,
        c,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?
    .trim()
    .to_owned();
    if !oid(&result) {
        return Err(crate::i18n::text(
            "backend-invalid-association-result-retained-without-updating-candidate-he-583370",
        )
        .into());
    }
    owned(
        r,
        v,
        c,
        &["update-ref", "--no-deref", "HEAD", &result, &c.old],
    )?;
    Ok(result)
}

pub(super) fn finish(
    r: &Runner<'_>,
    v: &mut View,
    i: usize,
    result: Result<String, String>,
) -> Result<(), String> {
    let c = v.state.candidates[i].clone();
    let unmerged = owned(
        r,
        v,
        &c,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--diff-filter=U",
            "--",
        ],
    )?;
    let status = owned(
        r,
        v,
        &c,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    match result {
        Ok(out) if unmerged.is_empty() && status.is_empty() => {
            let value = if c.kind == Kind::Split {
                out.lines()
                    .rev()
                    .find(|s| oid(s))
                    .ok_or(crate::i18n::text(
                        "backend-subtree-split-did-not-return-an-immutable-commit-e9d2fb",
                    ))?
                    .into()
            } else {
                owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?
            };
            let value = value.trim().to_owned();
            if !oid(&value) {
                return Err(crate::i18n::text(
                    "backend-invalid-subtree-result-retained-for-inspection-642ff8",
                )
                .into());
            }
            if c.kind == Kind::Split {
                let s = source(v, &c.source)?;
                let expected = owned(
                    r,
                    v,
                    &c,
                    &["rev-parse", "--verify", &format!("{}:{}", c.old, s.prefix)],
                )?;
                let actual = owned(
                    r,
                    v,
                    &c,
                    &["rev-parse", "--verify", &format!("{value}^{{tree}}")],
                )?;
                if expected.trim() != actual.trim() {
                    return Err(crate::i18n::text("backend-split-result-tree-differs-from-the-selected-committed-folder-reta-816a29").into());
                }
            }
            v.state.candidates[i].result = Some(value);
            v.state.candidates[i].stage = if c.kind == Kind::Split {
                Stage::SplitReady
            } else {
                Stage::Ready
            };
            v.state.candidates[i].notice = crate::i18n::text(
                "backend-immutable-result-retained-original-checkout-and-branch-were-not-c-3f0680",
            )
            .into();
        }
        err => {
            v.state.candidates[i].stage = if !unmerged.is_empty() {
                Stage::Conflict
            } else {
                Stage::Paused
            };
            v.state.candidates[i].notice = crate::i18n::message(
                "backend-unmerged-candidate-status-inspect-retained-work-no-original-branc-5a2435",
                &[
                    (
                        "git",
                        (git::redact(
                            &err.err().unwrap_or_else(|| {
                                crate::i18n::text("backend-helper-left-working-changes-edf853")
                                    .into()
                            }),
                            None,
                        ))
                        .to_string()
                        .into(),
                    ),
                    ("unmerged", (unmerged).to_string().into()),
                    ("status", (status).to_string().into()),
                ],
            );
            if v.state.candidates[i].notice.len() > 65536 {
                v.state.candidates[i].notice =
                    v.state.candidates[i].notice.chars().take(12000).collect();
            }
        }
    }
    save(r.repo, v)
}

pub(crate) fn resume(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    let c = v.state.candidates[i].clone();
    if c.kind != Kind::Update
        || !matches!(c.stage, Stage::Conflict | Stage::Running | Stage::Paused)
    {
        return Err(crate::i18n::text(
            "backend-only-an-inspected-retained-import-may-be-continued-9a9ccb",
        )
        .into());
    }
    if !owned(
        r,
        v,
        &c,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--diff-filter=U",
            "--",
        ],
    )?
    .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-resolve-and-stage-every-candidate-conflict-before-continuing-orig-007c3a",
        )
        .into());
    }
    let head = owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?;
    let pending = owned(r, v, &c, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]);
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Running;
    save(r.repo, &mut view)?;
    let result = if pending.is_ok() {
        owned(r, &view, &c, &["commit", "--no-edit", "--no-verify"])
    } else if head.trim() != c.old {
        Ok(String::new())
    } else {
        Err(crate::i18n::text(
            "backend-no-acknowledged-merge-result-exists-inspect-this-candidate-then-p-9fc5b1",
        )
        .into())
    };
    finish(r, &mut view, i, result)?;
    Ok(crate::i18n::text(
        "backend-candidate-inspected-refresh-to-review-the-immutable-result-fab422",
    )
    .into())
}

pub(super) fn accept_allowed(
    c: &Candidate,
    branch: &str,
    head: &str,
    clean: bool,
) -> Result<(), String> {
    if c.stage != Stage::Ready
        || c.kind == Kind::Split
        || c.result.is_none()
        || branch != c.branch
        || head != c.old
        || !clean
    {
        return Err(crate::i18n::text(
            "backend-acceptance-requires-the-reviewed-direct-branch-at-its-original-co-723f86",
        )
        .into());
    }
    Ok(())
}

pub(crate) fn accept(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    let i = candidate(v, key)?;
    let c = v.state.candidates[i].clone();
    clean_source(&ctx)?;
    accept_allowed(&c, &v.branch, &v.head, true)?;
    if v.candidate_diff.as_ref().map(|(id, _)| id.as_str()) != Some(key) {
        return Err(crate::i18n::text(
            "backend-review-this-exact-candidate-patch-before-accepting-c38089",
        )
        .into());
    }
    if owned(r, v, &c, &["rev-parse", "--verify", "HEAD"])?.trim()
        != c.result.as_deref().unwrap_or("")
        || !owned(
            r,
            v,
            &c,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-candidate-changed-refresh-and-review-it-before-accepting-91448b",
        )
        .into());
    }
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Accepting;
    save(r.repo, &mut view)?;
    // Shared history transaction owns native HEAD/ref/index/checkout guards and recovery.
    // Failure retains Accepting; no materialization rollback or blind acceptance retry.
    preflight::accept_rewrite(
        r.repo,
        &c.branch,
        &c.old,
        c.result
            .as_deref()
            .ok_or(crate::i18n::text("backend-no-result-c6008b"))?,
    )?;
    view.state.candidates[i].stage = Stage::Accepted;
    view.state
        .sources
        .iter_mut()
        .find(|s| s.id == c.source)
        .ok_or(crate::i18n::text("backend-source-disappeared-5ff00e"))?
        .imported = c.upstream;
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-subtree-update-accepted-through-the-shared-guarded-checkout-trans-ec2bef",
    )
    .into())
}

pub(crate) fn reconcile(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    let ctx = check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    let c = &v.state.candidates[i];
    if c.stage != Stage::Accepting {
        return Err(crate::i18n::text(
            "backend-only-an-uncertain-acceptance-can-be-reconciled-here-inspect-ambig-a6dce1",
        )
        .into());
    }
    clean_source(&ctx)?;
    let head = resolve(&ctx, "HEAD")?;
    let branch = branch(&ctx)?;
    let stage = if branch == c.branch && Some(head.as_str()) == c.result.as_deref() {
        Stage::Accepted
    } else if branch == c.branch && head == c.old {
        Stage::Ready
    } else {
        return Err(crate::i18n::text(
            "backend-original-checkout-differs-from-both-exact-acceptance-receipts-ret-e5ab48",
        )
        .into());
    };
    let mut view = v.clone();
    view.state.candidates[i].stage = stage;
    if stage == Stage::Accepted {
        view.state
            .sources
            .iter_mut()
            .find(|s| s.id == c.source)
            .ok_or(crate::i18n::text("backend-source-disappeared-5ff00e"))?
            .imported = c.upstream.clone();
    }
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-exact-original-checkout-receipt-reconciled-no-branch-or-working-f-50d6bd",
    )
    .into())
}

pub(crate) fn close(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, false)?;
    let i = candidate(v, key)?;
    if matches!(
        v.state.candidates[i].stage,
        Stage::Accepting | Stage::Sending
    ) {
        return Err(crate::i18n::text(
            "backend-inspect-reconcile-the-uncertain-outcome-before-closing-its-metada-1a17fe",
        )
        .into());
    }
    let mut view = v.clone();
    view.state.candidates[i].stage = Stage::Closed;
    save(r.repo, &mut view)?;
    Ok(crate::i18n::text(
        "backend-proposal-closed-its-worktree-results-and-pins-remain-retained-for-d32620",
    )
    .into())
}
