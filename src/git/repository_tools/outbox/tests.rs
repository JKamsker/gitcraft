use super::*;
fn queue(repo: &Path, view: &View, automatic: bool) -> Result<String, String> {
    enqueue(repo, view, automatic, &UserProfile::default(), None)
}
fn fixture(
    name: &str,
) -> Result<(git::fixture::Repo, git::fixture::Repo, std::path::PathBuf), String> {
    let r = git::fixture::Repo::new(name)?;
    r.commit("base", &[])?;
    let host = git::fixture::Repo::new(&format!("{name}-host"))?;
    let bare = host.path().join("host.git");
    std::fs::create_dir(&bare).map_err(|e| e.to_string())?;
    git::run(Some(&bare), &["init", "--bare"])?;
    r.git(&["remote", "add", "origin", &bare.to_string_lossy()])?;
    Ok((r, host, bare))
}
#[test]
fn durable_exact_oid_excludes_later_commits_and_refuses_duplicate_send() -> Result<(), String> {
    let (r, _host, bare) = fixture("outbox-exact")?;
    let queued = r.rev("HEAD")?;
    queue(r.path(), &read(r.path())?, false)?;
    r.commit("later", &[])?;
    let view = read(r.path())?;
    let e = &view.queue.entries[0];
    assert_eq!(e.commit, queued);
    let p = prepare(r.path(), &e.id, UserProfile::default(), None)?;
    let control = git::OpControl::new(|| {});
    let runner = Runner {
        repo: r.path(),
        control: &control,
        credentials: None,
    };
    send(
        &runner,
        &p,
        false,
        &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
    )?;
    let remote = git::run(Some(&bare), &["rev-parse", &p.plan.destination])?;
    assert_eq!(remote.trim(), queued);
    assert!(
        send(
            &runner,
            &p,
            false,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
        )
        .is_err()
    );
    assert_eq!(read(r.path())?.queue.entries[0].status, Status::Sent);
    Ok(())
}
#[test]
fn rewrites_and_changed_destinations_pause_without_any_push() -> Result<(), String> {
    let (r, _host, _bare) = fixture("outbox-pauses")?;
    let base = r.rev("HEAD")?;
    r.commit("queued", &[])?;
    queue(r.path(), &read(r.path())?, true)?;
    let id = read(r.path())?.queue.entries[0].id.clone();
    r.git(&["reset", "--hard", &base])?;
    assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
    assert!(matches!(
        read(r.path())?.queue.entries[0].status,
        Status::Paused(_)
    ));
    queue(r.path(), &read(r.path())?, true)?;
    let id = read(r.path())?.queue.entries[1].id.clone();
    r.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        "https://example.invalid/new.git",
    ])?;
    assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
    assert!(matches!(
        read(r.path())?.queue.entries[1].status,
        Status::Paused(_)
    ));
    Ok(())
}
#[test]
fn stale_queue_writers_and_uncertain_sends_are_not_retried() -> Result<(), String> {
    let (r, _host, _bare) = fixture("outbox-cas")?;
    let stale = read(r.path())?;
    queue(r.path(), &stale, true)?;
    assert!(queue(r.path(), &stale, false).is_err());
    let mut view = read(r.path())?;
    let id = view.queue.entries[0].id.clone();
    view.queue.entries[0].status = Status::Sending;
    save(r.path(), &view, None)?;
    assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
    assert!(remove(r.path(), &read(r.path())?, &id).is_err());
    Ok(())
}
#[test]
fn opted_in_clean_send_and_reviewed_destination_change_keep_the_exact_pin() -> Result<(), String> {
    let (r, _host, bare) = fixture("outbox-renewed")?;
    let queued = r.rev("HEAD")?;
    queue(r.path(), &read(r.path())?, true)?;
    let id = read(r.path())?.queue.entries[0].id.clone();
    let p = prepare(r.path(), &id, UserProfile::default(), None)?;
    assert!(
        p.plan.findings.is_empty(),
        "fixture should have a clean plain preflight"
    );
    assert!(preflight::push_args(&p.plan)?.contains(&"--recurse-submodules=check".into()));
    let control = git::OpControl::new(|| {});
    let runner = Runner {
        repo: r.path(),
        control: &control,
        credentials: None,
    };
    send(
        &runner,
        &p,
        true,
        &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
    )?;
    assert_eq!(read(r.path())?.queue.entries[0].status, Status::Sent);
    r.commit("second", &[])?;
    let queued2 = r.rev("HEAD")?;
    queue(r.path(), &read(r.path())?, true)?;
    let id = read(r.path())?.queue.entries[1].id.clone();
    r.commit("later excluded", &[])?;
    let url = format!("{}/.", bare.display());
    r.git(&["remote", "set-url", "--push", "origin", &url])?;
    assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
    let p = review_prepare(r.path(), &id, UserProfile::default(), None)?;
    assert_eq!(p.view.queue.entries[1].commit, queued2);
    assert!(!p.view.queue.entries[1].autosend);
    assert!(
        send(
            &runner,
            &p,
            true,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
        )
        .is_err()
    );
    send(
        &runner,
        &p,
        false,
        &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None),
    )?;
    assert_eq!(
        git::run(Some(&bare), &["rev-parse", &p.plan.destination])?.trim(),
        queued2
    );
    assert_ne!(queued, queued2);
    Ok(())
}
#[test]
fn automatic_findings_pause_before_transport_and_remote_rejections_stay_paused()
-> Result<(), String> {
    let (r, _host, bare) = fixture("outbox-reject")?;
    queue(r.path(), &read(r.path())?, true)?;
    let id = read(r.path())?.queue.entries[0].id.clone();
    let mut p = prepare(r.path(), &id, UserProfile::default(), None)?;
    p.plan.findings.push(preflight::Finding {
        kind: preflight::Kind::Message,
        commit: p.plan.head.clone(),
        path: String::new(),
        detail: "Review required".into(),
    });
    let control = git::OpControl::new(|| {});
    let runner = Runner {
        repo: r.path(),
        control: &control,
        credentials: None,
    };
    assert!(
        send(
            &runner,
            &p,
            true,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
        )
        .is_err()
    );
    assert!(matches!(
        read(r.path())?.queue.entries[0].status,
        Status::Paused(_)
    ));
    assert!(git::resolve(&bare, &p.plan.destination).is_err());
    let p = review_prepare(r.path(), &id, UserProfile::default(), None)?;
    // A receiver changes after inspection. Its new ref violates the absent-ref lease.
    let tree = git::run(Some(&bare), &["mktree"])?;
    let remote = git::run(
        Some(&bare),
        &[
            "-c",
            "user.name=Receiver",
            "-c",
            "user.email=receiver@example.com",
            "-c",
            "commit.gpgSign=false",
            "commit-tree",
            tree.trim(),
            "-m",
            "Other work",
        ],
    )?;
    git::run(
        Some(&bare),
        &["update-ref", &p.plan.destination, remote.trim()],
    )?;
    assert!(
        send(
            &runner,
            &p,
            false,
            &preflight::ReviewedSend::from_echo(preflight::push_args(&p.plan)?, None)
        )
        .is_err()
    );
    let e = read(r.path())?.queue.entries.remove(0);
    assert!(matches!(e.status, Status::Paused(_)));
    assert!(!e.receipt.is_empty());
    assert_eq!(
        git::run(Some(&bare), &["rev-parse", &p.plan.destination])?.trim(),
        remote.trim()
    );
    assert!(prepare(r.path(), &id, UserProfile::default(), None).is_err());
    Ok(())
}
