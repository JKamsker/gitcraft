use super::{
    guard::{Runner, Snapshot, text},
    remote_config::{set_values, values},
};
use std::path::Path;
#[derive(Clone, Debug)]
pub(crate) struct Route {
    pub remote: String,
    pub branch: String,
    pub counts: Option<(u64, u64)>,
}
#[derive(Clone, Debug)]
pub(crate) struct Branch {
    pub name: String,
    pub oid: String,
    pub pull: Route,
    pub push: Route,
    pub pull_remote: String,
    pub pull_branch: String,
    pub push_remote: String,
    pub push_branch: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Browser {
    pub snapshot: Snapshot,
    pub branches: Vec<Branch>,
    pub remotes: Vec<String>,
    pub selected: String,
    pub choices: Vec<(String, String)>,
}
fn last(repo: &Path, key: &str) -> Result<String, String> {
    Ok(values(repo, key)?.last().cloned().unwrap_or_default())
}
fn mapping(pattern: &str, replacement: &str, value: &str) -> Option<String> {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        Some(replacement.replace('*', value.strip_prefix(prefix)?.strip_suffix(suffix)?))
    } else {
        (pattern == value).then(|| replacement.into())
    }
}
fn tracking(repo: &Path, remote: &str, branch: &str) -> Result<Option<String>, String> {
    let source = format!("refs/heads/{branch}");
    let specs = values(repo, &format!("remote.{remote}.fetch"))?;
    if specs
        .iter()
        .filter_map(|s| s.strip_prefix('^'))
        .any(|s| mapping(s, "*", &source).is_some())
    {
        return Ok(None);
    }
    Ok(specs
        .iter()
        .filter_map(|s| s.trim_start_matches('+').split_once(':'))
        .find_map(|(a, b)| mapping(a, b, &source)))
}
fn route(repo: &Path, oid: &str, remote: String, branch: String) -> Result<Route, String> {
    let counts = if remote.is_empty() || branch.is_empty() {
        None
    } else if let Some(refname) = tracking(repo, &remote, &branch)? {
        let resolved =
            crate::git::run_captured(repo, &["rev-parse", "--verify", "-q", &refname], None)?;
        if resolved.code == Some(0) {
            let range = format!("{oid}...{}", resolved.stdout.trim());
            let out = text(repo, &["rev-list", "--left-right", "--count", &range])?;
            let fields: Vec<_> = out.split_whitespace().collect();
            if fields.len() == 2 {
                Some((
                    fields[0].parse().map_err(|_| "Invalid ahead count")?,
                    fields[1].parse().map_err(|_| "Invalid behind count")?,
                ))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    Ok(Route {
        remote,
        branch,
        counts,
    })
}
pub(crate) fn read(repo: &Path, selected: &str) -> Result<Browser, String> {
    let remotes: Vec<String> = text(repo, &["remote"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let rows = text(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:strip=2)%00%(objectname)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(push:remotename)%00%(push:remoteref)",
            "refs/heads/",
        ],
    )?;
    let mut branches = Vec::new();
    for row in rows.lines() {
        let f: Vec<_> = row.split('\0').collect();
        if f.len() != 6 {
            return Err("Invalid route metadata".into());
        }
        let name = f[0];
        let oid = f[1];
        let pull = route(
            repo,
            oid,
            f[2].into(),
            f[3].strip_prefix("refs/heads/").unwrap_or(f[3]).into(),
        )?;
        let configured = last(repo, &format!("branch.{name}.gitcraftPushBranch"))?;
        let mut push_remote = last(repo, &format!("branch.{name}.pushRemote"))?;
        if push_remote.is_empty() {
            push_remote = last(repo, "remote.pushDefault")?;
        }
        if push_remote.is_empty() {
            push_remote = if !f[4].is_empty() {
                f[4].into()
            } else if !pull.remote.is_empty() {
                pull.remote.clone()
            } else if remotes.iter().any(|r| r == "origin") {
                "origin".into()
            } else if remotes.len() == 1 {
                remotes[0].clone()
            } else {
                String::new()
            };
        }
        let push_branch = if !configured.is_empty() {
            configured
        } else if !f[5].is_empty() {
            f[5].strip_prefix("refs/heads/").unwrap_or(f[5]).into()
        } else {
            name.into()
        };
        let push = route(repo, oid, push_remote, push_branch)?;
        branches.push(Branch {
            name: name.into(),
            oid: oid.into(),
            pull_remote: pull.remote.clone(),
            pull_branch: pull.branch.clone(),
            push_remote: push.remote.clone(),
            push_branch: push.branch.clone(),
            pull,
            push,
        });
    }
    let mut choices = Vec::new();
    for remote in &remotes {
        for spec in values(repo, &format!("remote.{remote}.fetch"))? {
            let Some((source, dest)) = spec.trim_start_matches('+').split_once(':') else {
                continue;
            };
            for row in text(
                repo,
                &["for-each-ref", "--format=%(refname)", "refs/remotes/"],
            )?
            .lines()
            {
                if let Some(source) = mapping(dest, source, row)
                    && let Some(branch) = source.strip_prefix("refs/heads/")
                {
                    choices.push((remote.clone(), branch.into()));
                }
            }
        }
    }
    Ok(Browser {
        snapshot: Snapshot::capture(repo)?,
        branches,
        remotes,
        selected: selected.into(),
        choices,
    })
}
pub(crate) fn save(runner: &Runner<'_>, browser: &Browser, name: &str) -> Result<String, String> {
    browser.snapshot.check(runner.repo)?;
    let b = browser
        .branches
        .iter()
        .find(|b| b.name == name)
        .ok_or("Branch disappeared")?;
    if !browser.remotes.contains(&b.pull_remote) || !browser.remotes.contains(&b.push_remote) {
        return Err("Pick existing pull and push remotes".into());
    }
    for branch in [&b.pull_branch, &b.push_branch] {
        text(
            runner.repo,
            &["check-ref-format", &format!("refs/heads/{branch}")],
        )?;
    }
    for (field, value) in [
        ("remote", &b.pull_remote),
        ("merge", &format!("refs/heads/{}", b.pull_branch)),
        ("pushRemote", &b.push_remote),
        ("gitcraftPushBranch", &b.push_branch),
    ] {
        set_values(
            runner,
            &format!("branch.{name}.{field}"),
            std::slice::from_ref(value),
        )?;
    }
    let after = read(runner.repo, name)?;
    let actual = after
        .branches
        .iter()
        .find(|v| v.name == name)
        .ok_or("Branch disappeared after saving")?;
    if actual.pull.remote != b.pull_remote
        || actual.pull.branch != b.pull_branch
        || actual.push.remote != b.push_remote
        || actual.push.branch != b.push_branch
    {
        return Err("Effective routes differ after saving. A higher priority include, worktree or profile value may still apply; reopen routes before pushing.".into());
    }
    Ok(format!(
        "{name}: pull from {}/{}; push to {}/{}",
        b.pull_remote, b.pull_branch, b.push_remote, b.push_branch
    ))
}
/// Only rewrites a plain current-branch push when the user explicitly picked its branch
/// route. Explicit, mirror, deletion, force and multi-ref pushes retain their own semantics.
pub(crate) fn rewrite_plain_push(
    repo: &Path,
    args: &[&str],
) -> Result<Option<Vec<String>>, String> {
    let mut at = 0;
    while args.get(at) == Some(&"-c") && args.get(at + 1).is_some() {
        at += 2;
    }
    if args.get(at) != Some(&"push")
        || args[at + 1..]
            .iter()
            .any(|a| !["--porcelain", "--progress", "--no-verify"].contains(a))
    {
        return Ok(None);
    }
    let branch = text(repo, &["symbolic-ref", "--short", "HEAD"])?
        .trim()
        .to_owned();
    let target = last(repo, &format!("branch.{branch}.gitcraftPushBranch"))?;
    if target.is_empty() {
        let mut checked: Vec<String> = args.iter().map(|s| (*s).into()).collect();
        checked.push("--recurse-submodules=check".into());
        return Ok(Some(checked));
    }
    let browser = read(repo, &branch)?;
    let route = browser
        .branches
        .iter()
        .find(|b| b.name == branch)
        .ok_or("Branch disappeared")?;
    if !browser.remotes.contains(&route.push.remote) {
        return Err("Configured push remote is unavailable; choose its route again".into());
    }
    text(repo, &["check-ref-format", &format!("refs/heads/{target}")])?;
    let mut rewritten: Vec<String> = args.iter().map(|s| (*s).into()).collect();
    rewritten.push("--recurse-submodules=check".into());
    rewritten.extend([
        "--".into(),
        route.push.remote.clone(),
        format!("refs/heads/{branch}:refs/heads/{target}"),
    ]);
    Ok(Some(rewritten))
}
pub(crate) fn push_branch(
    runner: &Runner<'_>,
    branch: &str,
    expected: &str,
) -> Result<String, String> {
    let browser = read(runner.repo, branch)?;
    let b = browser
        .branches
        .iter()
        .find(|b| b.name == branch)
        .ok_or("Branch disappeared")?;
    if b.oid != expected {
        return Err("Branch changed; choose Push again".into());
    }
    if !browser.remotes.contains(&b.push.remote) {
        return Err("Choose a push route first".into());
    }
    browser.snapshot.check(runner.repo)?;
    runner.run(&[
        "push",
        "--porcelain",
        "--recurse-submodules=check",
        "--",
        &b.push.remote,
        &format!("{}:refs/heads/{}", b.oid, b.push.branch),
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn force_route_pins_push_host_and_uses_its_advertised_lease() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("route-force-source")?;
        let first = r.commit("base", &[])?;
        let host = crate::git::fixture::Repo::new("route-force-host")?;
        let bare = host.path().join("host.git");
        std::fs::create_dir_all(&bare).map_err(|e| e.to_string())?;
        crate::git::run(Some(&bare), &["init", "--bare"])?;
        r.git(&[
            "remote",
            "add",
            "origin",
            "https://example.invalid/upstream.git",
        ])?;
        r.git(&["remote", "add", "fork", &bare.to_string_lossy()])?;
        r.git(&["push", "fork", &format!("{first}:refs/heads/dev")])?;
        let tip = r.commit("local", &[])?;
        r.git(&["push", "fork", &format!("{tip}:refs/heads/backup")])?;
        r.git(&["config", "branch.main.remote", "origin"])?;
        r.git(&["config", "branch.main.merge", "refs/heads/main"])?;
        r.git(&["config", "branch.main.pushRemote", "fork"])?;
        r.git(&["config", "branch.main.gitcraftPushBranch", "dev"])?;
        let p = prepare_force(r.path(), "main", &tip)?;
        assert_eq!(p.remote, "fork");
        assert_eq!(p.destination, "refs/heads/dev");
        assert_eq!(p.remote_commit, first);
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        force(&runner, &p)?;
        assert_eq!(
            crate::git::run(Some(&bare), &["rev-parse", "refs/heads/dev"])?.trim(),
            tip
        );
        let stale = prepare_force(r.path(), "main", &tip)?;
        crate::git::run(Some(&bare), &["update-ref", "refs/heads/dev", &first])?;
        assert!(force(&runner, &stale).is_err());
        Ok(())
    }
    #[test]
    fn triangular_routes_have_independent_counts_and_safe_pick_changes() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("triangular-routes")?;
        let base = r.commit("base", &[])?;
        let tip = r.commit("local", &[])?;
        for remote in ["origin", "fork"] {
            r.git(&["remote", "add", remote, "https://example.invalid/repo.git"])?;
        }
        r.git(&["update-ref", "refs/remotes/origin/main", &base])?;
        r.git(&["update-ref", "refs/remotes/fork/dev", &tip])?;
        r.git(&["branch", "--set-upstream-to=origin/main", "main"])?;
        r.git(&["config", "branch.main.pushRemote", "fork"])?;
        r.git(&["config", "branch.main.gitcraftPushBranch", "dev"])?;
        let b = read(r.path(), "main")?;
        assert_eq!(b.branches[0].pull.counts, Some((1, 0)));
        assert_eq!(b.branches[0].push.counts, Some((0, 0)));
        let rewritten = rewrite_plain_push(
            r.path(),
            &["-c", "core.hooksPath=/dev/null", "push", "--porcelain"],
        )?
        .ok_or("No route rewrite")?;
        assert_eq!(
            &rewritten[rewritten.len() - 2..],
            ["fork", "refs/heads/main:refs/heads/dev"]
        );
        assert!(rewrite_plain_push(r.path(), &["push", "origin", "main"])?.is_none());
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        save(&runner, &b, "main")?;
        r.commit("changed", &[])?;
        assert!(save(&runner, &b, "main").is_err());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Force {
    pub snapshot: Snapshot,
    pub branch: String,
    pub commit: String,
    pub remote: String,
    pub destination: String,
    pub url: String,
    pub remote_commit: String,
}
pub(crate) fn prepare_force(repo: &Path, branch: &str, expected: &str) -> Result<Force, String> {
    if let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
    let browser = read(repo, branch)?;
    let b = browser
        .branches
        .iter()
        .find(|b| b.name == branch)
        .ok_or("Branch disappeared")?;
    if b.oid != expected || !browser.remotes.contains(&b.push.remote) {
        return Err("Branch or push route changed; choose it again".into());
    }
    let urls = text(
        repo,
        &["remote", "get-url", "--push", "--all", "--", &b.push.remote],
    )?;
    let urls: Vec<_> = urls.lines().collect();
    if urls.len() != 1 {
        return Err("Force push requires exactly one push URL. Use the remote editor to choose it explicitly.".into());
    }
    super::remote_config::validate_url(urls[0])?;
    let destination = format!("refs/heads/{}", b.push.branch);
    let advertised = text(repo, &["ls-remote", "--refs", "--", urls[0], &destination])?;
    let remote_commit = advertised
        .lines()
        .find_map(|l| {
            l.split_once('\t')
                .filter(|(_, r)| *r == destination)
                .map(|(o, _)| o.to_owned())
        })
        .ok_or("Push branch does not exist on this destination; use an ordinary push")?;
    browser.snapshot.check(repo)?;
    Ok(Force {
        snapshot: browser.snapshot,
        branch: branch.into(),
        commit: expected.into(),
        remote: b.push.remote.clone(),
        destination,
        url: urls[0].into(),
        remote_commit,
    })
}
pub(crate) fn force(runner: &Runner<'_>, plan: &Force) -> Result<String, String> {
    plan.snapshot.check(runner.repo)?;
    runner.run(&[
        "push",
        "--porcelain",
        "--recurse-submodules=check",
        &format!(
            "--force-with-lease={}:{}",
            plan.destination, plan.remote_commit
        ),
        "--",
        &plan.url,
        &format!("{}:{}", plan.commit, plan.destination),
    ])
}
