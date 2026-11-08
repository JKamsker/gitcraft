//! The actual remotes sidebar and its reachable menus at all requested widths/scales.
use super::*;
use crate::forge::{Forge, Hosting};
use egui_kittest::{Harness, kittest::Queryable};

fn fixture() -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/remote-ui".into(), &egui::Context::default());
    for (name, owner) in [("origin", "Vancete"), ("fork", "JKamsker")] {
        tab.remotes.push(Remote {
            name: name.into(),
            url: format!("https://github.com/{owner}/gitcraft"),
        });
        tab.forge_remotes.push((
            name.into(),
            Hosting {
                forge: Forge::GitHub,
                host: "github.com".into(),
                path: format!("{owner}/gitcraft"),
                web: "https://github.com".into(),
            },
        ));
        tab.refs.remotes.push((
            name.into(),
            vec![Branch {
                name: format!("{name}/a-very-long-remote-branch-name"),
                target: "a".repeat(40),
                is_head: false,
                upstream: String::new(),
                track: String::new(),
            }],
        ));
    }
    tab
}

fn save(harness: &mut Harness<'_, RepoTab>, name: &str) -> Result<(), String> {
    if let Ok(dir) = std::env::var("GITCRAFT_REMOTE_SHOTS") {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        harness
            .render()?
            .save(std::path::Path::new(&dir).join(format!("{name}.png")))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn remote_menus_and_hidden_groups_are_reachable_at_every_width_and_scale() -> Result<(), String> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for width in [360.0, 720.0, 1200.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut harness = Harness::builder()
                .with_size([width / scale, 700.0 / scale])
                .with_pixels_per_point(scale)
                .build_ui_state(
                    |ui, tab: &mut RepoTab| {
                        if ui.ctx().cumulative_pass_nr() == 0 {
                            crate::theme::install_fonts(ui.ctx());
                            egui_extras::install_image_loaders(ui.ctx());
                            return;
                        }
                        if tab.hosting.is_none() {
                            tab.take_forge_selection(false, ui.ctx());
                        }
                        let mut acts = Vec::new();
                        let mut sections = Sections::new(egui::Id::new("remote-test"), None);
                        // Keep a realistic sidebar width in the normal/wide window.
                        ui.set_max_width(ui.available_width().min(320.0));
                        ui.spacing_mut().item_spacing.y = 0.0;
                        tab.remotes_section(ui, &mut acts, true, "", &mut sections);
                        sections.show(ui, ui.clip_rect(), 0.0, &mut acts);
                        for act in acts {
                            match act {
                                Action::PrimaryForge(name) => tab.set_primary_forge(name, ui.ctx()),
                                Action::BranchVisibility(Change::RemoteSidebar(name)) => {
                                    if !tab.visibility.sidebar_remotes.remove(&name) {
                                        tab.visibility.sidebar_remotes.insert(name);
                                    }
                                    tab.visibility.store(ui.ctx(), tab.repo_key());
                                }
                                Action::BranchVisibility(Change::RemoteGraph(name)) => {
                                    tab.visibility.toggle_remote_graph(&name);
                                    tab.visibility.store(ui.ctx(), tab.repo_key());
                                }
                                _ => {}
                            }
                        }
                    },
                    fixture(),
                );
            harness.run();
            let suffix = format!("{}-{}", width as u32, (scale * 100.0) as u32);
            harness.get_by_label("Remote actions for fork").focus();
            harness.run();
            harness.key_press(egui::Key::Enter);
            harness.run();
            for label in [
                "Set as primary forge",
                "Hide remote branches in sidebar",
                "Hide remote branches in graph",
                "Remove…",
            ] {
                let rect = harness.get_by_label(label).rect();
                assert!(
                    harness.ctx.content_rect().contains_rect(rect),
                    "{label} unreachable at {suffix}: {rect:?}"
                );
            }
            save(&mut harness, &format!("remote-menu-{suffix}"))?;
            harness.get_by_label("Set as primary forge").focus();
            harness.run();
            harness.key_press(egui::Key::Enter);
            harness.run();
            assert_eq!(harness.state().primary_forge_remote(), Some("fork"));
            assert!(
                harness
                    .query_all_by_label_contains("remote fork, primary forge")
                    .next()
                    .is_some()
            );
            harness.get_by_label("Remote actions for fork").click();
            harness.run();
            harness
                .get_by_label("Hide remote branches in sidebar")
                .click();
            harness.run();
            assert!(
                harness
                    .query_all_by_label_contains("remote branch fork/")
                    .next()
                    .is_none()
            );
            assert!(
                harness
                    .query_all_by_label_contains("remote branch origin/")
                    .next()
                    .is_some()
            );
            save(&mut harness, &format!("remote-hidden-{suffix}"))?;
            harness.get_by_label("Remote actions for fork").click();
            harness.run();
            assert!(
                harness
                    .get_by_label("Show remote branches in sidebar")
                    .rect()
                    .is_positive()
            );
            harness
                .get_by_label("Hide remote branches in graph")
                .click();
            harness.run();
            assert!(harness.state().visibility.graph_remotes.contains("fork"));
            harness.get_by_label("Remote actions for fork").click();
            harness.run();
            save(&mut harness, &format!("remote-restore-menu-{suffix}"))?;
            harness
                .get_by_label("Show remote branches in sidebar")
                .click();
            harness.run();
            assert!(
                harness
                    .query_all_by_label_contains("remote branch fork/")
                    .next()
                    .is_some()
            );
            harness.get_by_label("Remote actions for fork").click();
            harness.run();
            harness
                .get_by_label("Show remote branches in graph")
                .click();
            harness.run();
            assert!(!harness.state().visibility.graph_remotes.contains("fork"));
        }
    }
    Ok(())
}
