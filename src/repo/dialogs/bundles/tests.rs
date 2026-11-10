#[test]
fn incoming_dialog_is_read_only_and_escape_cancels_without_ref_changes() -> Result<(), String> {
    use crate::git;
    use crate::repo::{Dialog, RepoTab};
    let root = std::env::temp_dir().join(format!("gitcraft-bundle-ui-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
        vec![
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "one",
        ],
    ] {
        git::run(Some(&root), &args)?;
    }
    let before = git::run(
        Some(&root),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )?;
    let refs = git::bundle_refs(&root)?;
    let head = refs.iter().find(|r| r.name == "HEAD").ok_or("no HEAD")?;
    let preview = git::inspect_bundle(
        &root,
        git::export_bundle(&root, std::slice::from_ref(head), None)?,
    )?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(root.clone(), &ctx);
    tab.dialog = Some(Dialog::bundle_incoming(
        root.clone(),
        tab._jobs.0.child(),
        "selected.bundle".into(),
        preview,
    ));
    let mut actions = Vec::new();
    let mut text = String::new();
    for frame in 0..3 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 900.0),
            )),
            ..Default::default()
        };
        if frame == 2 {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let mut output = ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut actions));
        text.push_str(
            &output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        output.textures_delta.clear();
    }
    assert!(text.contains("Inspection checks the header"), "{text}");
    assert!(text.contains("SHA-256"), "{text}");
    assert!(tab.dialog.is_none());
    assert!(actions.is_empty());
    assert_eq!(
        git::run(
            Some(&root),
            &["for-each-ref", "--format=%(refname) %(objectname)"]
        )?,
        before
    );
    tab.bundle_done(
        "Import bundle refs".into(),
        Err("fatal: Authentication failed for untrusted prerequisite text".into()),
        crate::repo::Refresh::Status,
        &ctx,
    );
    assert!(tab.dialog.is_none());
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(text, error)| *error && text.contains("Authentication failed"))
    );
    Ok(())
}
