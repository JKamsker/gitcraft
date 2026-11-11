use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn german_actual_row_activates_with_opaque_title_number_and_identity() {
    crate::i18n::set_language("de");
    let item = Item {
        repo: None,
        forge: Forge::GitLab,
        pr: InboxPr {
            number: 1234,
            title: "Fix 日本語".into(),
            url: "https://literal-host/a/b/-/merge_requests/1234".into(),
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
                &[Reason::ReviewRequested, Reason::NewComments(1234)],
                &t,
            )
            .clicked();
            crate::a11y::end_frame(ui.ctx());
            assert_eq!(item.forge.noun(), "merge request");
            assert_eq!(
                item.pr.url,
                "https://literal-host/a/b/-/merge_requests/1234"
            );
        },
        false,
    );
    h.run();
    let label = "GitLab Merge Request 1234: Fix 日本語, von Jonas, a/b. Review angefordert, 1.234 neue Kommentare";
    h.get_by_role_and_label(egui::accesskit::Role::Button, label);
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_label(label).is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(*h.state());
}

#[test]
fn protocol_errors_are_grouped_raw_and_only_display_is_translated() {
    crate::i18n::set_language("de");
    let raw = "no Forgejo login known for codeberg.org: add one with `tea login add`";
    let problem = Problem {
        forge: Forge::Forgejo,
        host: "codeberg.org".into(),
        at: "Forgejo · codeberg.org".into(),
        why: raw.into(),
    };
    assert!(forge_inbox::is_no_login(&problem.why));
    assert!(forge_inbox::gives_up(&problem.why));
    let shown = grouped(&[&problem]);
    assert!(shown[0].contains("Für codeberg.org ist keine Forgejo-Anmeldung bekannt"));
    assert_eq!(problem.why, raw);
    assert!(forge_inbox::is_no_login(&problem.why));
    assert!(forge_inbox::gives_up(&problem.why));
    assert_eq!(
        forge_inbox::display_error("HTTP 502 literal"),
        "HTTP 502 literal"
    );
}
