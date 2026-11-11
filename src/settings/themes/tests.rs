use super::*;
use egui_kittest::kittest::Queryable;

fn harness(width: f32, scale: f32) -> egui_kittest::Harness<'static, (Settings, ThemeId, bool)> {
    egui_kittest::Harness::builder()
        .with_size([width / scale, 900.0 / scale])
        .with_pixels_per_point(scale)
        .build_ui_state(
            |ui, (settings, selected, fonts): &mut (Settings, ThemeId, bool)| {
                if !*fonts {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    *fonts = true;
                    super::super::page::show(ui.ctx(), crate::settings::Page::Appearance);
                    return;
                }
                crate::theme::apply(ui.ctx(), selected, &settings.theme_tweaks);
                let mut view = SettingsView {
                    theme: selected.clone(),
                    git_version: "2.45.0",
                    dialog_open: false,
                    update: crate::settings::UpdateCheck::NotYet,
                    check_updates_now: false,
                    previous: None,
                    previous_is_newer: false,
                    pinned: String::new(),
                    go_back: false,
                    unpin: false,
                    request: None,
                };
                settings.page(ui, &mut view);
                if let Some(ProfileRequest::Theme(id)) = view.request {
                    *selected = id;
                }
            },
            (Settings::default(), ThemeId::default(), false),
        )
}

#[test]
fn area_d_followup_expanded_preview_keeps_controls_reachable_at_small_heights() -> Result<(), String>
{
    for (width, height) in [(640.0, 900.0), (800.0, 500.0), (1100.0, 500.0)] {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = harness(width, scale);
            h.set_size(egui::vec2(width / scale, height / scale));
            h.run();
            h.get_by_label("Preview").click();
            h.run();
            h.get_by_label("Show more").scroll_to_me();
            h.run();
            let rect = h.get_by_label("Show more").rect();
            assert!(
                h.ctx.content_rect().contains_rect(rect),
                "expanded preview makes disclosure unreachable at {width}x{height}/{scale}: {rect:?}"
            );
            if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                h.render()?
                    .save(format!(
                        "{dir}/followup-preview-{width:.0}-{height:.0}-{scale:.2}.png"
                    ))
                    .map_err(|e| e.to_string())?;
            }
            h.get_by_label("Show more").click();
            h.run();
            h.get_by_label("Show less");
            let external = theme::find(&ThemeId::new("catppuccin-latte")).ok_or("missing theme")?;
            h.get_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                .scroll_to_me();
            h.run();
            let card = h
                .get_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                .rect();
            assert!(h.ctx.content_rect().contains_rect(card));
            h.get_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                .click();
            h.run();
            assert_eq!(h.state().1, external.id);
        }
    }
    Ok(())
}

#[test]
fn area_d_followup_search_disclosure_honors_explicit_collapse() -> Result<(), String> {
    let mut h = harness(1100.0, 1.0);
    h.get_by_role(egui::accesskit::Role::TextInput).focus();
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("Catppuccin");
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Catppuccin Latte");
    // Exercise the original Show more -> Show less reproduction as well as a
    // correctly auto-expanded search, whose disclosure already reads Show less.
    if h.query_by_label("Show more").is_some() {
        h.get_by_label("Show more").click();
        h.run();
    }
    h.get_by_label("Show less").click();
    h.run();
    assert!(
        h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Catppuccin Latte")
            .next()
            .is_none(),
        "explicit collapse was overridden by automatic search reveal"
    );
    h.get_by_label("Show more");
    if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        h.render()?
            .save(format!("{dir}/followup-search-collapsed.png"))
            .map_err(|e| e.to_string())?;
    }
    h.get_by_label("Show more").click();
    h.run();
    h.get_by_label("Show less");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Catppuccin Latte");
    assert_eq!(h.state().1, ThemeId::default());
    Ok(())
}

#[test]
fn area_d_appearance_matrix_discloses_filters_and_preserves_active_theme() -> Result<(), String> {
    for width in [640.0, 1100.0, 1600.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = harness(width, scale);
            h.run();
            let builtin_names: Vec<_> = theme::themes()
                .iter()
                .filter(|t| t.group == Group::Gitcraft)
                .map(|t| t.name.clone())
                .collect();
            assert_eq!(builtin_names.len(), 5);
            for name in &builtin_names {
                h.get_by_role_and_label(egui::accesskit::Role::Button, name);
            }
            assert!(h.query_all_by_label("Catppuccin Latte").next().is_none());
            if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                h.render()?
                    .save(format!("{dir}/appearance-{width:.0}-{scale:.2}.png"))
                    .map_err(|e| e.to_string())?;
            }
            h.get_by_label("Show more").scroll_to_me();
            h.run();
            h.get_by_label("Show more").focus();
            h.key_press(egui::Key::Enter);
            h.run();
            h.get_by_label("Show less");
            let external =
                theme::find(&ThemeId::new("catppuccin-latte")).ok_or("missing fixture theme")?;
            h.get_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                .scroll_to_me();
            h.run();
            h.get_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                .click();
            h.run();
            assert_eq!(h.state().1, external.id);
            h.get_by_label("Show less").scroll_to_me();
            h.run();
            h.get_by_label("Show less").click();
            h.run();
            h.get_by_label_contains("Active theme: Catppuccin Latte");
            assert_eq!(h.state().1, external.id);
            h.get_by_label("Show more").click();
            h.run();
            h.get_by_label("Dark").scroll_to_me();
            h.run();
            h.get_by_label("Dark").click();
            h.run();
            assert!(
                h.query_all_by_role_and_label(egui::accesskit::Role::Button, &external.name)
                    .next()
                    .is_none()
            );
            h.get_by_label("Light").click();
            h.run();
            h.get_by_role_and_label(egui::accesskit::Role::Button, &external.name);
            assert_eq!(h.state().1, external.id);
            h.get_by_label("All").click();
            h.run();
            // A custom-painted theme card is keyboard focusable and activates with Enter.
            let dark = theme::themes()
                .iter()
                .find(|t| t.group == Group::Gitcraft && t.dark())
                .map(|t| t.name.clone())
                .ok_or("missing dark theme")?;
            h.get_by_role_and_label(egui::accesskit::Role::Button, &dark)
                .focus();
            h.key_press(egui::Key::Enter);
            h.run();
            assert_eq!(h.state().1.label(), dark);
        }
    }
    Ok(())
}
