//! Named publication groups use independent ordinary pushes on every supported Git version.
use super::{
    guard::{Runner, Snapshot, text},
    remote_config::{boolean, set_values, validate_url},
};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug)]
pub(crate) struct Destination {
    pub remote: String,
    pub url: String,
    pub result: Option<Result<String, String>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub snapshot: Snapshot,
    pub group: String,
    pub commit: String,
    pub branch: String,
    pub destinations: Vec<Destination>,
}
#[derive(Clone, Debug)]
pub(crate) struct Manager {
    pub snapshot: Snapshot,
    pub groups: BTreeMap<String, Vec<String>>,
    pub remotes: Vec<String>,
    pub group: String,
    pub selected: Vec<String>,
    pub plan: Option<Plan>,
}
fn valid_group(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(crate) fn read(repo: &Path, group: &str) -> Result<Manager, String> {
    let config = text(repo, &["config", "--null", "--list"])?;
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for record in config.split('\0') {
        let Some((key, value)) = record.split_once('\n') else {
            continue;
        };
        if let Some(name) = key
            .strip_prefix("gitcraft-publication.")
            .and_then(|s| s.strip_suffix(".remote"))
            && valid_group(name)
        {
            groups.entry(name.into()).or_default().push(value.into());
        }
    }
    let remotes = text(repo, &["remote"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let selected = groups.get(group).cloned().unwrap_or_default();
    Ok(Manager {
        snapshot: Snapshot::capture(repo)?,
        groups,
        remotes,
        group: group.into(),
        selected,
        plan: None,
    })
}
pub(crate) fn save_group(runner: &Runner<'_>, manager: &Manager) -> Result<String, String> {
    manager.snapshot.check(runner.repo)?;
    if !valid_group(&manager.group)
        || manager.selected.is_empty()
        || manager
            .selected
            .iter()
            .any(|r| !manager.remotes.contains(r))
    {
        return Err("Choose a group name (letters, digits, - or _) and existing remotes".into());
    }
    set_values(
        runner,
        &format!("gitcraft-publication.{}.remote", manager.group),
        &manager.selected,
    )?;
    Ok(format!(
        "Saved publication group {}. Reopen Publish to several hosts to prepare its exact commit and URLs.",
        manager.group
    ))
}
pub(crate) fn prepare(repo: &Path, mut manager: Manager) -> Result<Manager, String> {
    manager.snapshot.check(repo)?;
    if manager.groups.get(&manager.group) != Some(&manager.selected) {
        return Err("Save the named publication group before preparing".into());
    }
    let branch = text(repo, &["symbolic-ref", "--short", "HEAD"])?
        .trim()
        .to_owned();
    let commit = text(repo, &["rev-parse", "--verify", "HEAD^{commit}"])?
        .trim()
        .to_owned();
    let mut destinations = Vec::new();
    for remote in &manager.selected {
        if boolean(repo, &format!("remote.{remote}.mirror"), false)? {
            return Err(format!(
                "{remote} is a mirror; publication groups only send the selected branch. Change its mirror policy first."
            ));
        }
        for url in text(
            repo,
            &["remote", "get-url", "--push", "--all", "--", remote],
        )?
        .lines()
        {
            validate_url(url)?;
            destinations.push(Destination {
                remote: remote.clone(),
                url: url.into(),
                result: None,
            });
        }
    }
    if destinations.is_empty() {
        return Err("Group has no publication destinations".into());
    }
    manager.plan = Some(Plan {
        snapshot: manager.snapshot.clone(),
        group: manager.group.clone(),
        commit,
        branch,
        destinations,
    });
    Ok(manager)
}
pub(crate) fn publish(
    runner: &Runner<'_>,
    mut manager: Manager,
    retry: bool,
) -> Result<Manager, String> {
    let plan = manager
        .plan
        .as_mut()
        .ok_or("Prepare this publication first")?;
    plan.snapshot.check_publication(runner.repo)?;
    for destination in &mut plan.destinations {
        if retry && !matches!(destination.result, Some(Err(_))) {
            continue;
        }
        if !retry && destination.result.is_some() {
            continue;
        }
        if runner.control.is_cancelled() {
            destination.result = Some(Err("Cancelled before this destination".into()));
            continue;
        }
        // Revalidate routing before each destination, including after other pushes/hooks.
        if let Err(e) = plan.snapshot.check_publication(runner.repo) {
            destination.result = Some(Err(e));
            continue;
        }
        destination.result = Some(runner.endpoint(
            runner.repo,
            &destination.url,
            &destination.remote,
            &["push", "--porcelain", "--recurse-submodules=check", "--"],
            &[&format!("{}:refs/heads/{}", plan.commit, plan.branch)],
        ));
    }
    Ok(manager)
}
#[cfg(test)]
mod tests {
    use super::*;
    // Compiled only under the runtime hold. The literal path "origin" must not
    // be interpreted as the configured remote of the same name, on either URL.
    #[test]
    fn multiple_urls_include_literal_remote_names_without_reinterpretation() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("publication-literal-source")?;
        let commit = r.commit("one", &[])?;
        let a = r.path().join("origin");
        let b = r.path().join("second.git");
        let decoy = r.path().join("decoy.git");
        for path in [&a, &b, &decoy] {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            crate::git::run(Some(path), &["init", "--bare"])?;
        }
        r.git(&["remote", "add", "origin", &decoy.to_string_lossy()])?;
        r.git(&["remote", "add", "configured", "origin"])?;
        r.git(&["config", "--add", "remote.configured.pushurl", "origin"])?;
        r.git(&[
            "config",
            "--add",
            "remote.configured.pushurl",
            &b.to_string_lossy(),
        ])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        let mut manager = read(r.path(), "hosts")?;
        manager.selected = vec!["configured".into()];
        save_group(&runner, &manager)?;
        let manager = prepare(r.path(), read(r.path(), "hosts")?)?;
        assert_eq!(
            manager.plan.as_ref().ok_or("No plan")?.destinations.len(),
            2
        );
        let result = publish(&runner, manager, false)?;
        assert!(
            result
                .plan
                .ok_or("No plan")?
                .destinations
                .iter()
                .all(|d| matches!(d.result, Some(Ok(_))))
        );
        for path in [&a, &b] {
            assert_eq!(
                crate::git::run(Some(path), &["rev-parse", "refs/heads/main"])?.trim(),
                commit
            );
        }
        assert!(
            crate::git::run(Some(&decoy), &["rev-parse", "--verify", "refs/heads/main"]).is_err()
        );
        Ok(())
    }
    // Owned-Git regression: compiled under the runtime hold, not executed.
    #[test]
    fn chained_push_rewrites_cannot_redirect_a_reviewed_publication() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("publication-rewrite-source")?;
        let commit = r.commit("one", &[])?;
        let hosts = crate::git::fixture::Repo::new("publication-rewrite-hosts")?;
        let a = hosts.path().join("a.git");
        let b = hosts.path().join("b.git");
        for path in [&a, &b] {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            crate::git::run(Some(path), &["init", "--bare"])?;
        }
        r.git(&["remote", "add", "first", "publication-route://first"])?;
        r.git(&[
            "config",
            &format!("url.{}.pushInsteadOf", a.display()),
            "publication-route://first",
        ])?;
        r.git(&[
            "config",
            &format!("url.{}.pushInsteadOf", b.display()),
            &a.to_string_lossy(),
        ])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        let mut manager = read(r.path(), "hosts")?;
        manager.selected = vec!["first".into()];
        save_group(&runner, &manager)?;
        let manager = prepare(r.path(), read(r.path(), "hosts")?)?;
        assert_eq!(
            manager.plan.as_ref().ok_or("No plan")?.destinations[0].url,
            a.to_string_lossy()
        );
        let result = publish(&runner, manager, false)?;
        assert!(matches!(
            result.plan.ok_or("No plan")?.destinations[0].result,
            Some(Ok(_))
        ));
        assert_eq!(
            crate::git::run(Some(&a), &["rev-parse", "refs/heads/main"])?.trim(),
            commit
        );
        assert!(crate::git::run(Some(&b), &["rev-parse", "--verify", "refs/heads/main"]).is_err());
        Ok(())
    }
    #[test]
    fn group_persists_reports_partial_failure_and_retries_only_failed() -> Result<(), String> {
        let r = crate::git::fixture::Repo::new("publication-source")?;
        let oid = r.commit("one", &[])?;
        let a = crate::git::fixture::Repo::new("publication-host-a")?;
        let host_a = a.path().join("host.git");
        std::fs::create_dir_all(&host_a).map_err(|e| e.to_string())?;
        crate::git::run(Some(&host_a), &["init", "--bare"])?;
        let b = crate::git::fixture::Repo::new("publication-host-b")?;

        r.git(&["remote", "add", "a", &host_a.to_string_lossy()])?;
        let missing = b.path().join("missing");
        r.git(&["remote", "add", "b", &missing.to_string_lossy()])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        let mut m = read(r.path(), "mirrors")?;
        m.selected = vec!["a".into(), "b".into()];
        save_group(&runner, &m)?;
        r.git(&["config", "remote.a.mirror", "TRUE"])?;
        assert!(prepare(r.path(), read(r.path(), "mirrors")?).is_err());
        r.git(&["config", "--unset", "remote.a.mirror"])?;
        let m = prepare(r.path(), read(r.path(), "mirrors")?)?;
        let mut m = publish(&runner, m, false)?;
        let p = m.plan.as_ref().ok_or("no plan")?;
        assert!(
            matches!(p.destinations[0].result, Some(Ok(_))),
            "{:?}",
            p.destinations[0].result
        );
        assert!(matches!(p.destinations[1].result, Some(Err(_))));
        assert_eq!(
            crate::git::run(Some(&host_a), &["rev-parse", "refs/heads/main"])?.trim(),
            oid
        );
        // A successful endpoint is made unavailable: failed-only retry must never contact it.
        m.plan.as_mut().ok_or("no plan")?.destinations[0].url = "/path/that/does/not/exist".into();
        std::fs::create_dir_all(&missing).map_err(|e| e.to_string())?;
        crate::git::run(Some(&missing), &["init", "--bare"])?;
        let m = publish(&runner, m, true)?;
        let p = m.plan.as_ref().ok_or("no plan")?;
        assert!(
            p.destinations
                .iter()
                .all(|d| matches!(d.result, Some(Ok(_))))
        );
        assert_eq!(
            crate::git::run(Some(&missing), &["rev-parse", "refs/heads/main"])?.trim(),
            oid
        );
        r.commit("new work", &[])?;
        assert!(publish(&runner, m, true).is_err());
        Ok(())
    }
}
