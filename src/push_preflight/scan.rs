//! Assemble a plan only after route, content and final snapshot checks agree.
use super::*;

pub(crate) fn scan(
    repo: &Path,
    original: Vec<Vec<String>>,
    profile: UserProfile,
    active: &dyn Fn() -> bool,
) -> Result<Plan, String> {
    let route = routing::resolve(repo, &original, active)?;
    let ids = objects::commits(repo, &route.head, &route.base, route.deleting, active)?;
    let (receive_pack, lfs_config) =
        capture::endpoints(repo, &route.checkout_oid, &route.remote, active)?;
    let urls = &route.urls;
    let first_url = urls.first().ok_or("No push target")?;
    let (expected_name, expected_email) = profile.expected_author(first_url);
    let expected_name = if expected_name.is_empty() {
        config(repo, "user.name", active)?
    } else {
        expected_name.to_owned()
    };
    let expected_email = if expected_email.is_empty() {
        config(repo, "user.email", active)?
    } else {
        expected_email.to_owned()
    };
    let signs = profile.own_signing && profile.signing.sign_commits
        || config(repo, "commit.gpgSign", active)?.eq_ignore_ascii_case("true");
    let (commits, mut findings) =
        findings::collect(repo, &route, &ids, &profile, &expected_email, signs, active)?;
    let lfs_objects = objects::inspect(
        repo,
        &route.head,
        &route.base,
        &commits,
        &mut findings,
        active,
    )?;
    findings::remote_history(
        repo,
        route.deleting,
        &route.base,
        &route.head,
        &expected_email,
        &mut findings,
        active,
    )?;
    let routing::Route {
        initial_config,
        config_before_push,
        checkout,
        checkout_oid,
        local_refs,
        remote,
        source,
        destination,
        head,
        push_oid,
        base,
        tracking_ref,
        urls,
        force,
        tracking,
        extras,
        deleting,
        skip_hooks,
    } = route;
    let config = config_stamp(repo, active)?;
    if config != initial_config {
        return Err("The push configuration changed while checking. Reopen preflight.".into());
    }
    // This is selected profile metadata, never a guessed identity from SSH's `git`
    // transport username. Reading it does not resolve credentials or contact a service.
    let expected_accounts = BTreeMap::new();
    let targets = urls.iter().map(|u| target_label(u)).collect();
    let plan = Plan {
        branch: source,
        remote,
        destination,
        head,
        base,
        commits,
        findings,
        targets,
        expected_name,
        expected_email,
        expected_accounts,
        profile,
        force,
        deleting,
        tracking,
        lfs_objects,
        skip_hooks,
        push_oid,
        original,
        urls,
        checkout,
        checkout_oid,
        config,
        config_before_push,
        local_refs,
        extras,
        tracking_ref,
        lfs_config,
        receive_pack,
        selection: None,
        fork_scope: None,
    };
    check_active(repo, &plan, active)?;
    if !plan.deleting
        && read(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}^{{commit}}", plan.push_oid),
            ],
            active,
        )?
        .trim()
            != plan.head
    {
        return Err("The source changed while checking. Reopen preflight.".into());
    }
    Ok(plan)
}
