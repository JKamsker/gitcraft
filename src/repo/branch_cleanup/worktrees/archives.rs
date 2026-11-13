//! The window's archives of this repository: each with its branch, files and size, Restore
//! into a worktree picked (the one of its branch at first) and Delete (asked twice).

use std::path::PathBuf;

use egui::{RichText, Ui};

use super::Window;
use super::window::Asks;
use crate::git::worktree_cleanup::archive::Store;
use crate::theme::{self, Tokens};

pub(super) fn show(
    ui: &mut Ui,
    window: &mut Window,
    worktrees: &[(PathBuf, String)],
    store: Option<&Store>,
    asks: &mut Asks,
) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.separator();
    ui.label(
        RichText::new(crate::i18n::text("worktree-cleanup-archives")).font(theme::semibold(13.0)),
    );
    let listed = match &window.archives {
        None => {
            ui.add(crate::spinner::Spinner::new().size(10.0));
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, e);
            return;
        }
        Some(Ok((listed, used))) => {
            if let Some(store) = store {
                ui.label(
                    RichText::new(store.usage_line(*used))
                        .color(t.text_dim)
                        .small(),
                );
            }
            listed.clone()
        }
    };
    if let Some((said, bad)) = &window.said {
        let color = if *bad { t.danger } else { t.success };
        ui.add(egui::Label::new(RichText::new(said).color(color).small()).wrap());
    }
    if listed.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("worktree-cleanup-no-archives")).color(t.text_dim),
        );
        return;
    }
    for archive in listed {
        let m = &archive.manifest;
        let into = window.restore_into.entry(m.id.clone()).or_insert_with(|| {
            worktrees
                .iter()
                .find(|(_, branch)| *branch == m.branch)
                .or(worktrees.first())
                .map(|(p, _)| p.clone())
                .unwrap_or_default()
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(archive.describe()).small())
                .on_hover_text(format!("{}\n{}", m.worktree, archive.zip.display()));
            let shown = into
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            egui::ComboBox::from_id_salt(("worktree-archive-into", &m.id))
                .selected_text(shown)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for (path, branch) in worktrees {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.selectable_value(into, path.clone(), format!("{name} · {branch}"));
                    }
                });
            let restore = egui::Button::new(crate::i18n::text("worktree-cleanup-restore")).small();
            if ui
                .add_enabled(!into.as_os_str().is_empty(), restore)
                .on_hover_text(crate::i18n::text("worktree-cleanup-restore-tip"))
                .clicked()
            {
                asks.restore = Some((archive.clone(), into.clone()));
            }
            let asking = window.armed.as_deref() == Some(m.id.as_str());
            let label = if asking {
                crate::i18n::text("worktree-cleanup-delete-confirm")
            } else {
                crate::i18n::text("worktree-cleanup-delete")
            };
            let color = if asking { t.danger } else { t.text };
            if ui
                .add(egui::Button::new(RichText::new(label).color(color)).small())
                .clicked()
            {
                if asking {
                    asks.delete = Some(archive.clone());
                } else {
                    window.armed = Some(m.id.clone());
                }
            }
        });
    }
}
