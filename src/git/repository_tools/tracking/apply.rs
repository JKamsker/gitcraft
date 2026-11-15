//! Revalidated tracking mutation and exact rollback.
use super::super::{guard::Runner, remote_config};
use super::*;
use std::sync::Arc;

pub(crate) fn execute(
    repo: &Path,
    json: &str,
    control: Option<&Arc<crate::git::OpControl>>,
) -> Result<String, String> {
    let r: Request = serde_json::from_str(json).map_err(|e| e.to_string())?;
    crate::git::run(
        Some(repo),
        &["check-ref-format", &format!("refs/heads/{}", r.branch)],
    )?;
    let now = read(repo, &r.branch)?;
    if let Some(why) = &now.saving_blocked {
        return Err(why.clone());
    }
    if now.oid != r.oid
        || now.prior != r.expected
        || r.config.as_ref().is_some_and(|c| c != &now.config)
        || r.catalog.as_ref().is_some_and(|c| c != &now.catalog)
    {
        return Err(crate::i18n::text("tracking-stale").into());
    }
    if let Some(full) = &r.target {
        let c = now
            .choices
            .iter()
            .find(|c| &c.full == full)
            .ok_or_else(|| crate::i18n::text("tracking-stale").to_owned())?;
        if r.desired
            != (Values {
                remote: vec![c.remote.clone()],
                merge: vec![c.source.clone()],
            })
        {
            return Err(crate::i18n::text("tracking-stale").into());
        }
    }
    let owned = crate::git::OpControl::new(|| {});
    let runner = Runner {
        repo,
        control: control.unwrap_or(&owned),
        credentials: None,
    };
    let result = (|| -> Result<(), String> {
        if let Some(full) = &r.target {
            // Git appends rather than replaces ambiguous multi-value tracking keys.
            if r.expected.remote.len() > 1 || r.expected.merge.len() > 1 {
                remote_config::set_values(&runner, &format!("branch.{}.remote", r.branch), &[])?;
                remote_config::set_values(&runner, &format!("branch.{}.merge", r.branch), &[])?;
            }
            runner.run(&[
                "branch",
                &format!("--set-upstream-to={full}"),
                "--",
                &r.branch,
            ])?;
        } else {
            if r.desired.remote.is_empty() && r.desired.merge.is_empty() && !now.current.is_empty()
            {
                runner.run(&["branch", "--unset-upstream", "--", &r.branch])?;
            }
            remote_config::set_values(
                &runner,
                &format!("branch.{}.remote", r.branch),
                &r.desired.remote,
            )?;
            remote_config::set_values(
                &runner,
                &format!("branch.{}.merge", r.branch),
                &r.desired.merge,
            )?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = Runner {
            repo,
            control: &owned,
            credentials: None,
        };
        let restore_remote = remote_config::set_values(
            &rollback,
            &format!("branch.{}.remote", r.branch),
            &r.expected.remote,
        );
        let restore_merge = remote_config::set_values(
            &rollback,
            &format!("branch.{}.merge", r.branch),
            &r.expected.merge,
        );
        restore_remote?;
        restore_merge?;
        return Err(error);
    }
    Ok(String::new())
}
