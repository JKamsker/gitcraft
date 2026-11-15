//! Fetched-ref tracking plans. Reads and writes use the repository's Git transport.
use super::guard::{Runner, text};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

pub(crate) const APPLY: &str = "\0tracking";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Values {
    remote: Vec<String>,
    merge: Vec<String>,
}
#[derive(Clone)]
pub(crate) struct Choice {
    pub full: String,
    pub remote: String,
    pub path: String,
    pub url: String,
    source: String,
}
pub(crate) struct View {
    pub branch: String,
    pub current: String,
    pub choices: Vec<Choice>,
    oid: String,
    prior: Values,
    config: String,
    catalog: String,
}
#[derive(Serialize, Deserialize)]
struct Request {
    branch: String,
    oid: String,
    expected: Values,
    desired: Values,
    target: Option<String>,
    config: Option<String>,
    catalog: Option<String>,
}
fn local_values(repo: &Path, key: &str) -> Result<Vec<String>, String> {
    let out = crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--null", "--get-all", key],
        &[0, 1],
    )?;
    Ok(out.split_terminator('\0').map(str::to_owned).collect())
}
fn values(repo: &Path, branch: &str) -> Result<Values, String> {
    Ok(Values {
        remote: local_values(repo, &format!("branch.{branch}.remote"))?,
        merge: local_values(repo, &format!("branch.{branch}.merge"))?,
    })
}
fn config(repo: &Path) -> Result<String, String> {
    text(repo, &["config", "--null", "--list", "--includes"])
}
fn source(spec: &str, full: &str) -> Option<String> {
    let (from, to) = spec.trim_start_matches('+').split_once(':')?;
    if let Some((prefix, suffix)) = to.split_once('*') {
        Some(from.replace('*', full.strip_prefix(prefix)?.strip_suffix(suffix)?))
    } else {
        (to == full).then(|| from.to_owned())
    }
}
pub(crate) fn read(repo: &Path, branch: &str) -> Result<View, String> {
    crate::git::run(
        Some(repo),
        &["check-ref-format", &format!("refs/heads/{branch}")],
    )?;
    let before = config(repo)?;
    let catalog = text(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )?;
    let refs = crate::git::refs(repo)?;
    let local = refs
        .local
        .iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| crate::i18n::text("tracking-stale").to_owned())?;
    let remotes = crate::git::remotes(repo)?;
    let mut choices = Vec::new();
    for (alias, branches) in refs.remotes {
        let Some(remote) = remotes.iter().find(|r| r.name == alias) else {
            continue;
        };
        let specs = super::remote_config::values(repo, &format!("remote.{alias}.fetch"))?;
        for b in branches {
            let full = format!("refs/remotes/{}", b.name);
            if let Some(source) = specs.iter().find_map(|s| source(s, &full)) {
                choices.push(Choice {
                    path: b.name,
                    full,
                    remote: alias.clone(),
                    url: remote.url.clone(),
                    source,
                });
            }
        }
    }
    let prior = values(repo, branch)?;
    if before != config(repo)?
        || catalog
            != text(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                    "refs/heads/",
                    "refs/remotes/",
                ],
            )?
    {
        return Err(crate::i18n::text("tracking-stale").into());
    }
    Ok(View {
        branch: branch.into(),
        current: local.upstream.clone(),
        oid: local.target.clone(),
        prior,
        config: before,
        catalog,
        choices,
    })
}
fn step(request: Request) -> Result<Vec<Vec<String>>, String> {
    Ok(vec![vec![
        APPLY.into(),
        serde_json::to_string(&request).map_err(|e| e.to_string())?,
    ]])
}
#[derive(PartialEq, Eq)]
pub(crate) struct Plan {
    pub ops: Vec<Vec<String>>,
    pub undo: Vec<Vec<String>>,
    pub redo: Vec<Vec<String>>,
}
impl View {
    pub fn plan(&self, selected: Option<&str>) -> Result<Plan, String> {
        let desired = match selected {
            Some(full) => {
                let c = self
                    .choices
                    .iter()
                    .find(|c| c.full == full)
                    .ok_or_else(|| crate::i18n::text("tracking-stale").to_owned())?;
                Values {
                    remote: vec![c.remote.clone()],
                    merge: vec![c.source.clone()],
                }
            }
            None => Values {
                remote: vec![],
                merge: vec![],
            },
        };
        let make = |expected, desired, target, config: Option<String>| Request {
            branch: self.branch.clone(),
            oid: self.oid.clone(),
            expected,
            desired,
            target,
            catalog: config.as_ref().map(|_: &String| self.catalog.clone()),
            config,
        };
        let apply = step(make(
            self.prior.clone(),
            desired.clone(),
            selected.map(str::to_owned),
            Some(self.config.clone()),
        ))?;
        let undo = step(make(desired.clone(), self.prior.clone(), None, None))?;
        let redo = step(make(
            self.prior.clone(),
            desired,
            selected.map(str::to_owned),
            None,
        ))?;
        Ok(Plan {
            ops: apply,
            undo,
            redo,
        })
    }
}
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
                super::remote_config::set_values(
                    &runner,
                    &format!("branch.{}.remote", r.branch),
                    &[],
                )?;
                super::remote_config::set_values(
                    &runner,
                    &format!("branch.{}.merge", r.branch),
                    &[],
                )?;
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
            super::remote_config::set_values(
                &runner,
                &format!("branch.{}.remote", r.branch),
                &r.desired.remote,
            )?;
            super::remote_config::set_values(
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
        let restore_remote = super::remote_config::set_values(
            &rollback,
            &format!("branch.{}.remote", r.branch),
            &r.expected.remote,
        );
        let restore_merge = super::remote_config::set_values(
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

#[cfg(test)]
mod tests;
