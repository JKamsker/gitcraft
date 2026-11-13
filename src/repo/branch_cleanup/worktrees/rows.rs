//! A worktree in the window: its box (only when nothing keeps it), folder, branch and merge
//! evidence, what removing it gives back, what keeps it, and its files on demand.

use egui::{RichText, Ui};

use super::Row;
use crate::git::worktree_cleanup::{Inspection, Kind, Untracked, describe_size};
use crate::theme::{self, Tokens};

/// Files listed per kind when the details are open.
const LISTED: usize = 100;

pub(super) fn show(ui: &mut Ui, row: &mut Row, archive: bool) {
    let t = Tokens::get(ui.ctx());
    let blockers = match &row.found {
        Some(Ok(found)) => Some(found.blockers_with(archive)),
        _ => None,
    };
    let free = blockers.as_ref().is_some_and(Vec::is_empty);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = row
                    .target
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let tick = ui.add_enabled(free, egui::Checkbox::without_text(&mut row.ticked));
                crate::a11y::a11y(&tick, egui::WidgetType::Checkbox, &name);
                ui.add(egui::Label::new(RichText::new(&name).font(theme::medium(12.5))).truncate());
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} · {}",
                            row.target.branch, row.target.evidence.describe
                        ))
                        .color(t.text_dim)
                        .small(),
                    )
                    .truncate(),
                );
            });
            ui.add(
                egui::Label::new(
                    RichText::new(row.target.path.to_string_lossy())
                        .font(theme::mono(11.0))
                        .color(t.text_faint),
                )
                .wrap()
                .selectable(true),
            );
            match &row.found {
                None => {
                    ui.add(crate::spinner::Spinner::new().size(10.0));
                }
                Some(Err(e)) => {
                    ui.colored_label(t.danger, e);
                }
                Some(Ok(found)) => {
                    ui.label(RichText::new(summary(found)).color(t.text_dim).small());
                    for blocker in blockers.iter().flatten() {
                        ui.horizontal(|ui| {
                            crate::icons::inline(ui, "circle-alert", 12.0, t.danger);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(blocker.describe()).color(t.danger).small(),
                                )
                                .wrap(),
                            );
                        });
                    }
                    if details(ui, row.open, found, &t) {
                        row.open = !row.open;
                    }
                }
            }
        });
    ui.add_space(4.0);
}

/// "Gives back 12.3 MB · 2 changed · 3 untracked (2 text) · 1 ignored (4.1 MB)".
pub(super) fn summary(found: &Inspection) -> String {
    let text = found.untracked.iter().filter(|u| u.archivable()).count();
    crate::i18n::message(
        "worktree-cleanup-summary",
        &[
            ("size", describe_size(found.footprint).into()),
            ("changed", found.changed.len().into()),
            ("untracked", found.untracked.len().into()),
            ("text", text.into()),
            ("ignored", found.ignored.len().into()),
            ("ignored-size", describe_size(found.ignored_size).into()),
        ],
    )
}

/// The files, folded; whether the fold was clicked.
fn details(ui: &mut Ui, open: bool, found: &Inspection, t: &Tokens) -> bool {
    let any = !found.changed.is_empty() || !found.untracked.is_empty() || !found.ignored.is_empty();
    if !any {
        return false;
    }
    let label = if open {
        crate::i18n::text("worktree-cleanup-hide-files")
    } else {
        crate::i18n::text("worktree-cleanup-show-files")
    };
    let clicked = ui.link(RichText::new(label).small()).clicked();
    if open {
        let line = |ui: &mut Ui, text: String, color| {
            ui.add(egui::Label::new(RichText::new(text).color(color).small()).truncate());
        };
        for path in found.changed.iter().take(LISTED) {
            line(ui, format!("M  {path}"), t.warning);
        }
        for file in found.untracked.iter().take(LISTED) {
            let color = if file.archivable() {
                t.text_dim
            } else {
                t.danger
            };
            line(ui, untracked_line(file), color);
        }
        for path in found.ignored.iter().take(LISTED) {
            line(ui, format!("!! {path}"), t.text_faint);
        }
        let more = [
            found.changed.len(),
            found.untracked.len(),
            found.ignored.len(),
        ]
        .iter()
        .map(|n| n.saturating_sub(LISTED))
        .sum::<usize>();
        if more > 0 {
            line(
                ui,
                crate::i18n::message("worktree-cleanup-more-files", &[("count", more.into())]),
                t.text_faint,
            );
        }
    }
    clicked
}

/// "?? notes.txt (1.2 KB) · archived" or "… · not archived: binary".
pub(super) fn untracked_line(file: &Untracked) -> String {
    let why = match &file.kind {
        Kind::Text(_) => "worktree-cleanup-file-text",
        Kind::Binary => "worktree-cleanup-file-binary",
        Kind::TooLarge => "worktree-cleanup-file-large",
        Kind::Link => "worktree-cleanup-file-link",
        Kind::Other => "worktree-cleanup-file-other",
    };
    format!(
        "?? {} ({}) · {}",
        file.path,
        crate::git::worktree_cleanup::describe_size(crate::git::worktree_cleanup::Footprint {
            files: 1,
            bytes: file.size,
            complete: true,
        }),
        crate::i18n::text(why)
    )
}
