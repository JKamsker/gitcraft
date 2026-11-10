use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(
    width: f32,
    height: f32,
    scale: f32,
    many: bool,
) -> Harness<'static, (Notes, Vec<ExtrasCmd>)> {
    let notes = Notes {
        namespace: Some("commits".into()),
        namespaces: if many {
            (0..20)
                .map(|i| format!("review/team-platform/preproduction/release-{i:02}"))
                .collect()
        } else {
            vec!["commits".into(), "review".into()]
        },
        note: Some(Ok(Some("Review findings".into()))),
        ..Default::default()
    };
    let mut styled = false;
    Harness::builder()
        .with_size([width / scale, height / scale])
        .with_pixels_per_point(scale)
        .build_ui_state(
            move |ui, (notes, cmds): &mut (Notes, Vec<ExtrasCmd>)| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(
                        ui.ctx(),
                        &crate::theme::ThemeId::default(),
                        &Default::default(),
                    );
                    egui_extras::install_image_loaders(ui.ctx());
                    styled = true;
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| section(ui, notes, &[], true, cmds));
            },
            (notes, Vec::new()),
        )
}

#[test]
fn responsive_followup_namespace_field_keeps_popup_open_for_typing_and_submission() {
    let mut h = harness(420.0, 400.0, 1.0, false);
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("Notes namespace: commits").click();
    for _ in 0..4 {
        h.step();
    }
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    for _ in 0..4 {
        h.step();
    }
    assert!(
        h.query_by_label("Use namespace").is_some(),
        "clicking the namespace field dismissed the popup"
    );
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("release-review");
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().0.new_namespace, "release-review");
    crate::repo::detail::responsive_tests::evidence(&mut h, "followup-namespace-typing");
    h.key_press(egui::Key::Enter);
    for _ in 0..3 {
        h.step();
    }
    assert!(
        h.state()
            .1
            .iter()
            .any(|cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == "release-review"))
    );
    assert!(h.query_by_label("Use namespace").is_none());
    h.state_mut().1.clear();
    h.get_by_label("Notes namespace: commits").click();
    for _ in 0..4 {
        h.step();
    }
    h.get_by_label("review").click();
    for _ in 0..3 {
        h.step();
    }
    assert!(
        h.state()
            .1
            .iter()
            .any(|cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == "review"))
    );
    assert!(
        h.query_by_label("Use namespace").is_none(),
        "selection must dismiss the picker"
    );
}

#[test]
fn responsive_followup_many_namespaces_scroll_and_keep_creation_reachable() {
    for width in [360.0, 420.0, 800.0] {
        for scale in [1.0, 1.25, 1.5] {
            let height = 400.0;
            let mut h = harness(width, height, scale, true);
            for _ in 0..4 {
                h.step();
            }
            h.get_by_label("Notes namespace: commits").click();
            for _ in 0..4 {
                h.step();
            }
            crate::repo::detail::responsive_tests::evidence(
                &mut h,
                &format!("followup-namespaces-{width}-{scale}"),
            );
            let use_button = h.get_by_label("Use namespace").rect();
            assert!(
                use_button.bottom() <= height / scale && use_button.top() >= 0.0,
                "creation controls offscreen: {use_button:?}"
            );
            let last = "review/team-platform/preproduction/release-19";
            h.get_by_label(last).scroll_to_me();
            for _ in 0..5 {
                h.step();
            }
            let r = h.get_by_label(last).rect();
            assert!(
                r.top() >= 0.0 && r.bottom() < use_button.top(),
                "last namespace unreachable: {r:?}"
            );
            h.get_by_label(last).click();
            for _ in 0..3 {
                h.step();
            }
            assert!(
                h.state()
                    .1
                    .iter()
                    .any(|cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == last))
            );
            assert!(h.query_by_label("Use namespace").is_none());
        }
    }
}
