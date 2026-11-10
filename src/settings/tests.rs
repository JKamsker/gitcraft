use super::*;
use eframe::Storage as _;
use std::collections::HashMap;

#[derive(Default)]
struct Memory(HashMap<String, String>);

impl eframe::Storage for Memory {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn settings_of_an_older_version_still_load() {
    let mut storage = Memory::default();
    storage.set_string(
        "s",
        "(ui_scale: 1.25, refs_width: 270.0, show_author: false, show_date: true, \
         show_hash: false, avatars: false, editor: \"subl\")"
            .to_owned(),
    );
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    let back = back.unwrap_or_default();
    assert_eq!(
        (
            back.ui_scale,
            back.refs_width,
            back.show_author,
            back.show_date
        ),
        (1.25, 270.0, false, true)
    );
    assert_eq!(back.editor, "subl");
    assert!(back.branch_names_keep_end);
    let fresh = Settings::default();
    assert_eq!(
        (back.show_when, back.files_as_tree, back.author_width),
        (fresh.show_when, fresh.files_as_tree, fresh.author_width)
    );
}

#[test]
fn resetting_the_columns_keeps_which_show() {
    let mut settings = Settings {
        refs_width: 500.0,
        auto_refs_width: false,
        graph_width: 90.0,
        author_width: 300.0,
        date_width: 60.0,
        last_resized: Some(crate::repo::columns::Bar::Author),
        column_push: 30.0,
        show_hash: true,
        show_author: false,
        compact_graph: true,
        ..Settings::default()
    };
    settings.reset_columns();
    assert_eq!(
        settings,
        Settings {
            show_hash: true,
            show_author: false,
            compact_graph: true,
            ..Settings::default()
        }
    );
}

#[test]
fn a_compact_graph_fits_its_lanes_again() {
    let mut settings = Settings {
        graph_width: 200.0,
        ..Settings::default()
    };
    settings.set_compact_graph(true);
    assert!(settings.compact_graph);
    assert_eq!(settings.graph_width, 0.0);
}

#[test]
fn the_history_map_is_large_until_another_size_is_saved() {
    let mut storage = Memory::default();
    storage.set_string("old", "(ui_scale: 1.0)".to_owned());
    let old: Option<Settings> = eframe::get_value(&storage, "old");
    assert_eq!(old.map(|s| s.history_map), Some(HistoryMap::Large));
    let small = Settings {
        history_map: HistoryMap::Small,
        ..Settings::default()
    };
    eframe::set_value(&mut storage, "small", &small);
    let back: Option<Settings> = eframe::get_value(&storage, "small");
    assert_eq!(back.map(|s| s.history_map), Some(HistoryMap::Small));
    assert_eq!(HistoryMap::Hidden.width(), 0.0);
    assert!(HistoryMap::Small.width() < HistoryMap::Large.width());
}

#[test]
fn settings_survive_a_restart() {
    let settings = Settings {
        last_resized: Some(crate::repo::columns::Bar::Graph),
        column_push: 12.5,
        compact_graph: true,
        branch_names_keep_end: false,
        diff_whole_file: true,
        fold_generated: false,
        fold_patterns: "*.snap, dist/**".to_owned(),
        pull_mode: crate::repo::pull::PullMode::Rebase,
        ..Settings::default()
    };
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, "s", &settings);
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    assert_eq!(back, Some(settings));
}

/// Earlier versions kept "Commit as…" identities under `profiles`; they stay identities, and
/// everything else becomes the default profile's.
#[test]
fn settings_from_before_profiles_get_a_default_profile() {
    let mut storage = Memory::default();
    storage.set_string(
        "s",
        "(editor: \"subl\", profiles: [(label: \"Work\", name: \"Ann\", email: \"ann@corp\")])"
            .to_owned(),
    );
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    let mut back = back.unwrap_or_default();
    back.ensure_profile();
    assert_eq!(
        back.identities,
        [SavedIdentity {
            label: "Work".into(),
            name: "Ann".into(),
            email: "ann@corp".into(),
        }]
    );
    assert_eq!(back.user_profiles, [UserProfile::default()]);
    assert_eq!(back.profile().id, back.active_profile);
    assert!(!back.profile().sets_git());
}

#[test]
fn profiles_are_added_named_and_kept_apart() {
    let mut settings = Settings::default();
    settings.profile_mut().author_email = "me@home".into();
    let work = settings.add_profile(&settings.new_profile_name("Profile"), false);
    let copy = settings.add_profile(&settings.new_profile_name("Default"), true);
    assert_eq!(settings.new_profile_name("Profile"), "Profile 2");
    let names: Vec<&str> = settings
        .user_profiles
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["Default", "Profile", "Default 2"]);
    // A new one is empty, a copy has the settings of the one in use, and none is switched to.
    let by_id = |id| settings.user_profiles.iter().find(|p| p.id == id).cloned();
    assert_eq!(by_id(work).map(|p| p.author_email), Some(String::new()));
    assert_eq!(
        by_id(copy).map(|p| p.author_email),
        Some("me@home".to_owned())
    );
    assert_eq!(settings.active_profile, 1);
    assert_eq!((work, copy), (2, 3));

    // The active one deleted: another takes over.
    settings.user_profiles.retain(|p| p.id != 1);
    settings.ensure_profile();
    assert_eq!(settings.active_profile, work);
    settings.user_profiles.clear();
    settings.ensure_profile();
    assert_eq!(settings.user_profiles.len(), 1);
}

/// How many rows a search finds on each page, with the page drawn nowhere.
fn found(query: &str) -> Vec<(Page, usize)> {
    found_with(query, None, "")
}

/// [`found`], with a previous version kept to go back to, and a version pinned.
fn found_with(query: &str, previous: Option<&str>, pinned: &str) -> Vec<(Page, usize)> {
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    let mut counts = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        counts = Page::ALL
            .iter()
            .map(|&page| {
                let filter = Filter::new(query, page.title(), true);
                let mut view = SettingsView {
                    theme: crate::theme::ThemeId::default(),
                    git_version: "2.45.0",
                    dialog_open: false,
                    update: UpdateCheck::NotYet,
                    check_updates_now: false,
                    previous: previous.map(str::to_owned),
                    previous_is_newer: false,
                    pinned: pinned.to_owned(),
                    go_back: false,
                    unpin: false,
                    request: None,
                };
                settings.page_ui(ui, page, &filter, &mut view);
                (page, filter.found())
            })
            .filter(|(_, n)| *n > 0)
            .collect();
    });
    // Nothing renders here: the font texture is left unapplied on purpose.
    output.textures_delta.clear();
    counts
}

#[test]
fn a_search_finds_settings_by_name_keyword_section_or_page() {
    // A keyword only, any case.
    assert_eq!(found("GRAVATAR"), vec![(Page::Appearance, 1)]);
    // Every word has to be there.
    assert_eq!(found("fetch background"), vec![(Page::Git, 1)]);
    // A section's name finds all its rows.
    assert_eq!(found("mergiraf"), vec![(Page::Git, 3)]);
    // Shortcuts by their keys or what they do.
    assert_eq!(found("reopen"), vec![(Page::Shortcuts, 1)]);
    // The profile's own settings are found too.
    assert_eq!(found("user.email"), vec![(Page::Profile, 1)]);
    assert_eq!(
        found("theme dark"),
        vec![(Page::Profile, 1), (Page::Appearance, 1)]
    );
    assert_eq!(found("ssh passphrase"), vec![(Page::Git, 1)]);
    assert!(found("nothing like this qqq").is_empty());
}

/// Every preview draws, with the settings as they come and pushed to their ends (every column,
/// the widest branch column, compact lanes, square corners and your own accent), in a dark and
/// a light theme; and every page draws with the preview beside it.
#[test]
fn every_preview_and_page_draws() {
    let ctx = egui::Context::default();
    // The semibold and medium faces are gitcraft's own; they take a frame to arrive.
    crate::theme::install_fonts(&ctx);
    // Nothing renders here: the font texture is left unapplied on purpose.
    let frame = |ui: &mut dyn FnMut(&mut egui::Ui)| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |u| ui(u));
        output.textures_delta.clear();
    };
    frame(&mut |_| {});
    let mut ends = Settings {
        refs_width: 2000.0,
        compact_graph: true,
        show_hash: true,
        show_date: true,
        diff_split: true,
        diff_whole_file: true,
        files_as_tree: false,
        avatars: false,
        shell: "pwsh".to_owned(),
        terminal_font: 20.0,
        ..Settings::default()
    };
    ends.theme_tweaks.roundness = 0.0;
    ends.theme_tweaks.accent = Some([200, 40, 120]);
    ends.theme_tweaks.lanes = crate::theme::LaneColors::ColorBlind;
    for theme_id in ["Dark", "catppuccin-latte", "no such theme"] {
        let theme_id = crate::theme::ThemeId::new(theme_id);
        for settings in [Settings::default(), ends.clone()] {
            crate::theme::apply(&ctx, &theme_id, &settings.theme_tweaks);
            frame(&mut |ui| {
                for preview in preview::Preview::ALL {
                    preview::panel(ui, preview, &settings, &theme_id);
                }
            });
            for page in Page::ALL {
                page::show(&ctx, page);
                let mut settings = settings.clone();
                frame(&mut |ui| {
                    let mut view = SettingsView {
                        theme: theme_id.clone(),
                        git_version: "2.45.0",
                        dialog_open: false,
                        update: UpdateCheck::NotYet,
                        check_updates_now: false,
                        previous: None,
                        previous_is_newer: false,
                        pinned: String::new(),
                        go_back: false,
                        unpin: false,
                        request: None,
                    };
                    settings.page(ui, &mut view);
                });
            }
        }
    }
}

#[test]
fn going_back_and_unpinning_show_only_when_they_can_be_done() {
    assert!(found("go back").is_empty());
    assert!(found("unpin").is_empty());
    assert_eq!(
        found_with("go back", Some("0.3.3"), ""),
        [(Page::General, 1)]
    );
    assert_eq!(found_with("unpin", None, "0.3.3"), [(Page::General, 1)]);
}

#[test]
fn localized_editable_number_parser_matches_formatter_and_rejects_malformed_grouping() {
    for (locale, grouped, decimal) in [("de", "1.234", "12,5"), ("en-US", "1,234", "12.5")] {
        crate::i18n::set_language(locale);
        assert_eq!(super::parse_localized_number(grouped), Some(1234.0));
        assert_eq!(super::parse_localized_number(decimal), Some(12.5));
        assert_eq!(
            super::parse_localized_number(&crate::i18n::number(1234.5)),
            Some(1234.5)
        );
        assert_eq!(super::parse_localized_number("-2"), Some(-2.0));
        for invalid in ["1..234", "1,,234", "NaN", "inf", "1.2.3,4", "1,2,3.4"] {
            assert_eq!(
                super::parse_localized_number(invalid),
                None,
                "{locale}: {invalid}"
            );
        }
    }
}

#[test]
fn area_d_optional_settings_persist_without_changing_permissions() {
    let old: Settings = ron::from_str("(agent_writes: true)").unwrap_or_default();
    assert!(!old.show_agents);
    assert!(old.agent_writes);
    assert_eq!(old.context_menus, ContextMenus::default());
    for agent_writes in [false, true] {
        let settings = Settings {
            show_agents: true,
            agent_writes,
            context_menus: ContextMenus {
                diagnostics: true,
                surgery: true,
                rehearsal: true,
                release: true,
            },
            ..Settings::default()
        };
        let mut storage = Memory::default();
        eframe::set_value(&mut storage, "settings", &settings);
        let loaded: Option<Settings> = eframe::get_value(&storage, "settings");
        assert_eq!(loaded, Some(settings));
    }
    assert_eq!(found("Show Agents section"), vec![(Page::Agents, 1)]);
    assert_eq!(found("commit surgery"), vec![(Page::ContextMenus, 1)]);
}

#[test]
fn area_d_optional_settings_pages_fit_and_toggle_at_each_scale() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    for width in [640.0, 1100.0, 1600.0] {
        for scale in [1.0, 1.25, 1.5] {
            for page in [Page::ContextMenus, Page::Agents] {
                let mut h = egui_kittest::Harness::builder()
                    .with_size([width / scale, 900.0 / scale])
                    .with_pixels_per_point(scale)
                    .build_ui_state(
                        move |ui, (settings, fonts): &mut (Settings, bool)| {
                            if !*fonts {
                                crate::theme::install_fonts(ui.ctx());
                                egui_extras::install_image_loaders(ui.ctx());
                                *fonts = true;
                                super::page::show(ui.ctx(), page);
                                return;
                            }
                            let theme = crate::theme::ThemeId::default();
                            crate::theme::apply(ui.ctx(), &theme, &settings.theme_tweaks);
                            let mut view = SettingsView {
                                theme,
                                git_version: "2.45.0",
                                dialog_open: false,
                                update: UpdateCheck::NotYet,
                                check_updates_now: false,
                                previous: None,
                                previous_is_newer: false,
                                pinned: String::new(),
                                go_back: false,
                                unpin: false,
                                request: None,
                            };
                            settings.page(ui, &mut view);
                        },
                        (Settings::default(), false),
                    );
                h.run();
                let label = if page == Page::Agents {
                    "Show Agents section"
                } else {
                    "Show split and move changes"
                };
                h.get_by_role_and_label(egui::accesskit::Role::CheckBox, label)
                    .click();
                h.run();
                assert!(h.state().0.agent_writes);
                assert_eq!(h.state().0.show_agents, page == Page::Agents);
                assert_eq!(
                    h.state().0.context_menus.surgery,
                    page == Page::ContextMenus
                );
                let checkbox = h
                    .get_by_role_and_label(egui::accesskit::Role::CheckBox, label)
                    .rect();
                assert!(
                    h.ctx.content_rect().contains_rect(checkbox),
                    "setting clipped at {width}/{scale}: {checkbox:?}"
                );
                if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let name = if page == Page::Agents {
                        "agents-settings"
                    } else {
                        "context-settings"
                    };
                    h.render()?
                        .save(format!("{dir}/{name}-{width:.0}-{scale:.2}.png"))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}
