use super::*;
#[test]
fn dependency_is_verified_on_named_host_before_parent_and_stale_plan_refuses() -> Result<(), String>
{
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
    let stopped = publish_children(&runner, wrong)?;
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
    let result = publish_children(&runner, p.clone())?;
    // A successful child phase still cannot publish the parent. The UI must
    // obtain root preflight/echo approval in a separate continuation.
    assert_eq!(
        crate::git::run(Some(&bare), &["rev-parse", "refs/heads/main"])?.trim(),
        initial
    );
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
    assert!(publish_children(&runner, p).is_err());
    Ok(())
}
