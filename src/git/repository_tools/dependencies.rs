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

/// Register immutable expected names/OIDs before fetch. Finalization is independent
/// of operation cancellation and deletes only those exact refs with expected OIDs.
struct Advertisement {
    namespace: String,
    refs: BTreeMap<String, String>,
}
impl Advertisement {
    fn cleanup(
        &self,
        mut run: impl FnMut(&[&str], Vec<u8>) -> Result<String, String>,
    ) -> Result<(), String> {
        let rows = run(
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                &self.namespace,
            ],
            Vec::new(),
        )?;
        let mut input = String::new();
        for row in rows.lines() {
            let (name, oid) = row
                .split_once(' ')
                .ok_or("Invalid owned dependency ref record")?;
            if self.refs.get(name).map(String::as_str) != Some(oid) {
                return Err("Dependency ref changed or is foreign; retained for inspection".into());
            }
            input.push_str(&format!("delete {name} {oid}\n"));
        }
        if !input.is_empty() {
            run(&["update-ref", "--stdin"], input.into_bytes())?;
        }
        Ok(())
    }
    fn finish(
        &self,
        fetch: Result<String, String>,
        cleanup: Result<(), String>,
    ) -> Result<Vec<String>, String> {
        match (fetch, cleanup) {
            (Ok(_), Ok(())) => Ok(self.refs.values().cloned().collect()),
            (Err(e), Ok(())) => Err(e),
            (fetch, Err(e)) => Err(format!(
                "{}; owned-ref cleanup failed: {e}. Inspect retained namespace {} before retrying; no parent push was attempted",
                fetch
                    .err()
                    .unwrap_or_else(|| "Dependency fetch completed".into()),
                self.namespace
            )),
        }
    }
}
fn advertised(
    runner: &Runner<'_>,
    node: &Dependency,
    budget: &super::ReadBudget,
) -> Result<Vec<String>, String> {
    budget.take()?;
    let rows = runner.endpoint(
        &node.repo,
        &node.fetch_url,
        &node.remote,
        &["ls-remote", "--refs", "--"],
        &["refs/heads/*", "refs/tags/*"],
    )?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let ns = format!(
        "refs/gitcraft/dependency-check/{stamp}-{}-{}",
        std::process::id(),
        CHECKS.fetch_add(1, Ordering::Relaxed)
    );
    let mut owned = Advertisement {
        namespace: ns,
        refs: BTreeMap::new(),
    };
    let mut specs = Vec::new();
    for row in rows.lines() {
        if specs.len() >= 256 {
            return Err("More than 256 advertised dependency refs; narrow this remote before checking publication".into());
        }
        let (oid, name) = row
            .split_once('\t')
            .ok_or("Invalid dependency advertisement")?;
        if !super::object::valid_oid(oid)
            || !(name.starts_with("refs/heads/") || name.starts_with("refs/tags/"))
        {
            return Err("Invalid advertised dependency ref".into());
        }
        let target = format!("{}/{}", owned.namespace, specs.len());
        specs.push(format!("+{oid}:{target}"));
        owned.refs.insert(target, oid.into());
    }
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    // Ownership finalizer exists before the first command capable of creating refs.
    budget.take()?;
    let fetch = runner.endpoint(
        &node.repo,
        &node.fetch_url,
        &node.remote,
        &[
            "fetch",
            "--no-recurse-submodules",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
        ],
        &specs.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let cleanup = crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
        owned.cleanup(|args, input| {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err("Cleanup deadline reached".into());
            }
            String::from_utf8(
                super::capture_with_timeout(&node.repo, args, input, 1 << 20, &[], None, left)?
                    .checked(&[0])?,
            )
            .map_err(|_| "Invalid cleanup metadata".into())
        })
    });
    let tips = owned.finish(fetch, cleanup)?;
    budget.take()?;
    Ok(tips)
}
fn reachable(
    runner: &Runner<'_>,
    node: &Dependency,
    budget: &super::ReadBudget,
) -> Result<bool, String> {
    let tips = advertised(runner, node, budget)?;
    for required in &node.required {
        let mut found = false;
        for tip in &tips {
            if ancestor(runner, budget, &node.repo, required, tip)? {
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
fn inspect_node(
    runner: &Runner<'_>,
    node: &mut Dependency,
    budget: &super::ReadBudget,
) -> Result<(), String> {
    let text = |repo: &Path, args: &[&str]| budget.text(repo, args, Some(runner.control.clone()));
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
    node.published = reachable(runner, node, budget)?;
    let refs = text(
        &node.repo,
        &[
            "for-each-ref",
            "--format=%(objectname)%00%(refname:strip=2)",
            "refs/heads/",
        ],
    )?;
    if refs.lines().count() > 256 {
        return Err("More than 256 candidate submodule branches; narrow the remote/branch selection before checking publication".into());
    }
    let mut source = None;
    for row in refs.lines() {
        budget.take()?;
        let Some((oid, branch)) = row.split_once('\0') else {
            continue;
        };
        let mut all = true;
        for needed in &node.required {
            if !ancestor(runner, budget, &node.repo, needed, oid)? {
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
            return Err(
                "More than 128 submodule dependencies; narrow this publication before continuing"
                    .into(),
            );
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
pub(crate) fn publish(runner: &Runner<'_>, plan: Plan) -> Result<Plan, String> {
    crate::tasks::with_cancellation(runner.control.cancellation(), || {
        publish_inner(runner, plan)
    })
}
fn publish_inner(runner: &Runner<'_>, mut plan: Plan) -> Result<Plan, String> {
    let budget = super::ReadBudget::new(2048, std::time::Duration::from_secs(300));
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
        runner.endpoint(
            runner.repo,
            &plan.parent_url,
            &plan.parent_remote,
            &["push", "--porcelain", "--recurse-submodules=check", "--"],
            &[&format!(
                "{}:refs/heads/{}",
                plan.commit, plan.parent_branch
            )],
        )
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

#[cfg(test)]
mod cleanup_recording {
    use super::*;
    #[test]
    fn canceled_fetch_still_cleans_exact_refs_and_retains_both_failures() -> Result<(), String> {
        let name = "refs/gitcraft/dependency-check/owned/0";
        let oid = "a".repeat(40);
        let owned = Advertisement {
            namespace: "refs/gitcraft/dependency-check/owned".into(),
            refs: [(name.into(), oid.clone())].into(),
        };
        let mut calls = Vec::new();
        let cleanup = owned.cleanup(|args, input| {
            calls.push((
                args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                input,
            ));
            if args[0] == "for-each-ref" {
                Ok(format!("{name} {oid}\n"))
            } else {
                Ok(String::new())
            }
        });
        assert!(
            owned
                .finish(Err("cancelled after creation".into()), cleanup)
                .err()
                .ok_or("Expected operation failure")?
                .contains("cancelled after creation")
        );
        assert_eq!(calls.len(), 2);
        assert_eq!(
            String::from_utf8(calls[1].1.clone()).map_err(|e| e.to_string())?,
            format!("delete {name} {oid}\n")
        );
        let empty = owned.cleanup(|_, _| Ok(String::new()));
        assert!(
            owned
                .finish(Err("cancelled before creation".into()), empty)
                .is_err()
        );
        let listing = owned.cleanup(|_, _| Err("listing failure".into()));
        let error = owned
            .finish(Err("fetch failed".into()), listing)
            .err()
            .ok_or("Expected operation failure")?;
        assert!(
            error.contains("fetch failed")
                && error.contains("listing failure")
                && error.contains(&owned.namespace)
        );
        let mut deletes = 0;
        assert!(
            owned
                .cleanup(|args, _| {
                    if args[0] == "for-each-ref" {
                        Ok(format!("{name} {}\n", "b".repeat(40)))
                    } else {
                        deletes += 1;
                        Ok(String::new())
                    }
                })
                .is_err()
        );
        assert_eq!(deletes, 0);
        let cleanup = owned.cleanup(|args, _| {
            if args[0] == "for-each-ref" {
                Ok(format!("{name} {oid}\n"))
            } else {
                Err("delete failure".into())
            }
        });
        assert!(
            owned
                .finish(Ok(String::new()), cleanup)
                .err()
                .ok_or("Expected operation failure")?
                .contains("delete failure")
        );
        Ok(())
    }
}
