//! The General page's rows that add History in gitcraft and Blame in gitcraft to the file
//! manager's menu of files where no installer does: Finder (macOS) and Files (Linux).

use egui::{RichText, Ui};

use super::layout::{self, Card};
use crate::theme::Tokens;

/// One file manager's row: its actions added or removed with a click, and why that failed.
struct Menu {
    label: &'static str,
    keywords: &'static str,
    note: &'static str,
    installed: fn() -> bool,
    install: Box<dyn Fn() -> Result<(), String>>,
    uninstall: fn() -> Result<(), String>,
}

pub(super) fn file_menu_rows(card: &mut Card) {
    let mut menus = Vec::new();
    if crate::finder_actions::bundle().is_some() {
        menus.push(Menu {
            label: "Finder",
            keywords: "finder quick action services history blame menu files",
            note: crate::i18n::text("interaction-settings-finder-note"),
            installed: crate::finder_actions::installed,
            install: Box::new(crate::finder_actions::install),
            uninstall: crate::finder_actions::uninstall,
        });
    }
    if let Some(program) = crate::nautilus_scripts::program() {
        menus.push(Menu {
            label: "Files (GNOME)",
            keywords: "nautilus files gnome scripts history blame menu",
            note: crate::i18n::text("interaction-settings-files-note"),
            installed: crate::nautilus_scripts::installed,
            install: Box::new(move || crate::nautilus_scripts::install(&program)),
            uninstall: crate::nautilus_scripts::uninstall,
        });
    }
    for menu in menus {
        card.row(menu.label, menu.keywords, |ui| menu_row(ui, &menu));
    }
}

fn menu_row(ui: &mut Ui, menu: &Menu) {
    let error_id = egui::Id::new(("settings-file-menu-error", menu.label));
    let result = if (menu.installed)() {
        ui.button(crate::i18n::text("interaction-settings-remove-file-menu"))
            .clicked()
            .then(|| (menu.uninstall)())
    } else {
        ui.button(crate::i18n::text("interaction-settings-add-file-menu"))
            .clicked()
            .then(|| (menu.install)())
    };
    match result {
        Some(Err(e)) => ui.data_mut(|d| {
            d.insert_temp(error_id, e);
        }),
        Some(Ok(())) => ui.data_mut(|d| d.remove::<String>(error_id)),
        None => {}
    }
    layout::note(ui, menu.note);
    if let Some(e) = ui.data(|d| d.get_temp::<String>(error_id)) {
        let t = Tokens::get(ui.ctx());
        ui.label(
            RichText::new(crate::i18n::message(
                "interaction-settings-file-menu-error",
                &[("error", display_error(&e).into())],
            ))
            .color(t.danger)
            .small(),
        );
    }
}

/// The fixed application guard is known; native/path-bearing details stay opaque.
fn display_error(raw: &str) -> &str {
    match raw {
        "No home folder" => crate::i18n::text("interaction-shell-prose-home"),
        _ => raw,
    }
}

#[cfg(test)]
mod localized_error_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_supplied_file_menu_row_translates_at_paint_with_raw_identity_and_callbacks_untouched()
    {
        let menu = Menu {
            label: "Files (GNOME)",
            keywords: "nautilus files gnome",
            note: "",
            installed: || true,
            install: Box::new(|| Err("recorded install must not be called".into())),
            uninstall: || Err("recorded uninstall must not be called".into()),
        };
        let raw = "No home folder";
        let error_id = egui::Id::new(("settings-file-menu-error", menu.label));
        crate::i18n::set_language("en-US");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, original: &mut String| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                ui.data_mut(|d| d.insert_temp(error_id, original.clone()));
                let filter = layout::Filter::new("", "General", false);
                layout::section(ui, &filter, "General", None, |card| {
                    card.row(menu.label, menu.keywords, |ui| menu_row(ui, &menu))
                });
            },
            raw.to_owned(),
        );
        h.run();
        h.get_by_label("Files (GNOME)");
        crate::i18n::set_language("de");
        h.step();
        h.get_by_label("Dateien (GNOME)");
        assert_eq!(display_error(raw), "Kein Benutzerordner");
        h.get_by_label("Änderung fehlgeschlagen: Kein Benutzerordner");
        assert_eq!(h.state(), raw);
        // The exact raw widget/error identity survives the localized title.
        assert_eq!(
            h.ctx.data(|d| d.get_temp::<String>(error_id)).as_deref(),
            Some(raw)
        );
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Files (GNOME)");
        h.get_by_label("Couldn't change it: No home folder");
        assert_eq!(h.state(), raw);
        assert_eq!(
            display_error("native IO: No home folder"),
            "native IO: No home folder"
        );
    }
}
