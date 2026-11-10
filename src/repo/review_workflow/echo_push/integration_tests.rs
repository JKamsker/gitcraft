use super::*;

#[test]
fn selected_continuation_recording_echo_reviews_the_registered_push_and_keeps_its_authorization()
-> Result<(), String> {
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: "a".repeat(40),
        remote: "publish-host".into(),
        destination: "refs/heads/release".into(),
        urls: vec!["https://publish.invalid/team/repo".into()],
        lease: Some("b".repeat(40)),
    };
    let mut plan = crate::push_preflight::selected_continuation_recording::plan(&route);
    plan.retain_selection(git::repository_tools::routes::PreparedPush {
        snapshot: git::repository_tools::guard::Snapshot::recording(),
        route: route.clone(),
    })?;
    let expected = crate::push_preflight::push_args(&plan)?;
    let ops = crate::push_preflight::approved_ops(plan);
    let evidence = evidence_steps(&ops, |id| {
        let registered = crate::push_preflight::registered(id)?;
        crate::push_preflight::push_args(&registered)
    })?;
    assert_eq!(evidence, vec![ops[0].clone(), expected]);
    let approved = ops.last().ok_or("APPROVED handback missing")?;
    let marker = vec![
        "-c".into(),
        "gitcraft.commandSource=recorded-selected-source".into(),
    ];
    let bound = bind_step(approved, marker.clone());
    assert_eq!(&bound[..approved.len()], approved);
    assert_eq!(&bound[approved.len()..], marker);
    assert!(contains_push(std::slice::from_ref(&bound)));
    Ok(())
}

#[test]
fn merge_pure_approved_echo_keeps_plan_and_pinned_evidence() -> Result<(), String> {
    let approved = vec![
        crate::push_preflight::APPROVED.to_owned(),
        "recorded-plan".into(),
    ];
    let guard = crate::repo::operations::expect_ref_at("refs/heads/main", &"a".repeat(40));
    let original = vec![
        "push".to_owned(),
        "--recurse-submodules=no".into(),
        "--".into(),
        "team".into(),
        format!("{}:refs/heads/main", "a".repeat(40)),
    ];
    let mut calls = 0;
    let evidence = evidence_steps(&[guard.clone(), approved.clone()], |id| {
        calls += 1;
        assert_eq!(id, "recorded-plan");
        Ok(original.clone())
    })?;
    assert_eq!(calls, 1);
    assert_eq!(evidence, vec![guard, original.clone()]);
    let marker = vec!["-c".into(), "gitcraft.commandSource=recorded-source".into()];
    let bound = bind_step(&approved, marker.clone());
    assert_eq!(bound, [approved, marker.clone()].concat());
    assert!(contains_push(std::slice::from_ref(&bound)));
    assert!(
        evidence_steps(std::slice::from_ref(&bound), |_| Err(
            "must not recapture".into()
        ))
        .is_err()
    );
    assert_eq!(
        bind_step(&original, marker.clone()),
        [marker, original].concat()
    );
    Ok(())
}
