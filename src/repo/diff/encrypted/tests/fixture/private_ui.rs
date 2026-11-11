#[test]
fn decrypted_ui_is_read_only_and_mode_close_reload_or_park_disposes_it() -> Result<(), String> {
    let repo = Repo::new()?;
    let path = "secret.json";
    repo.write(path, &json("age1fixture", "cipher1"))?;
    repo.commit()?;
    repo.write(path, &json("age1fixture", "cipher2"))?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = crate::repo::RepoTab::new_deferred(repo.0.clone(), &ctx);
    let make_view = || -> Result<crate::repo::DiffView, String> {
        let mut view = crate::repo::DiffView::new(work(path, Area::Unstaged, 'M'));
        view.next_request();
        view.data = Some(crate::repo::diff::load(
            &repo.0,
            &view.key,
            git::DiffOptions::default(),
            true,
        ));
        let (send, _) = view.encrypted.begin();
        send.send(plaintext_compare(
            &json("a", "x"),
            &json("b", "y"),
            "fixture-private-old",
            "fixture-private-new",
        ))
        .map_err(|_| "No private receiver")?;
        view.encrypted.poll();
        Ok(view)
    };
    tab.diff = Some(make_view()?);
    let mut actions = Vec::new();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut actions));
    });
    let text = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.shapes.clear();
    out.textures_delta.clear();
    assert!(text.contains("fixture-private-new"));
    assert_eq!(text.lines().filter(|s| *s == "Wrap").count(), 1);
    assert!(!text.contains("Algorithm"));
    assert!(
        !text.contains("Stage file")
            && !text.contains("Stage hunk")
            && !text.contains("Stage 1 line")
    );
    assert!(actions.is_empty());
    out.shapes.clear();
    out.textures_delta.clear();
    for action in [
        crate::repo::Action::DiffAsFile(false),
        crate::repo::Action::DiffRendered(false),
        crate::repo::Action::DiffStructural(false),
        crate::repo::Action::DiffDecryptedEncoding(JsonEncoding::Binary),
        crate::repo::Action::OpenFileComparison(crate::repo::file_comparison::Page::Migration),
        crate::repo::Action::OpenFileComparison(crate::repo::file_comparison::Page::Lens),
        crate::repo::Action::OpenFileComparison(crate::repo::file_comparison::Page::Matrix),
        crate::repo::Action::CloseDiff,
    ] {
        tab.diff = Some(make_view()?);
        let active = tab
            .diff
            .as_ref()
            .and_then(|v| v.encrypted.active.clone())
            .ok_or("No active token")?;
        tab.apply(action, &ctx);
        assert!(!active.load(Ordering::Acquire));
    }
    tab.diff = Some(make_view()?);
    tab.poll_parked_view(&ctx);
    assert!(tab.diff.as_ref().is_some_and(|v| !v.encrypted.enabled));
    Ok(())
}
