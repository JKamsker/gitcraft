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
                &[("error", e.as_str().into())],
            ))
            .color(t.danger)
            .small(),
        );
    }
}
