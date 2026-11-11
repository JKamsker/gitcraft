use super::*;
use egui_kittest::kittest::Queryable;

#[test]
fn repository_toolbar_keeps_trailing_actions_reachable_at_narrow_scales() {
    for width in [640.0, 800.0, 1400.0] {
        for scale in [1.0, 1.25, 1.5] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/nonexistent/toolbar-layout".into(), &ctx);
            tab.refs.current = Some("feature/a-long-branch-name-with-many-words".into());
            let mut h = egui_kittest::Harness::builder()
                .with_size([width / scale, 500.0 / scale])
                .with_pixels_per_point(scale)
                .build_ui_state(
                    |ui, (tab, fonts, actions): &mut (RepoTab, bool, Vec<Action>)| {
                        if !*fonts {
                            crate::theme::install_fonts(ui.ctx());
                            egui_extras::install_image_loaders(ui.ctx());
                            *fonts = true;
                            return;
                        }
                        egui::Panel::top("toolbar-test").show(ui, |ui| {
                            tab.toolbar(ui, actions);
                        });
                    },
                    (tab, false, Vec::new()),
                );
            h.run();
            let reload = h.get_by_label("Reload").rect();
            assert!(
                h.ctx.content_rect().contains_rect(reload),
                "{width}/{scale}: {reload:?}"
            );
            h.get_by_label("Reload").click();
            h.run();
            assert!(h.state().2.iter().any(|a| matches!(a, Action::Reload)));
        }
    }
}

#[test]
fn area_d_open_in_order_icons_remote_reasons_and_copy_scope() -> Result<(), String> {
    for width in [640.0, 1100.0, 1600.0] {
        for scale in [1.0, 1.25, 1.5] {
            for path in [
                "/nonexistent/local repo",
                "ssh://host/home/user/remote repo",
            ] {
                let ctx = egui::Context::default();
                let tab = RepoTab::new_deferred(path.into(), &ctx);
                let mut h = egui_kittest::Harness::builder()
                    .with_size([width / scale, 760.0 / scale])
                    .with_pixels_per_point(scale)
                    .build_ui_state(
                        |ui, (tab, fonts): &mut (RepoTab, bool)| {
                            if !*fonts {
                                crate::theme::install_fonts(ui.ctx());
                                egui_extras::install_image_loaders(ui.ctx());
                                *fonts = true;
                                return;
                            }
                            let open = ui.button("Open In");
                            egui::Popup::menu(&open)
                                .style(crate::theme::menu_style)
                                .show(|ui| {
                                    crate::menu_item::bounded_menu(ui, "open-in", |ui| {
                                        tab.open_in_items(ui, &mut Vec::new())
                                    });
                                });
                        },
                        (tab, false),
                    );
                h.run();
                h.get_by_label("Open In").click();
                h.run();
                let manager = h.get_by_label(reveal_label()).rect();
                let editor = h.get_by_label("Open in editor").rect();
                let copy = h.get_by_label("Copy path").rect();
                let web = h.get_by_label("Open in browser").rect();
                assert!(
                    manager.bottom() <= editor.top()
                        && editor.bottom() <= copy.top()
                        && copy.bottom() < web.top()
                );
                h.get_by_label("No remote with a web page");
                if path.starts_with("ssh:") {
                    assert!(
                        h.query_all_by_label_contains("programs of this machine")
                            .next()
                            .is_some()
                    );
                } else if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    h.render()?
                        .save(format!("{dir}/open-in-{width:.0}-{scale:.2}.png"))
                        .map_err(|e| e.to_string())?;
                }
                h.get_by_label("Copy path").click();
                h.step();
                let expected = crate::remote::parse(std::path::Path::new(path))
                    .map_or_else(|| path.to_owned(), |remote| remote.path);
                assert!(h.output().platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == &expected)));
                h.run();
            }
        }
    }
    Ok(())
}
