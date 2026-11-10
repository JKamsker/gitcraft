// Delivery. Included to preserve libtest shard names.
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

#[test]
fn german_watcher_transitions_and_live_status_preserve_protocol_and_delivery_ownership()
-> Result<(), String> {
    let _network = crate::network::test_pause::set(None);
    let before = snapshot('a', false, Checks::Passing);
    let mut after = snapshot('b', true, Checks::Failing);
    after.advanced = Some(1234);
    crate::i18n::set_language("en-US");
    let english_keys: Vec<_> = transitions(&before, &after)
        .into_iter()
        .map(|n| n.key)
        .collect();
    let ctx = egui::Context::default();
    let path = PathBuf::from("/recorded/private repo/$(literal)");
    let mut watcher = Watcher::default();
    let tick = |watcher: &mut Watcher| {
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
        )
    };
    tick(&mut watcher);
    assert_eq!(watcher.status, "Watching 1 repository");
    let owner = watcher
        .deliveries
        .get(&path)
        .ok_or("missing delivery owner")?
        .0
        .clone();
    crate::i18n::set_language("de");
    tick(&mut watcher);
    assert_eq!(watcher.status, "1 Repository wird überwacht");
    assert!(
        !owner.is_cancelled(),
        "language does not retire profile/path delivery ownership"
    );
    let notices = transitions(&before, &after);
    assert_eq!(
        notices.iter().map(|n| n.key.clone()).collect::<Vec<_>>(),
        english_keys
    );
    assert_eq!(notices[0].text, "main ist um 1.234 Commits vorgerückt");
    assert_eq!(notices[1].text, "Review angefragt: #1 A change");
    assert_eq!(notices[2].text, "Prüfungen fehlgeschlagen: #1 A change");
    assert_eq!(
        notices[3].text,
        "Prüfungen des Standard-Branches fehlgeschlagen"
    );
    assert_eq!(
        crate::network::Pause::Metered.label(),
        "Getaktete Verbindung"
    );
    let mut probe = Recording {
        calls: Vec::new(),
        enabled: true,
        before: "a".repeat(40),
        after: "b".repeat(40),
        missing_head: false,
    };
    observe(&mut probe, Some(&before))?;
    assert_eq!(probe.calls[0], ["fetch", "--all", "--prune", "--quiet"]);
    assert!(
        probe
            .calls
            .iter()
            .any(|args| args == &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
    );
    watcher.queue_notification(&path, &notices[1].text);
    for job in std::mem::take(&mut watcher.queued_notifications) {
        job();
    }
    assert_eq!(
        *watcher
            .recorded_notifications
            .lock()
            .map_err(|e| e.to_string())?,
        vec!["$(literal): Review angefragt: #1 A change"]
    );
    Ok(())
}

#[test]
fn remote_mutation_bucket_does_not_merge_tray_subscriptions() {
    let a = PathBuf::from("ssh://build/main");
    let b = PathBuf::from("ssh://build/linked");
    let alias = PathBuf::from("ssh://alias/other");
    let selected = paths(&[a.clone(), b.clone()], std::slice::from_ref(&alias));
    assert_eq!(selected, vec![a.clone(), b.clone(), alias.clone()]);
    let mut model = Model::default();
    model.configure(0, connections::current(), selected);
    assert_eq!(model.subscriptions.len(), 3);
    for path in [a, b, alias] {
        assert!(model.subscriptions.contains_key(&path));
        assert_eq!(crate::repo::mutation_key(&path), path);
    }
}
