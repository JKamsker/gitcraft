#[test]
fn clone_form_is_owned_by_its_window_when_a_peer_renders_first() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    let child = ViewportId::from_hash_of("form-owner");
    app.windows
        .parked
        .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    app.in_window(child, |app| {
        app.start_clone();
        app.set_clone_url("https://example.invalid/owner/repo");
    });
    assert!(!app.window_has_dialog());
    let mut saw_peer_form = false;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.clone_dialog(ui.ctx());
        saw_peer_form = app.clone_form.is_some();
        let count = app.tabs.len();
        let blocked = app.window_has_dialog();
        app.window_command(crate::keymap::Command::NewTab, blocked, ui.ctx());
        assert_eq!(app.tabs.len(), count + 1);
    })
    .drop_without_applying_deltas();
    assert!(!saw_peer_form);
    app.in_window(child, |app| {
        assert!(app.window_has_dialog());
        assert_eq!(
            app.clone_url().as_deref(),
            Some("https://example.invalid/owner/repo")
        );
    });
    assert!(!app.window_has_dialog());
    assert!(matches!(app.tabs.last(), Some(Tab::Home)));
    drop(app);
    use egui_kittest::kittest::Queryable as _;
    let owner = h.state().app.clone();
    let mut initialized = false;
    let mut child_ui = egui_kittest::Harness::new_ui_state(
        |ui, owner: &mut Arc<Mutex<App>>| {
            if !std::mem::replace(&mut initialized, true) {
                crate::theme::install_fonts(ui.ctx());
                egui_extras::install_image_loaders(ui.ctx());
                return;
            }
            if let Ok(mut app) = owner.lock() {
                app.in_window(child, |app| app.clone_dialog(ui.ctx()));
            }
        },
        owner.clone(),
    );
    child_ui.run_steps(2);
    child_ui.get_by_label("Clone repository");
    child_ui.get_by_label("Cancel").click();
    child_ui.run();
    let app = owner.lock().map_err(|e| e.to_string())?;
    assert!(app.windows.parked[&child].clone_form.is_none());
    assert!(!app.window_has_dialog());
    Ok(())
}

#[test]
fn recorded_completions_route_to_the_initiator_and_credentials_follow_parked_targets()
-> Result<(), String> {
    #[derive(Clone, Copy)]
    enum Completion {
        Clone,
        Connection,
        Link,
    }
    for source in [Completion::Clone, Completion::Connection, Completion::Link] {
        for poll_from_child in [false, true] {
            let h = recorded_harness();
            let ctx = h.ctx.clone();
            let mut app = h
                .state()
                .as_ref()
                .ok_or("no recorded App")?
                .app
                .lock()
                .map_err(|e| e.to_string())?;
            let child = ViewportId::from_hash_of("completion-owner");
            let root = PathBuf::from("/virtual/completion-target");
            app.tabs = vec![Tab::Home];
            app.windows.parked.insert(
                child,
                Window::new(
                    vec![deferred(root.to_str().ok_or("path")?, &ctx), Tab::Home],
                    ViewportId::ROOT,
                    None,
                ),
            );
            app.windows.parked.get_mut(&child).ok_or("child")?.active = 1;
            match source {
                Completion::Clone => app.recorded_clone_completion(child, root.clone()),
                Completion::Connection => app.recorded_connection_completion(child, root.clone()),
                Completion::Link => app.recorded_launch_completion(child, root.clone()),
            }
            let poller = if poll_from_child {
                child
            } else {
                ViewportId::ROOT
            };
            app.in_window(poller, |app| match source {
                Completion::Clone => app.poll_clone(&ctx),
                Completion::Connection => app.poll_connecting(&ctx),
                Completion::Link => app.poll_launch(&ctx),
            });
            assert_eq!(app.windows.parked[&child].active, 0);
            assert!(
                matches!(&app.windows.parked[&child].tabs[0], Tab::Repo(repo) if repo.has_test_credentials("recorded-user", "recorded-secret") != matches!(source, Completion::Link))
            );
            assert!(matches!(app.tabs.first(), Some(Tab::Home)));
            // A completion may find its target in a different live window.
            app.recorded_connection_completion(ViewportId::ROOT, root.clone());
            app.poll_connecting(&ctx);
            assert!(
                matches!(&app.windows.parked[&child].tabs[0], Tab::Repo(repo) if repo.has_test_credentials("recorded-user", "recorded-secret"))
            );
        }
    }
    Ok(())
}
