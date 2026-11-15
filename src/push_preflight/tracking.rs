//! Match Git's successful `push --set-upstream` installation after all endpoints succeed.
use super::*;

pub(super) fn auto_rebase(repo: &Path, plan: &Plan) -> Result<bool, String> {
    Ok(plan.tracking
        && matches!(
            config(repo, "branch.autoSetupRebase", &|| true)?.as_str(),
            "always" | "remote"
        ))
}

pub(super) fn install(repo: &Path, plan: &Plan, rebase: bool) -> Result<(), String> {
    if !plan.tracking || !plan.destination.starts_with("refs/heads/") {
        return Ok(());
    }
    let Some(branch) = plan.branch.strip_prefix("refs/heads/") else {
        return Ok(());
    };
    // Unlike tracking-only edits (which override autoSetupRebase=never), publication
    // follows Git's branch.c/transport.c: a named remote honors remote/always, sets
    // rebase=true even over false, and otherwise leaves the existing value intact.
    for (field, value) in [("remote", &plan.remote), ("merge", &plan.destination)] {
        git::run(
            Some(repo),
            &["config", &format!("branch.{branch}.{field}"), value],
        )?;
    }
    if rebase {
        git::run(
            Some(repo),
            &["config", &format!("branch.{branch}.rebase"), "true"],
        )?;
    }
    Ok(())
}
