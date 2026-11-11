use super::*;
#[test]
fn r3_pure_sharing_selection_is_bound_to_the_frozen_endpoint_snapshot() -> Result<(), String> {
    use std::cell::RefCell;
    let observed = git::Remote {
        name: "origin".into(),
        url: "https://example.test/selected A".into(),
    };
    // Recording only: live selection was A, but capture consistently freezes B.
    // Even an internally unchanged B snapshot must not become the destination.
    let events = RefCell::new(Vec::new());
    let changed = capture_destination(
        &observed,
        || {
            events.borrow_mut().push("freeze B");
            Ok("https://example.test/replacement B")
        },
        |frozen, push| {
            assert!(!push, "must refuse before reading the replacement push URL");
            events.borrow_mut().push("frozen fetch B");
            Ok((*frozen).into())
        },
        |_| {
            events.borrow_mut().push("unexpected verification");
            Err("unchanged B cannot validate the observed A selection".into())
        },
    );
    assert!(
        changed
            .err()
            .ok_or("accepted replacement")?
            .contains("changed")
    );
    assert_eq!(*events.borrow(), ["freeze B", "frozen fetch B"]);

    // A split push URL is legitimate: select using frozen effective fetch semantics,
    // and retain the exact (possibly space-containing) effective endpoint strings.
    let push = "ssh://push.example.test/team repo";
    let (source, fetch, captured_push) = capture_destination(
        &observed,
        || Ok((observed.url.clone(), push.to_owned())),
        |source, push| Ok(if push { &source.1 } else { &source.0 }.clone()),
        |_| Ok(()),
    )?;
    assert_eq!(fetch, observed.url);
    assert_eq!(captured_push, push);
    assert_eq!(source, (fetch, captured_push));
    assert!(
        capture_destination(
            &observed,
            || Ok(observed.url.clone()),
            |source, _| Ok(source.clone()),
            |_| Err("context changed after capture".into()),
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn r2_pure_artifact_cache_import_is_atomic_cas_and_cannot_cross_source() -> Result<(), String> {
    let root = "refs/gitcraft/intent-remotes/source/";
    let old = "a".repeat(40);
    let new = "b".repeat(40);
    let transaction = cache_transaction(
        root,
        &[(format!("{root}old"), old.clone())],
        &[(format!("{root}new"), new.clone())],
    )?;
    assert!(transaction.starts_with("start\n"));
    assert!(transaction.ends_with("prepare\ncommit\n"));
    assert!(transaction.contains(&format!("update {root}new {new} {}", "0".repeat(40))));
    assert!(transaction.contains(&format!("delete {root}old {old}")));
    // A refresh replaces an existing live cache ref, even when the transfer's
    // captured private view originally had no such ref (or retained an older one).
    let reference = format!("{root}same");
    let refresh = cache_transaction(
        root,
        &[(reference.clone(), old.clone())],
        &[(reference.clone(), new.clone())],
    )?;
    assert_eq!(
        refresh,
        format!("start\nupdate {reference} {new} {old}\nprepare\ncommit\n")
    );
    assert!(
        cache_transaction(
            root,
            &[],
            &[("refs/gitcraft/intent-remotes/foreign/id".into(), new)]
        )
        .is_err()
    );
    Ok(())
}
#[test]
#[ignore = "R2 runtime hold: local Git routing/credential/ref fixture authored, not executed"]
fn r2_runtime_frozen_endpoint_survives_name_rewrite_and_profile_configuration_changes()
-> Result<(), String> {
    let source = git::stack::test_repo::TestRepo::new("frozen-artifact-source")?;
    let a = git::stack::test_repo::TestRepo::new("frozen-artifact-a")?;
    let b = git::stack::test_repo::TestRepo::new("frozen-artifact-b")?;
    let oid = source.tip("HEAD")?;
    source.git(&["remote", "add", "team", a.path().to_str().ok_or("path")?])?;
    let observed = git::remotes(source.path())?
        .into_iter()
        .find(|r| r.name == "team")
        .ok_or("remote")?;
    let captured = Target::capture(source.path(), &observed)?;
    let reference = "refs/gitcraft/recorded-artifact/owned";
    let spec = format!("{oid}:{reference}");
    // Construct the exact runner argv first, then change routing before the child runs.
    let args = captured
        .source
        .bind(&["push", "--", captured.push_url(), &spec]);
    source.git(&[
        "remote",
        "set-url",
        "team",
        b.path().to_str().ok_or("path")?,
    ])?;
    source.git(&[
        "config",
        &format!("url.{}.insteadOf", b.path().display()),
        a.path().to_str().ok_or("path")?,
    ])?;
    source.git(&[
        "config",
        &format!("url.{}.pushInsteadOf", b.path().display()),
        a.path().to_str().ok_or("path")?,
    ])?;
    let outcome = git::run_unattended(
        Some(source.path()),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        None,
    );
    assert!(outcome.is_err(), "worker must refuse changed configuration");
    assert!(a.git(&["rev-parse", "--verify", reference]).is_err());
    assert!(b.git(&["rev-parse", "--verify", reference]).is_err());
    assert!(captured.guard(source.path()).is_err());
    source.git(&["remote", "remove", "team"])?;
    let args = captured
        .source
        .bind(&["ls-remote", "--refs", captured.push_url(), reference]);
    assert!(
        git::run_unattended(
            Some(source.path()),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            None
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn replacement_remote_cannot_reuse_a_captured_destination() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("sharing-target")?;
    r.git(&["remote", "add", "team", "https://invalid.example/first"])?;
    let observed = git::remotes(r.path())?
        .into_iter()
        .find(|r| r.name == "team")
        .ok_or("remote")?;
    let captured = Target::capture(r.path(), &observed)?;
    r.git(&[
        "remote",
        "set-url",
        "team",
        "https://invalid.example/second",
    ])?;
    assert!(captured.guard(r.path()).is_err());
    assert!(Target::capture(r.path(), &observed).is_err());
    Ok(())
}
