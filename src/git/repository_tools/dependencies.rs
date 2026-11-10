//! Publication verifies the named collaborator endpoint, including gitlinks in outgoing history.
use super::{
    guard::{Runner, Snapshot, text},
    routes,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static CHECKS: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Debug)]
pub(crate) struct Dependency {
    pub path: String,
    pub repo: PathBuf,
    pub required: BTreeSet<String>,
    pub remotes: Vec<String>,
    pub remote: String,
    pub fetch_url: String,
    pub push_url: String,
    pub source: String,
    pub target: String,
    pub published: bool,
    pub snapshot: Option<Snapshot>,
    pub problem: Option<String>,
    pub result: Option<Result<String, String>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub snapshot: Snapshot,
    pub branch: String,
    pub commit: String,
    pub parent_remote: String,
    pub parent_url: String,
    pub parent_branch: String,
    pub dependencies: Vec<Dependency>,
    pub checked: bool,
    pub parent_result: Option<Result<String, String>>,
}
fn tree_links(repo: &Path, commit: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let out = text(
        repo,
        &["--no-replace-objects", "ls-tree", "-r", "-z", commit],
    )?;
    let mut links: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in out.split('\0').filter(|s| !s.is_empty()) {
        let Some((meta, path)) = row.split_once('\t') else {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        };
        let f: Vec<_> = meta.split_whitespace().collect();
        if f.len() == 3 && f[0] == "160000" {
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(
                    crate::i18n::text("backend-unsafe-submodule-path-in-git-tree-bb14e4").into(),
                );
            }
            links.entry(path.into()).or_default().insert(f[2].into());
        }
    }
    Ok(links)
}
fn outgoing_links(
    repo: &Path,
    tip: &str,
    remote: Option<&str>,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut links = tree_links(repo, tip)?;
    let excluded = remote.map(|s| format!("^{s}"));
    if let Some(oid) = remote {
        text(repo, &["cat-file", "-e", &format!("{oid}^{{commit}}")]).map_err(|_| {
            crate::i18n::text(
                "backend-fetch-the-destination-branch-first-its-tip-is-not-available-local-ed7eff",
            )
            .to_owned()
        })?;
    }
    let mut args = vec![
        "--no-replace-objects",
        "log",
        "--root",
        "-m",
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-renames",
        "--format=",
        tip,
    ];
    if let Some(excluded) = &excluded {
        args.push(excluded);
    }
    let changes = text(repo, &args)?;
    let mut rows = changes.split('\0');
    while let Some(meta) = rows.next() {
        let meta = meta.trim_start_matches('\n');
        if !meta.starts_with(':') {
            continue;
        }
        let path = rows.next().ok_or(crate::i18n::text(
            "backend-incomplete-raw-gitlink-record-c05320",
        ))?;
        let f: Vec<_> = meta.split_whitespace().collect();
        if f.len() != 5 {
            return Err(crate::i18n::text("backend-invalid-raw-gitlink-record-a7d9f1").into());
        }
        if f[1] == "160000" {
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(crate::i18n::text(
                    "backend-unsafe-submodule-path-in-outgoing-history-c9805c",
                )
                .into());
            }
            links.entry(path.into()).or_default().insert(f[3].into());
        }
    }
    Ok(links)
}
fn add(repo: &Path, path: String, required: BTreeSet<String>, preferred: &str) -> Dependency {
    let child = crate::remote::join(repo, &path);
    let remotes = text(&child, &["remote"])
        .map(|s| s.lines().map(str::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    let remote = if remotes.iter().any(|r| r == preferred) {
        preferred.into()
    } else if remotes.iter().any(|r| r == "origin") {
        "origin".into()
    } else {
        remotes.first().cloned().unwrap_or_default()
    };
    Dependency {
        path,
        repo: child,
        required,
        remotes,
        remote,
        fetch_url: String::new(),
        push_url: String::new(),
        source: String::new(),
        target: String::new(),
        published: false,
        snapshot: None,
        problem: None,
        result: None,
    }
}
pub(crate) fn read(repo: &Path) -> Result<Plan, String> {
    let branch = text(repo, &["symbolic-ref", "--short", "HEAD"])?
        .trim()
        .to_owned();
    let routes = routes::read(repo, &branch)?;
    let route = routes
        .branches
        .iter()
        .find(|b| b.name == branch)
        .ok_or(crate::i18n::text("backend-current-branch-is-unborn-bb8a2b"))?;
    if route.push.remote.is_empty() {
        return Err(
            crate::i18n::text("backend-choose-the-parent-s-push-route-first-3f38ae").into(),
        );
    }
    let urls = text(
        repo,
        &[
            "remote",
            "get-url",
            "--push",
            "--all",
            "--",
            &route.push.remote,
        ],
    )?;
    let urls: Vec<_> = urls.lines().collect();
    if urls.len() != 1 {
        return Err(crate::i18n::text(
            "backend-dependency-publication-requires-one-explicit-parent-push-url-use-a83308",
        )
        .into());
    }
    super::remote_config::validate_url(urls[0])?;
    let dependencies = tree_links(repo, &route.oid)?
        .into_iter()
        .map(|(p, required)| add(repo, p, required, &route.push.remote))
        .collect();
    Ok(Plan {
        snapshot: routes.snapshot,
        branch,
        commit: route.oid.clone(),
        parent_remote: route.push.remote.clone(),
        parent_url: urls[0].into(),
        parent_branch: route.push.branch.clone(),
        dependencies,
        checked: false,
        parent_result: None,
    })
}
fn remote_tip(
    runner: &Runner<'_>,
    repo: &Path,
    url: &str,
    branch: &str,
) -> Result<Option<String>, String> {
    let name = format!("refs/heads/{branch}");
    let out = runner.endpoint(repo, url, "", &["ls-remote", "--refs", "--"], &[&name])?;
    Ok(out.lines().find_map(|l| {
        l.split_once('\t')
            .filter(|(_, r)| *r == name)
            .map(|(o, _)| o.into())
    }))
}
fn ancestor(
    runner: &Runner<'_>,
    budget: &super::ReadBudget,
    repo: &Path,
    a: &str,
    b: &str,
) -> Result<bool, String> {
    let out = budget.read(
        repo,
        &["--no-replace-objects", "merge-base", "--is-ancestor", a, b],
        Some(runner.control.clone()),
    )?;
    match out.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    }
}

pub(crate) fn check(runner: &Runner<'_>, plan: Plan) -> Result<Plan, String> {
    crate::tasks::with_cancellation(runner.control.cancellation(), || check_inner(runner, plan))
}
fn check_inner(runner: &Runner<'_>, mut plan: Plan) -> Result<Plan, String> {
    let budget = super::ReadBudget::new(2048, std::time::Duration::from_secs(300));
    plan.snapshot.check_publication(runner.repo)?;
    let tip = remote_tip(runner, runner.repo, &plan.parent_url, &plan.parent_branch)?;
    let root = outgoing_links(runner.repo, &plan.commit, tip.as_deref())?;
    let overrides: BTreeMap<_, _> = plan
        .dependencies
        .iter()
        .map(|d| (d.path.clone(), (d.remote.clone(), d.target.clone())))
        .collect();
    plan.dependencies = root
        .into_iter()
        .map(|(p, required)| {
            let mut n = add(runner.repo, p.clone(), required, &plan.parent_remote);
            if let Some((remote, target)) = overrides.get(&p) {
                n.remote = remote.clone();
                n.target = target.clone();
            }
            n
        })
        .collect();
    let mut index = 0;
    while index < plan.dependencies.len() {
        budget.take()?;
        if runner.control.is_cancelled() {
            return Err(crate::git::CANCELLED.into());
        }
        if plan.dependencies.len() > 128 {
            return Err(crate::i18n::text(
                "backend-more-than-128-submodule-dependencies-narrow-this-publication-befo-cc96d4",
            )
            .into());
        }
        let node = &mut plan.dependencies[index];
        let outcome = inspect_node(runner, node, &budget);
        node.problem = outcome.err();
        if node.problem.is_none() {
            let mut nested = BTreeMap::<String, BTreeSet<String>>::new();
            for oid in &node.required {
                budget.take()?;
                if runner.control.is_cancelled() {
                    return Err(crate::git::CANCELLED.into());
                }
                for (p, oids) in tree_links(&node.repo, oid)? {
                    nested.entry(p).or_default().extend(oids);
                }
            }
            if !node.published && !node.source.is_empty() {
                let remote = remote_tip(runner, &node.repo, &node.push_url, &node.target)?;
                for (p, oids) in outgoing_links(&node.repo, &node.source, remote.as_deref())? {
                    nested.entry(p).or_default().extend(oids);
                }
            }
            let prefix = node.path.clone();
            let preferred = node.remote.clone();
            for (path, required) in nested {
                let full = format!("{prefix}/{path}");
                let mut n = add(runner.repo, full.clone(), required, &preferred);
                if let Some((remote, target)) = overrides.get(&full) {
                    n.remote = remote.clone();
                    n.target = target.clone();
                }
                plan.dependencies.push(n);
            }
        }
        index += 1;
    }
    plan.snapshot.check_publication(runner.repo)?;
    plan.checked = true;
    plan.parent_result = None;
    Ok(plan)
}
/// Publish children only. The UI must hand the parent to root's reviewed push
/// continuation; this worker never authorizes or sends a parent push.
pub(crate) fn publish_children(runner: &Runner<'_>, plan: Plan) -> Result<Plan, String> {
    crate::tasks::with_cancellation(runner.control.cancellation(), || {
        publish_inner(runner, plan)
    })
}
fn publish_inner(runner: &Runner<'_>, mut plan: Plan) -> Result<Plan, String> {
    let budget = super::ReadBudget::new(2048, std::time::Duration::from_secs(300));
    if !plan.checked || plan.dependencies.iter().any(|n| n.problem.is_some()) {
        return Err(crate::i18n::text(
            "backend-check-every-dependency-and-resolve-its-problem-before-publishing-b3ab61",
        )
        .into());
    }
    plan.snapshot.check_publication(runner.repo)?;
    let failed = publish_in_order(&mut plan.dependencies, |node| {
        node.snapshot
            .as_ref()
            .ok_or(crate::i18n::text(
                "backend-submodule-snapshot-missing-ae3935",
            ))?
            .check_publication(&node.repo)?;
        if !reachable(runner, node, &budget)? {
            text(
                &node.repo,
                &["check-ref-format", &format!("refs/heads/{}", node.target)],
            )?;
            runner.endpoint(
                &node.repo,
                &node.push_url,
                &node.remote,
                &["push", "--porcelain", "--"],
                &[&format!("{}:refs/heads/{}", node.source, node.target)],
            )?;
            if !reachable(runner, node, &budget)? {
                return Err(crate::i18n::text("backend-push-succeeded-but-the-required-commits-are-still-unreachable-fro-fd4428").into());
            }
        }
        Ok(crate::i18n::message(
            "backend-required-commits-are-published-on-0c6132",
            &[
                ("remote", (node.remote).to_string().into()),
                (
                    "crate",
                    (crate::git::redact(&node.fetch_url, None))
                        .to_string()
                        .into(),
                ),
            ],
        ))
    });
    plan.parent_result = Some(if failed {
        Err(crate::i18n::text(
            "backend-parent-not-pushed-because-a-submodule-dependency-failed-e7344b",
        )
        .into())
    } else {
        plan.snapshot.check_publication(runner.repo)?;
        Ok(crate::i18n::text(
            "backend-dependencies-published-parent-awaits-push-preflight-and-outgoing-2a9d8d",
        )
        .into())
    });
    Ok(plan)
}

fn publish_in_order(
    dependencies: &mut [Dependency],
    mut publish: impl FnMut(&Dependency) -> Result<String, String>,
) -> bool {
    dependencies.sort_by_key(|n| std::cmp::Reverse(n.path.matches('/').count()));
    let mut failed = false;
    for node in dependencies {
        let outcome = if failed {
            Err(crate::i18n::text("backend-not-pushed-an-earlier-dependency-failed-279cf5").into())
        } else {
            publish(node)
        };
        failed |= outcome.is_err();
        node.result = Some(outcome);
    }
    failed
}

#[cfg(test)]
mod parent_continuation_recording;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod cleanup_recording;

mod advertisement;
#[cfg(test)]
use advertisement::Advertisement;
use advertisement::inspect_node;
use advertisement::reachable;
