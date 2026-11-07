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
            return Err("Invalid tree entry".into());
        };
        let f: Vec<_> = meta.split_whitespace().collect();
        if f.len() == 3 && f[0] == "160000" {
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Unsafe submodule path in Git tree".into());
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
        text(repo,&["cat-file","-e",&format!("{oid}^{{commit}}")]).map_err(|_|"Fetch the destination branch first; its tip is not available locally for dependency analysis".to_owned())?;
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
        let path = rows.next().ok_or("Incomplete raw gitlink record")?;
        let f: Vec<_> = meta.split_whitespace().collect();
        if f.len() != 5 {
            return Err("Invalid raw gitlink record".into());
        }
        if f[1] == "160000" {
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Unsafe submodule path in outgoing history".into());
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
        .ok_or("Current branch is unborn")?;
    if route.push.remote.is_empty() {
        return Err("Choose the parent's push route first".into());
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
        return Err("Dependency publication requires one explicit parent push URL; use a dedicated remote for this route".into());
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
    let out = runner.in_repo(repo, &["ls-remote", "--refs", "--", url, &name])?;
    Ok(out.lines().find_map(|l| {
        l.split_once('\t')
            .filter(|(_, r)| *r == name)
            .map(|(o, _)| o.into())
    }))
}
fn ancestor(repo: &Path, a: &str, b: &str) -> Result<bool, String> {
    let out = crate::git::run_captured(
        repo,
        &["--no-replace-objects", "merge-base", "--is-ancestor", a, b],
        None,
    )?;
    match out.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(out.stderr),
    }
}
/// A unique app-owned namespace prevents a remote's fetch/mirror refspecs from moving local
/// branches. Cleanup uses exact ref names and expected OIDs, never broad selectors.
fn advertised(runner: &Runner<'_>, node: &Dependency) -> Result<Vec<String>, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let ns = format!(
        "refs/gitcraft/dependency-check/{stamp}-{}-{}",
        std::process::id(),
        CHECKS.fetch_add(1, Ordering::Relaxed)
    );
    let fetch = runner.in_repo(
        &node.repo,
        &[
            "fetch",
            "--no-recurse-submodules",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
            &node.fetch_url,
            &format!("+refs/heads/*:{ns}/heads/*"),
            &format!("+refs/tags/*:{ns}/tags/*"),
        ],
    );
    let rows = text(
        &node.repo,
        &["for-each-ref", "--format=%(refname) %(objectname)", &ns],
    )?;
    let mut tips = Vec::new();
    let mut cleanup_error = None;
    for row in rows.lines() {
        let (name, oid) = row.split_once(' ').ok_or("Invalid dependency ref record")?;
        tips.push(oid.into());
        if let Err(e) = runner.in_repo(&node.repo, &["update-ref", "-d", name, oid]) {
            cleanup_error = Some(e);
        }
    }
    fetch?;
    if let Some(e) = cleanup_error {
        return Err(format!(
            "Could not remove owned dependency-check refs: {e}. No parent push was attempted."
        ));
    }
    Ok(tips)
}
fn reachable(runner: &Runner<'_>, node: &Dependency) -> Result<bool, String> {
    let tips = advertised(runner, node)?;
    for required in &node.required {
        let mut found = false;
        for tip in &tips {
            if ancestor(&node.repo, required, tip)? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}
fn inspect_node(runner: &Runner<'_>, node: &mut Dependency) -> Result<(), String> {
    if !text(&node.repo, &["rev-parse", "--show-prefix"])?
        .trim()
        .is_empty()
    {
        return Err(
            "Initialize this submodule first; Git currently resolves it as its parent repository"
                .into(),
        );
    }
    if !node.remotes.contains(&node.remote) {
        return Err("Choose a remote that collaborators can fetch from".into());
    }
    node.fetch_url = text(&node.repo, &["remote", "get-url", "--", &node.remote])?
        .trim_end_matches(['\n', '\r'])
        .into();
    let urls = text(
        &node.repo,
        &["remote", "get-url", "--push", "--all", "--", &node.remote],
    )?;
    let urls: Vec<_> = urls.lines().collect();
    if urls.len() != 1 {
        return Err("Choose a submodule remote with exactly one push URL".into());
    }
    node.push_url = urls[0].into();
    super::remote_config::validate_url(&node.fetch_url)?;
    super::remote_config::validate_url(&node.push_url)?;
    node.published = reachable(runner, node)?;
    let refs = text(
        &node.repo,
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:strip=2)",
            "refs/heads/",
        ],
    )?;
    let mut source = None;
    for row in refs.lines() {
        let Some((oid, branch)) = row.split_once('\0') else {
            continue;
        };
        let mut all = true;
        for needed in &node.required {
            if !ancestor(&node.repo, needed, oid)? {
                all = false;
                break;
            }
        }
        if all {
            source = Some((oid.to_owned(), branch.to_owned()));
            break;
        }
    }
    if node.required.len() == 1 {
        node.source = node
            .required
            .iter()
            .next()
            .cloned()
            .ok_or("No required commits")?;
        text(
            &node.repo,
            &["cat-file", "-e", &format!("{}^{{commit}}", node.source)],
        )?;
        if node.target.is_empty() {
            node.target = source
                .as_ref()
                .map(|(_, b)| b.clone())
                .unwrap_or_else(|| format!("gitcraft-dependency-{}", &node.source[..12]));
        }
    } else if let Some((oid, branch)) = source {
        node.source = oid;
        if node.target.is_empty() {
            node.target = branch;
        }
    } else if !node.published {
        return Err("No local commit contains all required gitlinks. Open this submodule, merge its required commits, and check again".into());
    }
    node.snapshot = Some(Snapshot::capture(&node.repo)?);
    Ok(())
}
pub(crate) fn check(runner: &Runner<'_>, mut plan: Plan) -> Result<Plan, String> {
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
        if plan.dependencies.len() > 128 {
            return Err(
                "More than 128 submodule dependencies; narrow this publication before continuing"
                    .into(),
            );
        }
        let node = &mut plan.dependencies[index];
        let outcome = inspect_node(runner, node);
        node.problem = outcome.err();
        if node.problem.is_none() {
            let mut nested = BTreeMap::<String, BTreeSet<String>>::new();
            for oid in &node.required {
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
pub(crate) fn publish(runner: &Runner<'_>, mut plan: Plan) -> Result<Plan, String> {
    if !plan.checked || plan.dependencies.iter().any(|n| n.problem.is_some()) {
        return Err("Check every dependency and resolve its problem before publishing".into());
    }
    plan.snapshot.check_publication(runner.repo)?;
    plan.dependencies
        .sort_by_key(|n| std::cmp::Reverse(n.path.matches('/').count()));
    let mut failed = false;
    for node in &mut plan.dependencies {
        if failed {
            node.result = Some(Err("Not pushed: an earlier dependency failed".into()));
            continue;
        }
        let outcome = (|| {
            node.snapshot
                .as_ref()
                .ok_or("Submodule snapshot missing")?
                .check_publication(&node.repo)?;
            if !reachable(runner, node)? {
                text(
                    &node.repo,
                    &["check-ref-format", &format!("refs/heads/{}", node.target)],
                )?;
                runner.in_repo(
                    &node.repo,
                    &[
                        "push",
                        "--porcelain",
                        "--",
                        &node.push_url,
                        &format!("{}:refs/heads/{}", node.source, node.target),
                    ],
                )?;
                if !reachable(runner, node)? {
                    return Err("Push succeeded, but the required commits are still unreachable from the named fetch URL. Parent publication stopped.".into());
                }
            }
            Ok(format!(
                "Required commits are published on {} ({})",
                node.remote,
                crate::git::redact(&node.fetch_url, None)
            ))
        })();
        failed = outcome.is_err();
        node.result = Some(outcome);
    }
    plan.parent_result = Some(if failed {
        Err("Parent not pushed because a submodule dependency failed".into())
    } else {
        plan.snapshot.check_publication(runner.repo)?;
        runner.run(&[
            "push",
            "--porcelain",
            "--recurse-submodules=check",
            "--",
            &plan.parent_url,
            &format!("{}:refs/heads/{}", plan.commit, plan.parent_branch),
        ])
    });
    Ok(plan)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_is_verified_on_named_host_before_parent_and_stale_plan_refuses()
    -> Result<(), String> {
        let donor = crate::git::fixture::Repo::new("dependency-donor")?;
        donor.commit("published base", &[])?;
        let host = crate::git::fixture::Repo::new("dependency-parent-host")?;
        let bare = host.path().join("host.git");
        std::fs::create_dir_all(&bare).map_err(|e| e.to_string())?;
        crate::git::run(Some(&bare), &["init", "--bare"])?;
        let child_host = host.path().join("sub.git");
        crate::git::run(
            None,
            &[
                "clone",
                "--bare",
                &donor.path().to_string_lossy(),
                &child_host.to_string_lossy(),
            ],
        )?;
        let parent = crate::git::fixture::Repo::new("dependency-parent")?;
        parent.git(&["remote", "add", "origin", &bare.to_string_lossy()])?;
        parent.git(&[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            &child_host.to_string_lossy(),
            "lib",
        ])?;
        let initial = parent.commit("initial parent", &[])?;
        parent.git(&["push", "-u", "origin", "main"])?;
        let child = parent.path().join("lib");
        crate::git::run(Some(&child), &["config", "user.name", "Tester"])?;
        crate::git::run(Some(&child), &["config", "user.email", "t@t"])?;
        crate::git::run(
            Some(&child),
            &[
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-m",
                "unpublished dependency",
            ],
        )?;
        let required = text(&child, &["rev-parse", "HEAD"])?.trim().to_owned();
        parent.git(&["add", "lib"])?;
        parent.git(&["commit", "-m", "parent depends on new child"])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: parent.path(),
            control: &control,
            credentials: None,
        };
        let wrong_host = host.path().join("wrong-host.git");
        std::fs::create_dir_all(&wrong_host).map_err(|e| e.to_string())?;
        crate::git::run(Some(&wrong_host), &["init", "--bare"])?;
        crate::git::run(
            Some(&child),
            &[
                "config",
                "remote.origin.pushurl",
                &wrong_host.to_string_lossy(),
            ],
        )?;
        let wrong = check(&runner, read(parent.path())?)?;
        let stopped = publish(&runner, wrong)?;
        assert!(matches!(stopped.parent_result, Some(Err(_))));
        assert!(matches!(stopped.dependencies[0].result, Some(Err(_))));
        assert_eq!(
            crate::git::run(Some(&bare), &["rev-parse", "refs/heads/main"])?.trim(),
            initial
        );
        crate::git::run(
            Some(&child),
            &["config", "--unset-all", "remote.origin.pushurl"],
        )?;
        let p = check(&runner, read(parent.path())?)?;
        assert!(p.checked);
        assert_eq!(p.dependencies.len(), 1);
        assert!(!p.dependencies[0].published, "{:?}", p.dependencies[0]);
        assert!(
            p.dependencies[0].problem.is_none(),
            "{:?}",
            p.dependencies[0].problem
        );
        assert_eq!(
            crate::git::run(Some(&bare), &["rev-parse", "refs/heads/main"])?.trim(),
            initial
        );
        let result = publish(&runner, p.clone())?;
        assert!(
            matches!(result.parent_result, Some(Ok(_))),
            "{:?}",
            result.parent_result
        );
        assert!(
            result
                .dependencies
                .iter()
                .all(|n| matches!(n.result, Some(Ok(_))))
        );
        assert_eq!(
            crate::git::run(Some(&child_host), &["rev-parse", "refs/heads/main"])?.trim(),
            required
        );
        parent.commit("changed afterwards", &[])?;
        assert!(publish(&runner, p).is_err());
        Ok(())
    }
}
