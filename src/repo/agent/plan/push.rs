//! Explicit, reviewed push destinations and leases.

use super::*;

/// The remote and branch there that `upstream` (`origin/main`) names.
pub(super) fn split_upstream(remotes: &[git::Remote], upstream: &str) -> Option<(String, String)> {
    remotes
        .iter()
        .filter_map(|r| {
            let rest = upstream.strip_prefix(&format!("{}/", r.name))?;
            Some((r.name.clone(), rest.to_owned()))
        })
        // `my/fork/x` is of the remote `my/fork` rather than `my`.
        .max_by_key(|(remote, _)| remote.len())
}

pub(super) fn push(repo: &Path, here: &Here, force: bool) -> Result<Plan, String> {
    let branch = here.branch()?.to_owned();
    let refs = git::refs(repo)?;
    let remotes = git::remotes(repo)?;
    let local = refs
        .local
        .iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-agent-push-e5edacbd",
                &[("branch", (branch).clone().into())],
            )
        })?;
    let tracked = (!local.upstream.is_empty() && !local.upstream_gone())
        .then(|| split_upstream(&remotes, &local.upstream))
        .flatten();
    let (remote, there, publish) = match tracked {
        Some((remote, name)) => (remote, name, false),
        None => {
            let remote = remotes
                .iter()
                .map(|r| r.name.as_str())
                .find(|r| *r == "origin")
                .or_else(|| remotes.first().map(|r| r.name.as_str()))
                .ok_or(crate::i18n::text("repo-workflows-agent-push-f86f9688"))?
                .to_owned();
            (remote, branch.clone(), true)
        }
    };
    let (ahead, behind) = local.ahead_behind();
    if !publish && !force && ahead == 0 {
        return Err(crate::i18n::message(
            "repo-workflows-agent-push-2d7f2003",
            &[
                ("branch", (branch).clone().into()),
                ("remote", (remote).clone().into()),
                ("there", (there).clone().into()),
            ],
        ));
    }
    let refspec = format!("refs/heads/{branch}:refs/heads/{there}");
    let mut step = vec!["push".to_owned()];
    if force {
        // The lease is what the card shows: the remote branch where it was last fetched. A
        // fetch while the card waits doesn't widen what may be overwritten.
        let seen =
            git::resolve(repo, &format!("refs/remotes/{remote}/{there}")).unwrap_or_default();
        step.push(format!("--force-with-lease=refs/heads/{there}:{seen}"));
    }
    if publish {
        step.push("-u".to_owned());
    }
    step.extend([remote.clone(), refspec]);
    // What is sent is the branch as the card describes it: not commits made since.
    let at = local.target.clone();
    Ok(Plan {
        label: format!("{} {branch}", if force { "Force push" } else { "Push" }),
        ops: vec![expect_ref_at(&format!("refs/heads/{branch}"), &at), step],
        refresh: Refresh::All,
        undo: Undo::None,
        ask: Some(Ask::from_presentation(AskPresentation::Push {
            branch: branch.clone(),
            remote: remote.clone(),
            there: there.clone(),
            publish,
            force,
            ahead,
            behind,
        })),
    })
}
