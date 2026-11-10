use super::*;
use egui_kittest::{Harness, kittest::Queryable};
#[test]
fn inbox_exposes_forge_repository_author_and_reasons_and_activates() {
    let item = Item {
        repo: None,
        forge: Forge::GitHub,
        pr: InboxPr {
            number: 42,
            title: "Fix 日本語".into(),
            url: "https://github.com/a/b/pull/42".into(),
            author: "Jonas".into(),
            project: "a/b".into(),
            ..Default::default()
        },
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, opened: &mut bool| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            *opened |= inbox_row(
                ui,
                &item,
                &[Reason::ReviewRequested, Reason::ChecksFailing],
                &t,
            )
            .clicked();
            crate::a11y::end_frame(ui.ctx());
        },
        false,
    );
    h.run();
    let label =
        "GitHub pull request 42: Fix 日本語, by Jonas, a/b. review requested, checks failing";
    h.get_by_role_and_label(egui::accesskit::Role::Button, label);
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_label(label).is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(*h.state());
}
