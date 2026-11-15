use super::*;
use crate::git::fixture::Repo;

fn fixture(name: &str) -> Result<(Repo, Vec<Repo>), String> {
    let repo = Repo::new(name)?;
    repo.commit("base", &[])?;
    let mut hosts = Vec::new();
    for (index, remote) in ["origin", "alice", "nested/alias"].into_iter().enumerate() {
        let host = Repo::new(&format!("{name}-host-{index}"))?;
        host.commit(&format!("remote-{index}"), &[])?;
        host.git(&["branch", "topic"])?;
        host.git(&["clone", "--bare", ".", "host.git"])?;
        repo.git(&[
            "remote",
            "add",
            remote,
            &host.path().join("host.git").to_string_lossy(),
        ])?;
        repo.git(&["fetch", remote])?;
        hosts.push(host);
    }
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ])?;
    repo.git(&["branch", "origin/topic"])?;
    Ok((repo, hosts))
}
fn run(repo: &Repo, ops: &[Vec<String>]) -> Result<(), String> {
    let control = crate::git::OpControl::new(|| {});
    for op in ops {
        execute(repo.path(), &op[1], Some(&control))?;
    }
    Ok(())
}
#[test]
fn fetched_catalog_and_exact_multivalue_tracking_undo() -> Result<(), String> {
    let (repo, _host) = fixture("tracking-multivalue")?;
    for value in [".", "origin"] {
        repo.git(&["config", "--add", "branch.main.remote", value])?;
    }
    for value in ["refs/heads/old", "refs/heads/another"] {
        repo.git(&["config", "--add", "branch.main.merge", value])?;
    }
    repo.git(&["config", "branch.main.pushRemote", "alice"])?;
    let before = values(repo.path(), "main")?;
    let view = read(repo.path(), "main")?;
    assert_eq!(view.choices.len(), 6);
    let plan = view.plan(Some("refs/remotes/alice/topic"))?;
    run(&repo, &plan.ops)?;
    assert_eq!(values(repo.path(), "main")?.remote, ["alice"]);
    assert_eq!(values(repo.path(), "main")?.merge, ["refs/heads/topic"]);
    run(&repo, &plan.undo)?;
    assert_eq!(values(repo.path(), "main")?, before);
    run(&repo, &plan.redo)?;
    assert_eq!(values(repo.path(), "main")?.remote, ["alice"]);
    assert_eq!(
        repo.git(&["config", "branch.main.pushRemote"])?.trim(),
        "alice"
    );
    let unset = read(repo.path(), "main")?.plan(None)?;
    run(&repo, &unset.ops)?;
    assert_eq!(
        values(repo.path(), "main")?,
        Values {
            remote: vec![],
            merge: vec![]
        }
    );
    run(&repo, &unset.undo)?;
    assert_eq!(values(repo.path(), "main")?.remote, ["alice"]);
    Ok(())
}
#[test]
fn stale_branch_remote_target_or_prior_tracking_refuses_before_mutation() -> Result<(), String> {
    for change in ["branch", "url", "target", "prior", "removed", "renamed"] {
        let (repo, _host) = fixture(&format!("tracking-stale-{change}"))?;
        let view = read(repo.path(), "main")?;
        assert!(view.plan(Some("refs/remotes/origin/absent")).is_err());
        let plan = view.plan(Some("refs/remotes/origin/topic"))?;
        match change {
            "branch" => {
                repo.commit("moved", &[])?;
            }
            "url" => {
                repo.git(&[
                    "remote",
                    "set-url",
                    "origin",
                    "https://example.invalid/changed",
                ])?;
            }
            "target" => {
                let moved = repo.git(&[
                    "commit-tree",
                    "HEAD^{tree}",
                    "-p",
                    "HEAD",
                    "-m",
                    "remote moved",
                ])?;
                repo.git(&["update-ref", "refs/remotes/origin/topic", moved.trim()])?;
            }
            "prior" => {
                repo.git(&["config", "branch.main.remote", "."])?;
            }
            "renamed" => {
                repo.git(&["remote", "rename", "origin", "changed"])?;
            }
            _ => {
                repo.git(&["remote", "remove", "origin"])?;
            }
        }
        let before = values(repo.path(), "main")?;
        assert!(run(&repo, &plan.ops).is_err(), "{change}");
        assert_eq!(values(repo.path(), "main")?, before);
    }
    Ok(())
}
#[test]
fn dot_tracking_and_missing_upstream_can_be_unset_and_restored() -> Result<(), String> {
    let (repo, _host) = fixture("tracking-dot-gone")?;
    for (remote, merge) in [
        (".", "refs/heads/origin/topic"),
        ("origin", "refs/heads/gone"),
    ] {
        repo.git(&["config", "branch.main.remote", remote])?;
        repo.git(&["config", "branch.main.merge", merge])?;
        let prior = values(repo.path(), "main")?;
        let view = read(repo.path(), "main")?;
        if remote == "." {
            assert_eq!(view.current_full, "refs/heads/origin/topic");
        }
        let plan = view.plan(None)?;
        run(&repo, &plan.ops)?;
        run(&repo, &plan.undo)?;
        assert_eq!(values(repo.path(), "main")?, prior);
    }
    Ok(())
}

#[test]
fn unresolvable_raw_tracking_can_be_unset_and_restored() -> Result<(), String> {
    let (repo, _hosts) = fixture("tracking-unresolvable")?;
    repo.git(&["config", "branch.main.remote", "origin"])?;
    repo.git(&["config", "branch.main.merge", "refs/heads/topic"])?;
    repo.git(&["config", "--unset-all", "remote.origin.fetch"])?;
    let before = values(repo.path(), "main")?;
    let view = read(repo.path(), "main")?;
    assert!(view.current.is_empty());
    assert!(view.has_tracking);
    let plan = view.plan(None)?;
    run(&repo, &plan.ops)?;
    assert!(!read(repo.path(), "main")?.has_tracking);
    run(&repo, &plan.undo)?;
    assert_eq!(values(repo.path(), "main")?, before);
    Ok(())
}
#[test]
fn included_tracking_is_blocked_for_set_unset_and_stale_execution() -> Result<(), String> {
    let (repo, _hosts) = fixture("tracking-included")?;
    let stale = read(repo.path(), "main")?.plan(Some("refs/remotes/alice/topic"))?;
    let included = repo.path().join(".git/tracking-include");
    std::fs::write(
        &included,
        "[branch \"main\"]\nremote = origin\nmerge = refs/heads/topic\n",
    )
    .map_err(|e| e.to_string())?;
    repo.git(&["config", "include.path", &included.to_string_lossy()])?;
    for local in [false, true] {
        if local {
            repo.git(&["config", "branch.main.remote", "alice"])?;
            repo.git(&["config", "branch.main.merge", "refs/heads/main"])?;
        }
        let view = read(repo.path(), "main")?;
        assert!(view.has_tracking);
        assert!(
            view.saving_blocked
                .as_deref()
                .is_some_and(|why| why.contains("branch.main."))
        );
        assert!(view.plan(None).is_err());
        assert!(view.plan(Some("refs/remotes/alice/topic")).is_err());
        let before = config(repo.path())?;
        assert!(run(&repo, &stale.ops).is_err());
        assert_eq!(config(repo.path())?, before);
    }
    Ok(())
}
#[test]
fn leading_dash_and_unicode_local_branch_tracking_roundtrips() -> Result<(), String> {
    let (repo, _hosts) = fixture("tracking-unicode-dash")?;
    let branch = "-töpíc";
    repo.git(&["update-ref", &format!("refs/heads/{branch}"), "HEAD"])?;
    let plan = read(repo.path(), branch)?.plan(Some("refs/remotes/alice/topic"))?;
    run(&repo, &plan.ops)?;
    assert_eq!(values(repo.path(), branch)?.remote, ["alice"]);
    run(&repo, &plan.undo)?;
    assert!(!read(repo.path(), branch)?.has_tracking);
    Ok(())
}
