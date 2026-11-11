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
