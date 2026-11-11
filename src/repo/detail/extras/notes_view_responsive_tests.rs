use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn responsive_notes_keeps_namespace_and_draft_editing_out_of_the_form() {
    for width in [240.0, 420.0, 800.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut styled = false;
            let notes = Notes {
                namespace: Some("commits".into()),
                namespaces: vec!["commits".into(), "review".into()],
                note: Some(Ok(Some("Review findings\nMore detail".into()))),
                remote: Some("fork".into()),
                existing: vec!["commits".into()],
                ..Default::default()
            };
            let mut h = Harness::builder()
                .with_size([width / scale, 550.0])
                .with_pixels_per_point(scale)
                .build_ui_state(
                    move |ui, (notes, cmds): &mut (Notes, Vec<ExtrasCmd>)| {
                        if !styled {
                            crate::theme::install_fonts(ui.ctx());
                            crate::theme::apply(
                                ui.ctx(),
                                &crate::theme::ThemeId::default(),
                                &crate::theme::Tweaks::default(),
                            );
                            egui_extras::install_image_loaders(ui.ctx());
                            styled = true;
                            return;
                        }
                        egui::CentralPanel::default().show(ui, |ui| {
                            section(ui, notes, &["origin".into(), "fork".into()], true, cmds);
                        });
                    },
                    (notes, Vec::new()),
                );
            for _ in 0..4 {
                h.step();
            }
            assert!(h.query_by_label("Namespace").is_none());
            assert!(h.query_by_label("Use").is_none());
            h.get_by_label("Notes namespace: commits").click();
            h.step();
            assert!(h.query_by_label("review").is_some());
            h.key_press(egui::Key::Escape);
            h.step();
            h.get_by_label("Edit").click();
            h.step();
            assert_eq!(
                h.state().0.draft.as_deref(),
                Some("Review findings\nMore detail")
            );
            h.get_by_label("Notes namespace: commits").click();
            h.step();
            assert!(h.state().1.is_empty(), "writing changed namespace");
            crate::repo::detail::responsive_tests::evidence(
                &mut h,
                &format!("notes-{width}-{scale}"),
            );
            h.get_by_label("Save note").click();
            h.step();
            assert!(h.state().1.iter().any(
                |c| matches!(c, ExtrasCmd::SaveNote(t) if t == "Review findings\nMore detail")
            ));
        }
    }
}
