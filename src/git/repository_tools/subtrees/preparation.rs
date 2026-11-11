//! Subtree route capture, fetch construction, and candidate preparation.

use super::*;

pub(super) fn route(repo: &Path, s: &Source) -> Result<(), String> {
    for flags in [
        vec!["remote", "get-url", "--all", "--", s.remote.as_str()],
        vec![
            "remote",
            "get-url",
            "--push",
            "--all",
            "--",
            s.remote.as_str(),
        ],
    ] {
        let urls = super::super::guard::text(repo, &flags)?;
        if urls.is_empty() {
            return Err(
                crate::i18n::text("backend-registered-source-has-no-endpoint-bbd38e").into(),
            );
        }
        for url in urls.lines() {
            validate_url(url)?;
        }
    }
    Ok(())
}

pub(super) fn fetch_words(s: &Source, pin: &str) -> Vec<String> {
    [
        "-c",
        "fetch.recurseSubmodules=false",
        "-c",
        "fetch.writeCommitGraph=false",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "fetch",
        "--no-tags",
        "--no-recurse-submodules",
        "--no-write-fetch-head",
        "--",
        &s.remote,
        &format!("refs/heads/{}:{pin}", s.branch),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub(crate) fn fetch(r: &Runner<'_>, v: &View, key: &str) -> Result<String, String> {
    check(r.repo, v, true)?;
    let s = source(v, key)?.clone();
    route(r.repo, &s)?;
    if let Some(why) = crate::network::refused() {
        return Err(why.why().into());
    }
    let pin = format!("refs/gitcraft/subtrees/fetch/{}", next_id());
    let ctx = check(r.repo, v, true)?;
    if !text(&ctx, &["rev-parse", "--verify", "-q", &pin], &[0, 1])?.is_empty() {
        return Err(crate::i18n::text(
            "backend-private-subtree-fetch-ref-already-exists-no-fetch-attempted-bd251e",
        )
        .into());
    }
    let args = fetch_words(&s, &pin);
    r.run(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    let ctx = check(r.repo, v, true)?;
    let result = resolve(&ctx, &pin)?;
    let mut view = v.clone();
    view.state
        .sources
        .iter_mut()
        .find(|s| s.id == key)
        .ok_or(crate::i18n::text("backend-subtree-disappeared-1c5e16"))?
        .fetched = Some(result);
    save(r.repo, &mut view)?;
    Ok(crate::i18n::message(
        "backend-fetched-the-registered-branch-into-pin-original-branch-index-unch-367bf0",
        &[("pin", (pin).to_string().into())],
    ))
}

pub(super) fn subtree_words(
    s: &Source,
    kind: Kind,
    old: &str,
    upstream: Option<&str>,
    exists: bool,
    sign_commits: bool,
) -> Result<Vec<String>, String> {
    let mut a = vec![
        "subtree".into(),
        match kind {
            Kind::Split => "split",
            Kind::Associate => {
                return Err(
                    crate::i18n::text("backend-association-uses-immutable-metadata-commits-not-a-subtree-helper-f977d8").into(),
                );
            }
            Kind::Update if exists => "merge",
            Kind::Update => "add",
        }
        .into(),
        format!("--prefix={}", s.prefix),
    ];
    if sign_commits {
        a.push("--gpg-sign".into());
    }
    if kind == Kind::Update {
        if s.history == History::Squash {
            a.push("--squash".into());
        }
        a.push(
            upstream
                .filter(|s| oid(s))
                .ok_or(crate::i18n::text(
                    "backend-fetch-a-source-commit-before-preparing-an-import-587a9b",
                ))?
                .into(),
        );
    } else {
        a.push(old.into());
    }
    Ok(a)
}

/// These options reach the inner Git command after deferred's ownership checks.
/// Keep the deferred runner's unsigned default for every other consumer.
pub(super) fn candidate_words<'a>(sign_commits: bool, args: &[&'a str]) -> Vec<&'a str> {
    let mut words = vec![
        "-c",
        if sign_commits {
            "commit.gpgSign=true"
        } else {
            "commit.gpgSign=false"
        },
    ];
    words.extend_from_slice(args);
    words
}

pub(super) fn owned(
    r: &Runner<'_>,
    v: &View,
    c: &Candidate,
    args: &[&str],
) -> Result<String, String> {
    deferred::registered_run(
        r,
        &v.common,
        &c.worktree,
        c.identity.as_deref().ok_or(crate::i18n::text(
            "backend-creation-outcome-is-unacknowledged-candidate-cannot-be-adopted-f78ef5",
        ))?,
        &candidate_words(c.sign_commits, args),
    )
}

pub(super) fn clean_source(ctx: &Context) -> Result<(), String> {
    let active = || !crate::tasks::cancellation().is_cancelled();
    git::drafts::inspection_index(ctx, &active)?;
    let head = resolve(ctx, "HEAD")?;
    let staged = ctx.run(
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            &head,
            "--",
        ],
        vec![],
        &[0],
        &active,
    )?;
    let work = ctx.work(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "--",
        ],
        vec![],
        &[0],
        &active,
    )?;
    let untracked = ctx.work(&["ls-files", "--others", "-z"], vec![], &[0], &active)?;
    if !staged.is_empty() || !work.is_empty() || !untracked.is_empty() {
        return Err(crate::i18n::text(
            "backend-original-checkout-must-have-no-staged-unstaged-or-untracked-files-b562d4",
        )
        .into());
    }
    Ok(())
}

pub(crate) fn prepare(
    r: &Runner<'_>,
    v: &View,
    key: &str,
    kind: Kind,
    sign_commits: bool,
) -> Result<String, String> {
    let ctx = check(r.repo, v, true)?;
    let s = source(v, key)?.clone();
    if !v.branch.starts_with("refs/heads/")
        || !text(&ctx, &["symbolic-ref", "-q", &v.branch], &[0, 1])?.is_empty()
    {
        return Err(crate::i18n::text(
            "backend-select-a-direct-local-branch-before-preparing-a-subtree-candidate-364b76",
        )
        .into());
    }
    let exists = tree(&ctx, &v.head, &s.prefix)?.is_some();
    if kind == Kind::Split && !exists {
        return Err(crate::i18n::text(
            "backend-commit-a-vendored-folder-before-splitting-its-history-994603",
        )
        .into());
    }
    let args = if kind == Kind::Associate {
        if !exists || s.imported.is_some() || s.fetched.is_none() {
            return Err(crate::i18n::text(
                "backend-association-requires-an-existing-folder-reviewed-fetched-baseline-8325da",
            )
            .into());
        }
        if v.local
            .as_ref()
            .is_none_or(|l| l.source != s.id || l.baseline != s.fetched)
        {
            return Err(crate::i18n::text(
                "backend-inspect-this-folder-against-the-exact-fetched-baseline-before-ass-b4af92",
            )
            .into());
        }
        Vec::new()
    } else {
        subtree_words(
            &s,
            kind,
            &v.head,
            s.fetched.as_deref(),
            exists,
            sign_commits,
        )?
    };
    if v.state.candidates.len() >= 64 {
        return Err(crate::i18n::text(
            "backend-64-candidates-retained-explicitly-archive-reviewed-metadata-befor-1eab61",
        )
        .into());
    }
    let id = next_id();
    let parent = tempfile::Builder::new()
        .prefix(&format!("gitcraft-subtree-{id}-"))
        .tempdir_in(&v.common)
        .map_err(|e| e.to_string())?
        .keep();
    let worktree = parent.join("candidate");
    let mut view = v.clone();
    view.state.candidates.push(Candidate {
        id: id.clone(),
        source: key.into(),
        branch: v.branch.clone(),
        old: v.head.clone(),
        upstream: if kind != Kind::Split {
            s.fetched.clone()
        } else {
            None
        },
        kind,
        sign_commits,
        stage: Stage::Preparing,
        worktree: worktree.clone(),
        identity: None,
        result: None,
        notice: crate::i18n::text(
            "backend-creation-intent-persisted-if-acknowledgement-is-lost-this-directo-00e4cd",
        )
        .into(),
    });
    save(r.repo, &mut view)?; // Before any helper/ref/working-folder operation.
    if std::fs::symlink_metadata(&worktree).is_ok() {
        return Err(crate::i18n::text(
            "backend-candidate-path-appeared-no-existing-directory-was-adopted-3ed7f8",
        )
        .into());
    }
    check(r.repo, &view, true)?;
    r.run(&[
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
        "worktree",
        "add",
        "--detach",
        "--lock",
        "--reason",
        "gitcraft retained subtree candidate",
        "--",
        worktree.to_str().ok_or(crate::i18n::text(
            "backend-candidate-path-requires-utf-8-df8c79",
        ))?,
        &v.head,
    ])?;
    let i = candidate(&view, &id)?;
    view.state.candidates[i].identity =
        Some(deferred::registered_identity(r.repo, &v.common, &worktree)?);
    view.state.candidates[i].stage = Stage::Running;
    view.state.candidates[i].notice = crate::i18n::text(
        "backend-helper-dispatch-recorded-an-interrupted-operation-requires-inspec-07f6aa",
    )
    .into();
    save(r.repo, &mut view)?;
    let c = view.state.candidates[i].clone();
    let result = if kind == Kind::Associate {
        associate(r, &view, &c, &s)
    } else {
        owned(
            r,
            &view,
            &c,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    };
    finish(r, &mut view, i, result)?;
    Ok(crate::i18n::message(
        "backend-subtree-candidate-id-retained-at-refresh-to-inspect-its-result-be-4e0ada",
        &[
            ("id", (id).to_string().into()),
            ("path", (worktree.display()).to_string().into()),
        ],
    ))
}
