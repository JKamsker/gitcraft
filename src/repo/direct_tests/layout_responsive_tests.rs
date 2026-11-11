use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn responsive_detail_collapses_and_reopens_without_losing_drafts_or_width()
-> Result<(), Box<dyn std::error::Error>> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/responsive-layout".into(), &ctx);
    tab.commit_summary = "keep this draft".into();
    tab.commit_body = "and its description".into();
    tab.amend = true;
    let hash = "a".repeat(40);
    tab.refs.head = hash.clone();
    tab.detail = Some(crate::git::CommitDetail {
        hash: hash.clone(),
        parents: Vec::new(),
        author: "Tester".into(),
        email: "test@example.invalid".into(),
        author_date: "2026-11-07".into(),
        committer: "Tester".into(),
        commit_date: "2026-11-07".into(),
        message: "Original subject".into(),
        files: Vec::new(),
        in_head: true,
        merges_after: false,
    });
    tab.reword = Some(crate::repo::Reword {
        hash,
        subject: "Editing survives collapse".into(),
        body: "Retain this edit too".into(),
        focus: true,
        fill: false,
    });
    let mut styled = false;
    let mut h = Harness::builder()
        .with_size([1100.0, 800.0])
        .build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(
                        ui.ctx(),
                        &crate::theme::ThemeId::default(),
                        &crate::theme::Tweaks::default(),
                    );
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = Settings::get(ui.ctx());
                    settings.show_branches = false;
                    settings.show_detail = true;
                    settings.set(ui.ctx());
                    styled = true;
                    return;
                }
                tab.side_panels(ui, &mut Vec::new());
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Central content");
                });
            },
            tab,
        );
    for _ in 0..5 {
        h.step();
    }
    assert!(h.query_by_label("Hide the detail panel").is_some());
    assert!(h.get_by_role(egui::accesskit::Role::TextInput).is_focused());
    let original = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("responsive layout fixture lost its panel/editor state")?
        .size()
        .x;
    h.set_size(egui::vec2(360.0, 800.0));
    for _ in 0..6 {
        h.step();
    }
    assert!(h.query_by_label("Show the detail panel").is_some());
    assert!(
        Settings::get(&h.ctx).show_detail,
        "temporary collapse overwrote visibility preference"
    );
    assert_eq!(h.state().commit_summary, "keep this draft");
    assert_eq!(h.state().commit_body, "and its description");
    assert!(h.state().amend);
    crate::repo::detail::responsive_tests::evidence(&mut h, "detail-auto-collapsed");
    h.get_by_label("Show the detail panel").click();
    for _ in 0..6 {
        h.step();
    }
    assert!(
        h.query_by_label("Hide the detail panel").is_some(),
        "explicit reopening at the narrow width failed"
    );
    assert_eq!(
        h.state()
            .reword
            .as_ref()
            .ok_or("responsive layout fixture lost its panel/editor state")?
            .subject,
        "Editing survives collapse"
    );
    assert!(
        h.get_by_role(egui::accesskit::Role::TextInput).is_focused(),
        "reopening lost the editor's keyboard focus"
    );
    h.set_size(egui::vec2(1100.0, 800.0));
    for _ in 0..6 {
        h.step();
    }
    let restored = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("responsive layout fixture lost its panel/editor state")?
        .size()
        .x;
    assert!(
        (original - restored).abs() < 1.0,
        "preferred width changed: {original} → {restored}"
    );
    assert!(h.query_by_label("Hide the detail panel").is_some());
    // A divider dragged below usable size collapses fully and retains a reopening width.
    let mut state = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("responsive layout fixture lost its panel/editor state")?;
    state.outer_rect.set_width(180.0);
    h.ctx
        .data_mut(|d| d.insert_persisted(egui::Id::new("repo_detail"), state));
    for _ in 0..6 {
        h.step();
    }
    assert!(!Settings::get(&h.ctx).show_detail);
    assert!(h.query_by_label("Show the detail panel").is_some());
    assert_eq!(h.state().commit_summary, "keep this draft");
    Ok(())
}
