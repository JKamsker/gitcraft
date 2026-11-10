//! Approval validation and exact, explicitly requested publication.
use super::*;

pub(crate) fn check(repo: &Path, plan: &Plan) -> Result<(), String> {
    check_active(repo, plan, &|| true)
}

pub(super) fn check_active(
    repo: &Path,
    plan: &Plan,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    plan.check_fork_scope(repo)?;
    check_selected_snapshot(plan, |snapshot| snapshot.check_publication(repo))?;
    clean_lfs_config(repo, &plan.checkout_oid, active)?;
    if read(repo, &["rev-parse", "--verify", "HEAD"], active)?.trim() != plan.checkout_oid {
        return Err("The checked-out branch moved. Reopen push preflight.".into());
    }
    let config = config_stamp(repo, active)?;
    if (config != plan.config && config != plan.config_before_push)
        || optional(repo, &["symbolic-ref", "-q", "HEAD"], active)?.trim() != plan.checkout
        || (!plan.deleting
            && read(
                repo,
                &["rev-parse", "--verify", "--end-of-options", &plan.branch],
                active,
            )?
            .trim()
                != plan.push_oid)
    {
        return Err("The branch, checkout, identity or push configuration changed. Reopen push preflight; nothing was pushed.".into());
    }
    let urls: Vec<String> = read(
        repo,
        &["remote", "get-url", "--push", "--all", &plan.remote],
        active,
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    if urls != plan.urls {
        return Err("The push addresses changed. Reopen push preflight.".into());
    }
    Ok(())
}

pub(super) fn check_selected_snapshot(
    plan: &Plan,
    check: impl FnOnce(&git::repository_tools::guard::Snapshot) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(selected) = &plan.selection {
        check(&selected.snapshot)?;
    }
    Ok(())
}

pub(crate) fn push_args(plan: &Plan) -> Result<Vec<String>, String> {
    if let Some(selected) = &plan.selection {
        plan.matches_selection(&selected.route)?;
    }
    let mut expected_absent = false;
    for words in plan.original.iter().filter_map(|step| push_words(step)) {
        expected_absent |= matches!(
            lease_requirement(words, &plan.destination, plan.deleting)?,
            Some(CapturedLease::Absent)
        );
        if captured_lease(words, &plan.destination, plan.base.clone(), plan.deleting)? != plan.base
        {
            return Err("The captured push lease changed; reopen its original action.".into());
        }
    }
    if (plan.force || plan.deleting) && plan.base.is_none() && (!expected_absent || plan.deleting) {
        return Err(
            "Fetch this exact target before force pushing: no remote lease was inspected.".into(),
        );
    }
    let mut args = vec![
        "push".into(),
        "--porcelain".into(),
        "--no-follow-tags".into(),
        if plan
            .extras
            .iter()
            .any(|s| s == "--recurse-submodules=check")
        {
            "--recurse-submodules=check".into()
        } else {
            "--recurse-submodules=no".into()
        },
    ];
    if !plan.destination.starts_with("refs/for/") {
        if !plan.deleting
            && !plan.force
            && plan.findings.iter().any(|f| f.kind == Kind::RemoteHistory)
        {
            return Err("The fetched target is not an ancestor. Keep its commits or deliberately choose force-with-lease.".into());
        }
        // Normal mode is still a fast-forward (or absent-ref creation). The explicit
        // expected tip makes the inspected outgoing range valid at the server too.
        args.push(format!(
            "--force-with-lease={}:{}",
            plan.destination,
            plan.base.as_deref().unwrap_or_default()
        ));
    }
    args.extend(
        plan.extras
            .iter()
            .filter(|s| s.as_str() != "--recurse-submodules=check")
            .cloned(),
    );
    args.extend([
        "--".into(),
        plan.remote.clone(),
        format!("{}:{}", plan.push_oid, plan.destination),
    ]);
    Ok(args)
}

pub(crate) fn approved_ops(plan: Plan) -> Vec<Vec<String>> {
    let mut ops: Vec<_> = plan
        .local_refs
        .iter()
        .map(|(name, oid)| crate::repo::operations::expect_ref_at(name, oid))
        .collect();
    let original = plan.original.clone();
    let approved = register(plan);
    ops.extend(original.into_iter().map(|step| {
        if push_words(&step).is_some() {
            approved.clone()
        } else {
            step
        }
    }));
    ops
}

#[cfg(test)]
pub(crate) fn execute(
    repo: &Path,
    plan: &Plan,
    creds: Option<&git::Credentials>,
) -> Result<String, String> {
    execute_controlled(repo, plan, creds, None)
}
/// Source evidence returned by RepoTab::reviewed_send after the user's personal
/// rulebook gate. It is bound to the exact preflight argv, never a generic approval.
#[derive(Clone)]
pub(crate) struct ReviewedSend {
    original: Vec<String>,
    source: Option<Arc<git::CommandSource>>,
}
impl ReviewedSend {
    pub(crate) fn from_echo(
        original: Vec<String>,
        source: Option<Arc<git::CommandSource>>,
    ) -> Self {
        Self { original, source }
    }
    pub(crate) fn check_plan(&self, plan: &Plan) -> Result<(), String> {
        if self.original != push_args(plan)? {
            return Err("Reviewer echo belongs to a different exact push; prepare again".into());
        }
        Ok(())
    }
}
/// Execute only after caller preflight/consent and RepoTab::reviewed_send. Retain
/// original-route typed auth receipts and repeat route/source proof per endpoint.
pub(crate) fn execute_bound_controlled(
    repo: &Path,
    plan: &Plan,
    reviewed: &ReviewedSend,
    creds: Option<&git::Credentials>,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    reviewed.check_plan(plan)?;
    let captured = std::cell::RefCell::new(None);
    crate::repo::run_reviewed_push(
        &reviewed.original,
        |args| {
            let binding = git::RouteBinding::capture(repo, args)?;
            captured.replace(Some(binding.clone()));
            Ok(binding)
        },
        || {
            let route = captured.borrow();
            let route = route.as_ref().ok_or("Reviewed route was not captured")?;
            execute_controlled_with_guard(repo, plan, creds, control, || {
                route.verify(repo)?;
                if let Some(source) = &reviewed.source {
                    source.check_execution()?;
                }
                Ok(())
            })
        },
        control,
    )
}

pub(crate) fn execute_controlled(
    repo: &Path,
    plan: &Plan,
    creds: Option<&git::Credentials>,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    execute_controlled_with_guard(repo, plan, creds, control, || Ok(()))
}
pub(crate) fn execute_controlled_with_guard(
    repo: &Path,
    plan: &Plan,
    creds: Option<&git::Credentials>,
    control: Option<&Arc<git::OpControl>>,
    before_publish: impl Fn() -> Result<(), String>,
) -> Result<String, String> {
    if let Some(why) = crate::network::refused() {
        return Err(why.why().into());
    }
    check(repo, plan)?;
    let mut args = push_args(plan)?;
    for (i, url) in plan.urls.iter().enumerate() {
        if let Some(why) = crate::network::refused() {
            return Err(why.why().into());
        }
        check(repo, plan)?;
        let opaque = format!(
            "gitcraft-preflight-{}-{}-{i}",
            std::process::id(),
            git::now()
        );
        let remote_at = args.len() - 2;
        args[remote_at] = opaque.clone();
        let objects: Vec<_> = plan.lfs_objects.iter().cloned().collect();
        before_publish()?;
        plan.check_fork_scope(repo)?;
        git::push_preflight_with(
            repo,
            &args,
            git::PushTarget {
                address: url,
                skip_hooks: plan.skip_hooks,
                alias: &opaque,
                original_remote: &plan.remote,
                lfs_config: &plan.lfs_config,
                receive_pack: &plan.receive_pack,
                lfs_objects: &objects,
            },
            creds,
            control,
        )
        .map_err(|error| {
            if git::is_auth_error(&error) {
                remember_auth_endpoint(url)
            } else {
                error
            }
        })?;
    }
    if let Some(reference) = &plan.tracking_ref {
        // A background fetch may already have updated it. Never overwrite a newer tip.
        let args = if plan.deleting {
            vec![
                "update-ref",
                "-d",
                reference,
                plan.base.as_deref().unwrap_or(""),
            ]
        } else {
            vec![
                "update-ref",
                reference,
                &plan.head,
                plan.base.as_deref().unwrap_or(""),
            ]
        };
        let _ = git::forecast_run(repo, &args, &[], &[0], &|| true);
    }
    if plan.tracking
        && plan.branch.starts_with("refs/heads/")
        && plan.destination.starts_with("refs/heads/")
    {
        // Setting an upstream is local metadata; it is never used to choose this push's ref.
        let branch = plan.branch.trim_start_matches("refs/heads/");
        git::run(
            Some(repo),
            &["config", &format!("branch.{branch}.remote"), &plan.remote],
        )?;
        git::run(
            Some(repo),
            &[
                "config",
                &format!("branch.{branch}.merge"),
                &plan.destination,
            ],
        )?;
    }
    Ok("Push completed to the reviewed target(s).".into())
}
