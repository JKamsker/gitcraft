use super::*;
fn steps(args: &[&str]) -> Vec<Vec<String>> {
    vec![args.iter().map(|s| (*s).into()).collect()]
}
#[test]
fn merge_pure_reviewed_echo_uses_exact_source_and_retains_dispatch_authorization()
-> Result<(), String> {
    let approved = vec![crate::push_preflight::APPROVED.into(), "prepared-id".into()];
    let original = vec![
        "push".into(),
        "--force-with-lease=refs/heads/main:old".into(),
        "--".into(),
        "original-remote".into(),
        format!("{}:refs/heads/main", "a".repeat(40)),
    ];
    let evidence = evidence_steps(std::slice::from_ref(&approved), |id| {
        assert_eq!(id, "prepared-id");
        Ok(original.clone())
    })
    .map_err(|error| error.to_string())?;
    assert_eq!(evidence, [original]);
    let bound = bind_step(
        &approved,
        vec!["-c".into(), "gitcraft.commandSource=retained".into()],
    );
    assert_eq!(&bound[..2], approved);
    assert_eq!(&bound[2..], ["-c", "gitcraft.commandSource=retained"]);
    assert!(contains_push(std::slice::from_ref(&bound)));
    let looked_up = std::cell::Cell::new(false);
    assert!(
        evidence_steps(&[bound], |_| {
            looked_up.set(true);
            Err("must refuse nested evidence before lookup".into())
        })
        .is_err()
    );
    assert!(
        !looked_up.get(),
        "must refuse nested evidence before lookup"
    );
    Ok(())
}
#[test]
fn r2_pure_echo_rejects_git_context_prefixes_and_retains_lease_options() {
    for prefix in [
        vec!["--git-dir=/other.git"],
        vec!["-C", "/other"],
        vec!["--namespace=other"],
        vec!["--config-env=remote.origin.url=OTHER"],
        vec!["-c", "core.worktree=/other"],
        vec!["-c", "url.foreign.insteadOf=https://selected"],
        vec!["--unknown-prefix"],
    ] {
        let prefix: Vec<_> = prefix.into_iter().map(str::to_owned).collect();
        assert!(validate_prefix(&prefix).is_err());
    }
    assert!(validate_prefix(&["-c".into(), "push.default=simple".into()]).is_ok());
    // The published source context is prefixed to the original argv: leases, force,
    // upstream behavior and porcelain source identity remain the original operation.
    assert!(contains_push(&steps(&[
        "push",
        "--force-with-lease=refs/heads/main:old",
        "--",
        "origin",
        "HEAD:refs/heads/main"
    ])));
    assert_eq!(conservative_base("empty-tree"), "empty-tree");
}
#[test]
#[ignore = "R2 runtime hold: exact source-after-queued-result and upstream fixture authored, not executed"]
fn r2_runtime_queued_echo_publishes_only_captured_oid_and_preserves_upstream() -> Result<(), String>
{
    let r = git::stack::test_repo::TestRepo::new("echo-delayed-source")?;
    let remote = git::stack::test_repo::TestRepo::new("echo-delayed-destination")?;
    r.git(&[
        "remote",
        "add",
        "team",
        remote.path().to_str().ok_or("path")?,
    ])?;
    let d = r.commit("handler.rs", "fn checked() {}\n", "checked D")?;
    let branch = r.git(&["symbolic-ref", "--short", "HEAD"])?;
    let destination = "refs/heads/reviewed";
    let spec = format!("HEAD:{destination}");
    let prepared = prepare(
        r.path(),
        &steps(&["push", "--porcelain", "-u", "team", &spec]),
    )?;
    assert_eq!(prepared.pairs.as_ref().map_err(Clone::clone)?[0].1, d);
    // The preflight result is ready; a terminal advances the original source before
    // the UI callback dispatches the operation. Its private source view remains D.
    let e = r.commit(
        "handler.rs",
        "fn unchecked(){value.unwrap();}\n",
        "unchecked E",
    )?;
    for step in &prepared.steps {
        git::run_unattended(
            Some(r.path()),
            &step.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
        )?;
    }
    assert_eq!(remote.tip(destination)?, d);
    assert_eq!(r.tip("HEAD")?, e);
    assert_eq!(
        r.git(&["config", &format!("branch.{}.remote", branch.trim())])?
            .trim(),
        "team"
    );
    assert_eq!(
        r.git(&["config", &format!("branch.{}.merge", branch.trim())])?
            .trim(),
        destination
    );
    Ok(())
}
#[test]
fn explicit_force_publish_wildcard_and_default_sources_are_pinned() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("echo-push-sources")?;
    let base = r.tip("HEAD")?;
    r.git(&["remote", "add", "origin", "https://invalid.example/repo"])?;
    r.git(&["update-ref", "refs/remotes/origin/release", &base])?;
    let head = r.commit("source.rs", "bad();\n", "outgoing")?;
    r.git(&["branch", "feature", &head])?;
    assert_eq!(
        sources(
            r.path(),
            &steps(&[
                "push",
                "--force-with-lease=refs/heads/release:old",
                "--",
                "origin",
                "refs/heads/feature:refs/heads/release"
            ])
        )?,
        vec![(
            git::run_input(
                Some(r.path()),
                &["hash-object", "-t", "tree", "--stdin"],
                ""
            )?
            .trim()
            .into(),
            head.clone()
        )]
    );
    let initial = sources(
        r.path(),
        &steps(&[
            "-c",
            "push.default=simple",
            "push",
            "-u",
            "origin",
            "refs/heads/feature",
        ]),
    )?;
    assert_eq!(initial[0].1, head);
    assert!(
        !sources(
            r.path(),
            &steps(&["push", "origin", "refs/heads/*:refs/heads/*"])
        )?
        .is_empty()
    );
    assert!(!sources(r.path(), &steps(&["push"]))?.is_empty());
    assert!(sources(r.path(), &steps(&["push", "--mirror", "origin"])).is_err());
    assert!(sources(r.path(), &steps(&["push", "--delete", "origin", "old"]))?.is_empty());
    let old = stamp(r.path())?;
    r.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        "https://invalid.example/other",
    ])?;
    assert_ne!(stamp(r.path())?, old);
    r.git(&[
        "config",
        "remote.origin.push",
        "refs/heads/feature:refs/heads/release",
    ])?;
    let mapped = sources(r.path(), &steps(&["push", "origin", "refs/heads/feature"]))?;
    assert_eq!(
        mapped[0].0,
        git::run_input(
            Some(r.path()),
            &["hash-object", "-t", "tree", "--stdin"],
            ""
        )?
        .trim()
    );
    assert_eq!(mapped[0].1, r.tip("refs/heads/feature")?);
    Ok(())
}

#[test]
fn ordinary_push_with_submodule_check_retains_echo_source_evidence() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("echo-ordinary-submodule-check")?;
    let remote = git::stack::test_repo::TestRepo::new("echo-ordinary-target")?;
    r.git(&[
        "remote",
        "add",
        "origin",
        remote.path().to_str().ok_or("path")?,
    ])?;
    let head = r.commit("handler.rs", "fn checked() {}\n", "checked source")?;
    let branch = r.git(&["symbolic-ref", "--short", "HEAD"])?;
    let spec = format!("HEAD:refs/heads/{}", branch.trim());
    let ordinary = crate::push_preflight::prepare_ordinary_push(
        r.path(),
        &steps(&["push", "--porcelain", "--", "origin", &spec]),
        &|| true,
    )?;
    let reviewed = prepare(r.path(), &ordinary)?;
    let pairs = reviewed.pairs?;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].1, head);
    // Checking submodules adds no child publication. Publishing them would require
    // separate source evidence and remains outside this ordinary root review.
    assert!(
        prepare(
            r.path(),
            &steps(&[
                "push",
                "--recurse-submodules=on-demand",
                "--",
                "origin",
                &spec
            ])
        )?
        .pairs
        .is_err()
    );
    Ok(())
}
