use super::*;
use git::repository_tools::routes::{PreparedPush, PushRoute};

pub(crate) fn plan(route: &PushRoute) -> Plan {
    Plan {
        branch: format!("refs/heads/{}", route.branch),
        remote: route.remote.clone(),
        destination: route.destination.clone(),
        head: route.commit.clone(),
        base: route.lease.clone(),
        commits: Vec::new(),
        findings: Vec::new(),
        targets: route.urls.iter().map(|u| target_label(u)).collect(),
        expected_name: String::new(),
        expected_email: String::new(),
        expected_accounts: BTreeMap::new(),
        profile: UserProfile::default(),
        force: route.lease.is_some(),
        deleting: false,
        tracking: false,
        lfs_objects: BTreeSet::new(),
        skip_hooks: false,
        push_oid: route.commit.clone(),
        original: route.operations(),
        urls: route.urls.clone(),
        checkout: "refs/heads/other-checkout".into(),
        checkout_oid: "c".repeat(40),
        config: String::new(),
        config_before_push: String::new(),
        local_refs: BTreeMap::new(),
        extras: Vec::new(),
        tracking_ref: None,
        lfs_config: Vec::new(),
        receive_pack: String::new(),
        selection: None,
        fork_scope: None,
    }
}

#[test]
fn prewrite_snapshot_guard_blocks_changed_selection_and_leaves_ordinary_pushes_unchanged()
-> Result<(), String> {
    let route = PushRoute {
        branch: "topic".into(),
        commit: "a".repeat(40),
        remote: "publish-host".into(),
        destination: "refs/heads/release".into(),
        urls: vec!["https://publish.invalid/team/repo".into()],
        lease: Some("b".repeat(40)),
    };
    let original = git::repository_tools::guard::Snapshot::recording();
    for (selected, change) in [(true, 0), (true, 1), (true, 2), (false, 1)] {
        let mut plan = plan(&route);
        let expected_args = push_args(&plan)?;
        if selected {
            plan.retain_selection(PreparedPush {
                snapshot: original.clone(),
                route: route.clone(),
            })?;
        }
        let mut current = original.clone();
        match change {
            1 => current.worktrees.push_str("new linked worktree"),
            2 => current.status.push_str("tracked working changes"),
            _ => {}
        }
        let mut checks = 0;
        let mut endpoints = 0;
        let result = check_selected_snapshot(&plan, |captured| {
            checks += 1;
            assert_eq!(captured, &original); // retained input, never recaptured
            if *captured == current {
                Ok(())
            } else {
                Err("selected Snapshot changed".into())
            }
        })
        .and_then(|()| {
            // Inert dispatch callback after the same production guard. No
            // repository read, command constructor or endpoint is executed.
            endpoints += 1;
            push_args(&plan)
        });
        assert_eq!(checks, usize::from(selected));
        if selected && change != 0 {
            assert_eq!(result, Err("selected Snapshot changed".into()));
            assert_eq!(endpoints, 0);
        } else {
            assert_eq!(result?, expected_args);
            assert_eq!(endpoints, 1);
        }
    }
    Ok(())
}

#[test]
fn changed_endpoint_oid_destination_or_lease_cannot_enter_the_selected_checklist()
-> Result<(), String> {
    let route = PushRoute {
        branch: "topic".into(),
        commit: "a".repeat(40),
        remote: "publish-host".into(),
        destination: "refs/heads/release".into(),
        urls: vec!["https://publish.invalid/team/repo".into()],
        lease: Some("b".repeat(40)),
    };
    let selected = PreparedPush {
        snapshot: git::repository_tools::guard::Snapshot::recording(),
        route: route.clone(),
    };
    let mut original = plan(&route);
    original.retain_selection(selected.clone())?;
    assert_eq!(original.selection().map(|s| &s.route), Some(&route));
    assert_eq!(original.again(), route.operations());
    assert!(push_args(&original)?.contains(&format!(
        "--force-with-lease=refs/heads/release:{}",
        "b".repeat(40)
    )));
    for change in 0..5 {
        let mut changed = plan(&route);
        match change {
            0 => changed.urls = vec!["https://foreign.invalid/repo".into()],
            1 => changed.push_oid = "d".repeat(40),
            2 => changed.destination = "refs/heads/other".into(),
            3 => changed.remote = "pull-host".into(),
            _ => changed.base = Some("e".repeat(40)),
        }
        assert!(changed.retain_selection(selected.clone()).is_err());
        assert!(changed.selection().is_none());
    }
    // A mutable UI plan also cannot substitute another lease after review.
    original.base = Some("f".repeat(40));
    assert!(push_args(&original).is_err());
    Ok(())
}
