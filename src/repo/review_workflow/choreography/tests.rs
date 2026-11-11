use super::*;
fn sources() -> Vec<Source> {
    ["backend", "sdk"]
        .iter()
        .map(|name| Source {
            name: (*name).into(),
            repo: PathBuf::from("/recording"),
            stages: (0..2)
                .map(|i| Stage {
                    label: i.to_string(),
                    commit: i.to_string(),
                    tree: i.to_string(),
                    generated_index: None,
                })
                .collect(),
        })
        .collect()
}
#[test]
fn rehearsed_cross_repo_behavior_derives_a_passing_order() -> Result<(), String> {
    let backend = git::stack::test_repo::TestRepo::new("release-backend")?;
    let sdk = git::stack::test_repo::TestRepo::new("release-sdk")?;
    let b0 = backend.commit("backend.txt", "old\n", "deployed")?;
    let b1 = backend.commit("backend.txt", "dual\n", "bridge")?;
    let s0 = sdk.commit("sdk.txt", "old\n", "deployed")?;
    let s1 = sdk.commit("sdk.txt", "new\n", "switch")?;
    let plan=Plan{components:vec![
            Component{name:"backend".into(),repo:backend.path().to_string_lossy().into(),revisions:format!("deployed={b0}\nbridge={b1}")},
            Component{name:"sdk".into(),repo:sdk.path().to_string_lossy().into(),revisions:format!("deployed={s0}\nnext={s1}")}
        ],rules:vec![],command:r#"test "$(cat "$GITCRAFT_COMPONENT_SDK/sdk.txt")" != new || test "$(cat "$GITCRAFT_COMPONENT_BACKEND/backend.txt")" = dual"#.into(),trusted:true};
    let report = run(backend.path(), &plan, true, &git::RunStop::default())?;
    assert_eq!(report.states.len(), 4);
    assert_eq!(
        report
            .states
            .iter()
            .filter(|s| s.observed.as_ref().is_some_and(|o| o.code == Some(1)))
            .count(),
        1
    );
    assert_eq!(report.route, Some(vec![vec![0, 0], vec![1, 0], vec![1, 1]]));
    assert_eq!(backend.tip("HEAD")?, b1);
    assert_eq!(sdk.tip("HEAD")?, s1);
    assert!(report.cleanup.is_empty());
    Ok(())
}
#[test]
fn ordering_rules_and_observed_failures_control_the_route() {
    let sources = sources();
    let rules = vec![Rule {
        dependent: "sdk".into(),
        at: 1,
        prerequisite: "backend".into(),
        min: 1,
        reason: "dual protocol first".into(),
    }];
    let mut states = combinations(&sources, &rules);
    assert_eq!(
        route(&states, &sources, false),
        Some(vec![vec![0, 0], vec![1, 0], vec![1, 1]])
    );
    assert!(route(&states, &sources, true).is_none());
    for s in &mut states {
        if s.allowed {
            s.observed = Some(Observation {
                commit: "pinned".into(),
                tree: "combination".into(),
                command: "integration".into(),
                code: Some(if s.stages == [1, 0] { 1 } else { 0 }),
                stopped: false,
                lines: vec![],
                machine: "recording".into(),
            });
        }
    }
    assert!(route(&states, &sources, true).is_none());
    if let Some(state) = states.iter_mut().find(|s| s.stages == [1, 0])
        && let Some(o) = &mut state.observed
    {
        o.code = Some(0);
    }
    assert!(route(&states, &sources, true).is_some());
}
#[test]
fn command_environment_names_cannot_inject_shell_syntax() {
    assert!(valid_name("backend_v2"));
    for bad in ["", "2sdk", "SDK;rm", "$(command)", "SDK-NEXT"] {
        assert!(!valid_name(bad));
    }
}
