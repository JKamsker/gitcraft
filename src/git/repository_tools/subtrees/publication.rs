//! Prepared subtree publication with captured principal and route.

use super::*;

pub(super) fn principal(
    c: Option<&git::Credentials>,
) -> Result<Option<(String, String, String)>, String> {
    c.map(|c| {
        let s = c.scope.as_ref().ok_or(crate::i18n::text(
            "backend-sign-in-lacks-endpoint-account-scope-sign-in-again-71595d",
        ))?;
        Ok((s.endpoint(), s.account().into(), c.username.clone()))
    })
    .transpose()
}

pub(super) fn publishable(c: &Candidate, renewed: bool) -> bool {
    c.kind == Kind::Split
        && c.result.is_some()
        && (c.stage == Stage::SplitReady
            || (renewed && matches!(c.stage, Stage::Sending | Stage::Paused)))
}

pub(super) fn push_words(s: &Source, c: &Candidate) -> Result<Vec<Vec<String>>, String> {
    Ok(vec![vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        "--recurse-submodules=check".into(),
        "--".into(),
        s.remote.clone(),
        format!(
            "{}:refs/heads/{}",
            c.result
                .as_deref()
                .ok_or(crate::i18n::text("backend-split-result-missing-1948a0"))?,
            s.branch
        ),
    ]])
}

pub(crate) fn prepare_publish(
    repo: &Path,
    v: &View,
    key: &str,
    profile: UserProfile,
    credentials: Option<&git::Credentials>,
    generation: u64,
    renewed: bool,
) -> Result<Prepared, String> {
    check(repo, v, true)?;
    let c = &v.state.candidates[candidate(v, key)?];
    if !publishable(c, renewed) {
        return Err(crate::i18n::text(
            "backend-split-is-not-ready-uncertain-rejected-sends-require-explicit-remo-73d014",
        )
        .into());
    }
    let context = crate::connections::current();
    if context.error.is_some() || context.choices != profile.connections {
        return Err(crate::i18n::text(
            "backend-selected-profile-connection-changed-prepare-again-b5de52",
        )
        .into());
    }
    let s = source(v, &c.source)?;
    route(repo, s)?;
    let plan = preflight::scan(repo, push_words(s, c)?, profile, &|| {
        !crate::tasks::cancellation().is_cancelled()
    })?;
    if Some(plan.head.as_str()) != c.result.as_deref()
        || plan.remote != s.remote
        || plan.destination != format!("refs/heads/{}", s.branch)
        || plan.force
        || plan.deleting
    {
        return Err(crate::i18n::text(
            "backend-publication-preflight-differs-from-the-registered-exact-split-des-11eaf6",
        )
        .into());
    }
    Ok(Prepared {
        view: v.clone(),
        id: key.into(),
        plan,
        context,
        principal: principal(credentials)?,
        generation,
        renewed,
    })
}

pub(crate) fn send(
    r: &Runner<'_>,
    p: &Prepared,
    reviewed: &preflight::ReviewedSend,
) -> Result<String, String> {
    reviewed.check_plan(&p.plan)?;
    let ctx = check(r.repo, &p.view, true)?;
    let _ = ctx;
    if p.context != crate::connections::current() || p.principal != principal(r.credentials)? {
        return Err(crate::i18n::text(
            "backend-prepared-connection-account-changed-prepare-and-approve-again-975afc",
        )
        .into());
    }
    let i = candidate(&p.view, &p.id)?;
    let c = &p.view.state.candidates[i];
    if !publishable(c, p.renewed) {
        return Err(
            crate::i18n::text("backend-split-no-longer-eligible-for-this-approval-51ae0f").into(),
        );
    }
    preflight::check(r.repo, &p.plan)?;
    let mut view = p.view.clone();
    view.state.candidates[i].stage = Stage::Sending;
    view.state.candidates[i].notice = crate::i18n::text(
        "backend-network-dispatch-intent-persisted-missing-rejected-receipt-never-786593",
    )
    .into();
    save(r.repo, &mut view)?;
    let result = preflight::execute_bound_controlled(
        r.repo,
        &p.plan,
        reviewed,
        r.credentials,
        Some(r.control),
    );
    view.state.candidates[i].stage = if result.is_ok() {
        Stage::Sent
    } else {
        Stage::Paused
    };
    view.state.candidates[i].notice = result
        .as_ref()
        .map(|_| {
            crate::i18n::text(
                "backend-exact-split-push-acknowledged-local-candidate-pins-retained-557cac",
            )
            .into()
        })
        .unwrap_or_else(|e| {
            crate::i18n::message(
                "backend-push-rejected-or-uncertain-inspect-destination-before-renewing-ap-8dc126",
                &[("git", (git::redact(e, None)).to_string().into())],
            )
        });
    save(r.repo, &mut view)?;
    result
}
