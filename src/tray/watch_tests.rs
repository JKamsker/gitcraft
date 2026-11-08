use super::*;
fn snapshot(oid: char, review: bool, checks: Checks) -> Observation {
    Observation {
        default: Some((
            "refs/remotes/origin/main".into(),
            oid.to_string().repeat(40),
        )),
        prs: Some(vec![InboxPr {
            url: "https://forge/o/r/pull/1".into(),
            number: 1,
            title: "A change".into(),
            review_requested: review,
            checks,
            mine: true,
            ..Default::default()
        }]),
        ci_failed: Some(checks == Checks::Failing),
        ..Default::default()
    }
}
#[test]
fn fake_clock_baseline_transitions_backoff_and_missing_provider_results() {
    let now = Instant::now();
    let path = PathBuf::from("/recorded/repo");
    let mut model = Model::default();
    model.configure(1, Default::default(), vec![path.clone()]);
    assert_eq!(model.next(now, &HashSet::new(), true), Some(path.clone()));
    assert!(
        model
            .finish(
                &path,
                Ok(snapshot('a', true, Checks::Failing)),
                now,
                Duration::from_secs(60)
            )
            .is_empty()
    );
    assert!(model.next(now, &HashSet::new(), true).is_none());
    assert!(
        model
            .next(
                now + Duration::from_secs(60),
                &HashSet::from([path.clone()]),
                true
            )
            .is_none()
    );
    assert!(
        model
            .next(now + Duration::from_secs(60), &HashSet::new(), false)
            .is_none()
    );
    model.finish(
        &path,
        Ok(snapshot('a', false, Checks::Passing)),
        now,
        Duration::from_secs(60),
    );
    let mut after = snapshot('b', true, Checks::Failing);
    after.advanced = Some(3);
    let notices = model.finish(&path, Ok(after.clone()), now, Duration::from_secs(60));
    assert_eq!(
        notices.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
        vec![
            "main moved 3 commits",
            "Review requested: #1 A change",
            "Checks failed: #1 A change",
            "Default-branch checks failed"
        ]
    );
    assert!(
        model
            .finish(&path, Ok(after.clone()), now, Duration::from_secs(60))
            .is_empty()
    );
    model.finish(
        &path,
        Ok(Observation::default()),
        now,
        Duration::from_secs(60),
    );
    assert!(
        model
            .finish(&path, Ok(after), now, Duration::from_secs(60))
            .is_empty()
    );
    model.finish(
        &path,
        Err("recorded failure".into()),
        now,
        Duration::from_secs(60),
    );
    assert!(
        model
            .next(now + Duration::from_secs(59), &HashSet::new(), true)
            .is_none()
    );
    assert_eq!(
        model.next(now + Duration::from_secs(60), &HashSet::new(), true),
        Some(path)
    );
}
#[test]
fn paths_are_bounded_and_open_and_recent_share_one_subscription() {
    let open = vec!["/one".into(), "/one".into(), "/two".into()];
    let recent = (0..20)
        .map(|i| format!("/{i}").into())
        .collect::<Vec<PathBuf>>();
    let merged = paths(&open, &recent);
    assert_eq!(merged.len(), MAX_REPOS);
    assert_eq!(
        &merged[..2],
        &[PathBuf::from("/one"), PathBuf::from("/two")]
    );
    let mut model = Model::default();
    model.configure(1, Default::default(), merged.clone());
    assert_eq!(model.subscriptions.len(), MAX_REPOS);
    let different = connections::Context {
        error: Some("different profile configuration".into()),
        ..Default::default()
    };
    assert!(model.configure(1, different, merged));
    assert!(model.subscriptions.values().all(|s| s.baseline.is_none()));
}
struct Recording {
    calls: Vec<Vec<String>>,
    enabled: bool,
    before: String,
    after: String,
    missing_head: bool,
}
impl Probe for Recording {
    fn git(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.push(args.iter().map(|s| (*s).into()).collect());
        match args {
            ["fetch", "--all", "--prune", "--quiet"] => Ok(String::new()),
            ["remote"] => Ok("fork\norigin\n".into()),
            ["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"] => {
                if self.missing_head {
                    Err("no local remote HEAD".into())
                } else {
                    Ok("refs/remotes/origin/main\n".into())
                }
            }
            ["ls-remote", "--symref", "--", "origin", "HEAD"] => {
                Ok("ref: refs/heads/main\tHEAD\n".into())
            }
            [
                "rev-parse",
                "--verify",
                "--end-of-options",
                "refs/remotes/origin/main^{commit}",
            ] => Ok(self.after.clone()),
            ["rev-list", "--count", range, "--"]
                if *range == format!("{}..{}", self.before, self.after) =>
            {
                Ok("7\n".into())
            }
            _ => Err(format!("unexpected recording argv {args:?}")),
        }
    }
    fn hosting(&mut self) -> Option<forge::Hosting> {
        None
    }
    fn inbox(&mut self, _: &forge::Hosting) -> Result<Vec<InboxPr>, String> {
        Err("not called".into())
    }
    fn ci_failed(&mut self, _: &forge::Hosting, _: &str) -> Result<Option<bool>, String> {
        Err("not called".into())
    }
    fn may_run(&self) -> bool {
        self.enabled
    }
}
#[test]
fn actual_observation_orchestration_records_quiet_fetch_validated_revisions_and_exact_count()
-> Result<(), String> {
    let before = snapshot('a', false, Checks::Passing);
    let mut probe = Recording {
        calls: Vec::new(),
        enabled: true,
        before: "a".repeat(40),
        after: "b".repeat(40),
        missing_head: false,
    };
    let observed = observe(&mut probe, Some(&before))?;
    assert_eq!(observed.advanced, Some(7));
    assert_eq!(probe.calls.len(), 5);
    assert_eq!(probe.calls[0], ["fetch", "--all", "--prune", "--quiet"]);
    probe.enabled = false;
    probe.calls.clear();
    assert!(observe(&mut probe, Some(&before)).is_err());
    assert!(probe.calls.is_empty());
    probe.enabled = true;
    probe.missing_head = true;
    assert_eq!(observe(&mut probe, Some(&before))?.advanced, Some(7));
    assert!(
        probe
            .calls
            .iter()
            .any(|args| args == &["ls-remote", "--symref", "--", "origin", "HEAD"])
    );
    probe.calls.clear();
    probe.after = "--bad-object".into();
    assert!(observe(&mut probe, Some(&before))?.default.is_none());
    assert!(
        !probe
            .calls
            .iter()
            .any(|c| c.first().is_some_and(|s| s == "rev-list"))
    );
    Ok(())
}
#[test]
fn profile_switch_cancels_and_discards_an_already_queued_result_before_new_work()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let path = PathBuf::from("/recorded/repo");
    let mut watcher = Watcher::default();
    watcher
        .model
        .configure(1, connections::current(), vec![path.clone()]);
    watcher.model.finish(
        &path,
        Ok(snapshot('a', false, Checks::Passing)),
        Instant::now(),
        Duration::from_secs(60),
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = tasks::Cancellation::default();
    watcher.running = Some(Running {
        path: path.clone(),
        rx,
        cancel: cancel.clone(),
        started: Instant::now(),
        stale: false,
    });
    let mut after = snapshot('b', true, Checks::Failing);
    after.advanced = Some(4);
    tx.send(Ok(after)).map_err(|e| e.to_string())?;
    watcher.tick(
        Inputs {
            enabled: true,
            profile: 2,
            open: std::slice::from_ref(&path),
            busy: &HashSet::new(),
            recent: &[],
            minutes: 10,
            notifications_enabled: true,
        },
        &ctx,
    );
    assert!(cancel.is_cancelled());
    assert!(watcher.running.is_none());
    assert!(
        watcher
            .model
            .subscriptions
            .get(&path)
            .is_some_and(|s| s.baseline.is_none())
    );
    assert!(watcher.notified.is_empty());
    Ok(())
}

#[test]
fn pause_and_deadline_cancel_only_the_owned_job_and_wait_for_acknowledgement() -> Result<(), String>
{
    let _network = crate::network::test_pause::set(None);
    let ctx = egui::Context::default();
    let path = PathBuf::from("/recorded/owned");
    let mut watcher = Watcher::default();
    watcher
        .model
        .configure(1, connections::current(), vec![path.clone()]);
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = tasks::Cancellation::default();
    watcher.running = Some(Running {
        path: path.clone(),
        rx,
        cancel: cancel.clone(),
        started: Instant::now() - JOB_LIMIT - Duration::from_secs(1),
        stale: false,
    });
    watcher.tick(
        Inputs {
            enabled: true,
            profile: 1,
            open: std::slice::from_ref(&path),
            busy: &HashSet::new(),
            recent: &[],
            minutes: 10,
            notifications_enabled: true,
        },
        &ctx,
    );
    assert!(cancel.is_cancelled());
    assert!(watcher.running.as_ref().is_some_and(|job| job.stale));
    assert!(watcher.model.subscriptions.get(&path).is_some_and(|sub| sub.failures == 1 && sub.due.is_some_and(|due| due > Instant::now())));
    tx.send(Ok(snapshot('b', true, Checks::Failing)))
        .map_err(|e| e.to_string())?;
    watcher.tick(
        Inputs {
            enabled: true,
            profile: 1,
            open: std::slice::from_ref(&path),
            busy: &HashSet::new(),
            recent: &[],
            minutes: 10,
            notifications_enabled: true,
        },
        &ctx,
    );
    assert!(watcher.running.is_none());
    assert!(watcher.notified.is_empty());
    let _offline = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    watcher.tick(
        Inputs {
            enabled: true,
            profile: 1,
            open: std::slice::from_ref(&path),
            busy: &HashSet::new(),
            recent: &[],
            minutes: 10,
            notifications_enabled: true,
        },
        &ctx,
    );
    assert!(watcher.status.contains("paused"));
    assert!(watcher.running.is_none());
    Ok(())
}

#[test]
fn linked_worktrees_and_recent_aliases_share_the_recorded_mutation_owner() {
    let paths = paths_by(
        &["/worktree-one".into()],
        &["/worktree-two".into(), "/other".into()],
        |path| {
            if path.to_string_lossy().starts_with("/worktree-") {
                "/common/gitdir".into()
            } else {
                path.into()
            }
        },
    );
    assert_eq!(
        paths,
        vec![PathBuf::from("/worktree-one"), PathBuf::from("/other")]
    );
}

#[test]
fn checked_out_branch_ci_uses_existing_push_destination_routes_and_keeps_transition_status()
-> Result<(), String> {
    let _network = crate::network::test_pause::set(None);
    let remotes = vec![
        git::Remote {
            name: "origin".into(),
            url: "https://github.com/Vancete/gitcraft.git".into(),
        },
        git::Remote {
            name: "fork".into(),
            url: "https://github.com/JKamsker/gitcraft.git".into(),
        },
    ];
    let refs = git::Refs {
        current: Some("dev".into()),
        head: "a".repeat(40),
        push_remotes: std::collections::HashMap::from([("dev".into(), "fork".into())]),
        local: vec![git::Branch {
            name: "dev".into(),
            target: "a".repeat(40),
            is_head: true,
            upstream: "origin/dev".into(),
            track: String::new(),
        }],
        ..Default::default()
    };
    let plan =
        branch_plan(forge::Forge::GitHub, &remotes, &refs).ok_or("missing branch CI plan")?;
    assert_eq!(plan.name, "dev");
    assert_eq!(plan.sha, "a".repeat(40));
    assert_eq!(
        plan.projects
            .iter()
            .map(|p| p.path.as_str())
            .collect::<Vec<_>>(),
        vec!["JKamsker/gitcraft", "Vancete/gitcraft"]
    );
    let ctx = egui::Context::default();
    let path = PathBuf::from("/recorded/repo");
    let mut watcher = Watcher::default();
    watcher
        .model
        .configure(1, connections::current(), vec![path.clone()]);
    let mut before = snapshot('a', false, Checks::Passing);
    before.branch_checks = Some(BranchChecks {
        name: "dev".into(),
        sha: "a".repeat(40),
        failed: false,
    });
    watcher.model.finish(
        &path,
        Ok(before.clone()),
        Instant::now(),
        Duration::from_secs(60),
    );
    let mut after = before;
    after.branch_checks = Some(BranchChecks {
        name: "dev".into(),
        sha: "b".repeat(40),
        failed: true,
    });
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(Ok(after)).map_err(|e| e.to_string())?;
    watcher.running = Some(Running {
        path: path.clone(),
        rx,
        cancel: Default::default(),
        started: Instant::now(),
        stale: false,
    });
    watcher.tick(
        Inputs {
            enabled: true,
            profile: 1,
            open: std::slice::from_ref(&path),
            busy: &HashSet::new(),
            recent: &[],
            minutes: 10,
            notifications_enabled: false,
        },
        &ctx,
    );
    assert_eq!(watcher.status, "repo: Checks failed on dev");
    let rows = crate::tray::menu(std::slice::from_ref(&path), &watcher.status);
    assert_eq!(rows[1].title, "repo: Checks failed on dev");
    assert!(rows[1].action.is_none());
    assert!(watcher.notified.is_empty()); // tray status works with desktop notifications off
    Ok(())
}

#[test]
fn actual_queued_delivery_rechecks_profile_context_path_opt_in_preference_and_drop_owner()
-> Result<(), String> {
    let _network = crate::network::test_pause::set(None);
    let ctx = egui::Context::default();
    let path = PathBuf::from("/recorded/private-repository");
    for invalidation in [
        "unchanged",
        "profile",
        "context",
        "path",
        "opt-out",
        "notifications-off",
        "drop",
    ] {
        let mut watcher = Watcher::default();
        let tick =
            |watcher: &mut Watcher, enabled, profile, open: &[PathBuf], notifications_enabled| {
                watcher.tick(
                    Inputs {
                        enabled,
                        profile,
                        open,
                        busy: &HashSet::new(),
                        recent: &[],
                        minutes: 10,
                        notifications_enabled,
                    },
                    &ctx,
                );
            };
        tick(&mut watcher, true, 1, std::slice::from_ref(&path), true);
        watcher.queue_notification(&path, "Review requested: private title");
        assert_eq!(watcher.queued_notifications.len(), 1);
        let jobs = std::mem::take(&mut watcher.queued_notifications);
        let delivered = std::sync::Arc::clone(&watcher.recorded_notifications);
        match invalidation {
            "profile" => tick(&mut watcher, true, 2, std::slice::from_ref(&path), true),
            "context" => connections::Context {
                error: Some("new immutable configuration".into()),
                ..Default::default()
            }
            .run(|| tick(&mut watcher, true, 1, std::slice::from_ref(&path), true)),
            "path" => tick(&mut watcher, true, 1, &[], true),
            "opt-out" => tick(&mut watcher, false, 1, std::slice::from_ref(&path), true),
            "notifications-off" => tick(&mut watcher, true, 1, std::slice::from_ref(&path), false),
            "unchanged" => tick(&mut watcher, true, 1, std::slice::from_ref(&path), true),
            "drop" => drop(watcher),
            _ => return Err("unknown recording case".into()),
        }
        for job in jobs {
            job();
        }
        let messages = delivered.lock().map_err(|e| e.to_string())?;
        if invalidation == "unchanged" {
            assert_eq!(
                *messages,
                vec!["private-repository: Review requested: private title"],
                "{invalidation}"
            );
        } else {
            assert!(messages.is_empty(), "stale delivery escaped {invalidation}");
        }
    }
    Ok(())
}
