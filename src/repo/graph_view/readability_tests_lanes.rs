#[test]
fn lane_names_distinguish_refs_tags_detached_and_reused_columns() {
    let (ctx, mut tab) = fixture(8);
    tab.commits[3] = Arc::new(git::Commit {
        hash: "unrelated".into(),
        parents: vec!["other".into()],
        refs: vec!["origin/topic".into()],
        ..Default::default()
    });
    tab.commits[4] = Arc::new(git::Commit {
        hash: "other".into(),
        parents: vec!["tail".into()],
        ..Default::default()
    });
    tab.commits[5] = Arc::new(git::Commit {
        hash: "tail".into(),
        ..Default::default()
    });
    tab.commits[6] = Arc::new(git::Commit {
        hash: "tagged".into(),
        refs: vec!["tag: v1".into()],
        ..Default::default()
    });
    tab.commits[7] = Arc::new(git::Commit {
        hash: "detached".into(),
        ..Default::default()
    });
    tab.refs.remotes = vec![("origin".into(), vec![])];
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    ctx.run_ui(Default::default(), |ui| {
        let named = tab.readable_viewport(ui, 1..3, 24);
        assert_eq!(named.labels.len(), 1);
        assert_eq!(named.labels[&1], "main");
        // A visible real pill suppresses the faint duplicate, even if it is a tag.
        assert!(tab.readable_viewport(ui, 3..5, 24).labels.is_empty());
        let tip_offscreen = tab.readable_viewport(ui, 4..6, 24);
        assert_eq!(tip_offscreen.labels[&4], "origin/topic");
        let tag_visible = tab.readable_viewport(ui, 6..7, 24);
        assert!(tag_visible.labels.is_empty());
        let detached = tab.readable_viewport(ui, 7..8, 24);
        assert_eq!(detached.labels[&7], "detached");
    })
    .drop_without_applying_deltas();
}

#[test]
fn auto_sized_pills_keep_double_click_and_branch_context_hitboxes() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["feature".to_owned()];
    tab.rebuild_graph_labels();
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.05);
    let point = painted_text_center(&shapes, "feature");
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 0.1);
    let mut actions = Vec::new();
    for (n, pressed) in [true, false, true, false].into_iter().enumerate() {
        let (_, acts) = frame(
            &ctx,
            &mut tab,
            size,
            vec![Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            0.2 + n as f64 * 0.03,
        );
        actions.extend(acts);
    }
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Checkout(name) if name == "feature"))
    );
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    frame(&ctx, &mut tab, size, events, 0.6);
    let hash = &tab.commits[0].hash;
    assert_eq!(
        ctx.data(|d| d.get_temp::<Option<String>>(super::menu::pill_menu_id(hash))),
        Some(Some("feature".to_owned()))
    );
    assert!(egui::Popup::is_any_open(&ctx));
}

#[test]
fn ghost_rows_do_not_offer_an_unresolvable_message_popup() {
    let (ctx, mut tab) = fixture(10);
    let created = git::Commit {
        hash: "\0ghost-test".into(),
        summary: "Preview only".into(),
        parents: vec![tab.commits[0].hash.clone()],
        ..Default::default()
    };
    tab.ghost.show_for_test(
        crate::repo::ghost::Outcome {
            created: vec![created],
            copies: 1,
            ..Default::default()
        },
        &tab.commits,
    );
    let size = vec2(900.0, 220.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.0);
    let point = egui::pos2(
        header_x(&shapes, "DESCRIPTION") + 35.0,
        summary_y(&shapes, "Preview only"),
    );
    for time in [0.1, 1.0, 2.0] {
        let (shapes, _) = frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], time);
        assert!(
            !texts(&shapes)
                .iter()
                .any(|t| t.galley.text().contains("Loading full commit message"))
        );
    }
    assert!(
        tab.graph_messages
            .message(&tab.path, "\0ghost-test")
            .is_none()
    );
}

#[test]
fn cached_badges_refresh_when_refs_change_even_without_changing_the_graph() {
    let (ctx, mut tab) = fixture(1);
    let commit = git::Commit {
        refs: vec!["origin/topic".into()],
        ..Default::default()
    };
    tab.refs.remotes = vec![("origin".into(), vec![])];
    tab.rebuild_graph_labels();
    let first = tab.graph_badges(&commit);
    assert_eq!(first[0].label, "topic");
    assert!(!first[0].local);
    tab.refs.remotes.clear();
    tab.refs.local = vec![git::Branch {
        name: "origin/topic".into(),
        is_head: true,
        target: String::new(),
        upstream: String::new(),
        track: String::new(),
    }];
    tab.refs.current = Some("origin/topic".into());
    tab.rebuild_graph_labels();
    let refreshed = tab.graph_badges(&commit);
    assert_eq!(refreshed[0].label, "origin/topic");
    assert!(refreshed[0].local && refreshed[0].head && refreshed[0].remotes.is_empty());
    frame(&ctx, &mut tab, vec2(900.0, 220.0), vec![], 0.0);
}

#[test]
fn hidden_remote_names_are_not_reintroduced_by_faint_lane_labels() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["refs/remotes/origin/hidden".into()];
    tab.visibility.toggle_remote_graph("origin");
    tab.rebuild_graph_labels();
    ctx.run_ui(Default::default(), |ui| {
        let viewport = tab.readable_viewport(ui, 1..3, 24);
        assert!(
            viewport
                .labels
                .values()
                .all(|name| !name.contains("hidden"))
        );
    })
    .drop_without_applying_deltas();
}
