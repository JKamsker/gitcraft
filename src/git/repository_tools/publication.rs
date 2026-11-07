//! Named publication groups use independent ordinary pushes on every supported Git version.
use super::{
    guard::{Runner, Snapshot, text},
    remote_config::{set_values, values},
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
        if values(repo, &format!("remote.{remote}.mirror"))?
            .last()
            .is_some_and(|s| matches!(s.as_str(), "true" | "yes" | "1" | "on"))
        {
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
            if url.starts_with('-') || url.contains("::") {
                return Err("Unsupported option-like URL or external transport helper".into());
            }
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
        destination.result = Some(runner.run(&[
            "push",
            "--porcelain",
            "--recurse-submodules=check",
            "--",
            &destination.url,
            &format!("{}:refs/heads/{}", plan.commit, plan.branch),
        ]));
    }
    Ok(manager)
}
#[cfg(test)]
mod tests {
    use super::*;
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
