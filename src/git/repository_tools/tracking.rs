//! Fetched-ref tracking plans. Reads and writes use the repository's Git transport.
use super::guard::text;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub current_full: String,
    pub choices: Vec<Choice>,
    pub has_tracking: bool,
    pub ambiguous_mappings: bool,
    pub saving_blocked: Option<String>,
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
        &[
            "config",
            "--local",
            "--no-includes",
            "--null",
            "--get-all",
            key,
        ],
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
fn catalog_snapshot(repo: &Path) -> Result<String, String> {
    text(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )
}
pub(crate) fn read(repo: &Path, branch: &str) -> Result<View, String> {
    crate::git::run(
        Some(repo),
        &["check-ref-format", &format!("refs/heads/{branch}")],
    )?;
    let before = config(repo)?;
    let catalog = catalog_snapshot(repo)?;
    let refs = crate::git::refs(repo)?;
    let local = refs
        .local
        .iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| crate::i18n::text("tracking-stale").to_owned())?;
    let (choices, ambiguous_mappings) = catalog::choices(repo, &refs)?;
    let current_full = text(
        repo,
        &[
            "for-each-ref",
            "--format=%(upstream)",
            &format!("refs/heads/{branch}"),
        ],
    )?
    .trim()
    .to_owned();
    let prior = values(repo, branch)?;
    let mut saving_blocked = None;
    let mut has_tracking = false;
    for (field, own) in [("remote", &prior.remote), ("merge", &prior.merge)] {
        let key = format!("branch.{branch}.{field}");
        let effective = super::remote_config::values(repo, &key)?;
        has_tracking |= !effective.is_empty() || !own.is_empty();
        if effective != *own {
            saving_blocked = Some(crate::i18n::message(
                "tracking-inherited",
                &[("key", key.into())],
            ));
        }
    }
    if before != config(repo)? || catalog != catalog_snapshot(repo)? {
        return Err(crate::i18n::text("tracking-stale").into());
    }
    Ok(View {
        branch: branch.into(),
        has_tracking,
        ambiguous_mappings,
        saving_blocked,
        current: local.upstream.clone(),
        current_full,
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
        if let Some(why) = &self.saving_blocked {
            return Err(why.clone());
        }
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
mod apply;
mod catalog;
pub(crate) use apply::execute;

#[cfg(test)]
mod tests;
