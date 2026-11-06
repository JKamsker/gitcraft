//! A `.dvc` file shown as the data it tracks, before and after the change, like files in Git
//! LFS: each path with its size, its hash and, for a folder, how many files it holds.

use egui::{RichText, Ui};

use super::dvc::{DvcDiff, Tracked};
use crate::git::human_size;
use crate::repo::Action;
use crate::theme::{self, Tokens};

pub(super) fn show(ui: &mut Ui, diff: &DvcDiff, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical()
        .id_salt("rich_dvc")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Data tracked by DVC")
                            .font(theme::medium(13.0))
                            .color(t.text),
                    );
                    ui.add_space(8.0);
                    for (k, (old, new)) in diff.pairs.iter().enumerate() {
                        ui.push_id(k, |ui| pair(ui, &t, old.as_ref(), new.as_ref(), acts));
                        ui.add_space(10.0);
                    }
                });
        });
}

fn pair(
    ui: &mut Ui,
    t: &Tokens,
    old: Option<&Tracked>,
    new: Option<&Tracked>,
    acts: &mut Vec<Action>,
) {
    let Some(any) = new.or(old) else { return };
    let state = match (old, new) {
        (None, _) => "now tracked",
        (_, None) => "no longer tracked",
        (Some(o), Some(n)) if o == n || (o.hash == n.hash && o.size == n.size) => "unchanged",
        _ => "changed",
    };
    ui.horizontal(|ui| {
        let path = ui
            .add(
                egui::Label::new(
                    RichText::new(&any.path)
                        .font(theme::mono(13.0))
                        .color(t.text),
                )
                .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Go to its line");
        if path.clicked() {
            let (o, n) = match new {
                Some(n) => (None, Some(n.line)),
                None => (Some(any.line), None),
            };
            acts.push(Action::DiffGoToLine { old: o, new: n });
        }
        let what = if any.list == "deps" {
            "imported from"
        } else {
            ""
        };
        if !what.is_empty() {
            ui.label(RichText::new(what).color(t.text_faint));
        }
        ui.label(RichText::new(state).color(t.text_dim));
    });
    ui.columns(2, |cols| {
        side(&mut cols[0], t, "Before", old);
        side(&mut cols[1], t, "After", new);
    });
}

fn side(ui: &mut Ui, t: &Tokens, title: &str, tracked: Option<&Tracked>) {
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).color(t.text_faint).small());
            let Some(x) = tracked else {
                ui.label(RichText::new("not tracked").color(t.text_faint));
                return;
            };
            let size = x.size.map_or("size not recorded".to_owned(), human_size);
            ui.label(RichText::new(size).font(theme::semibold(16.0)));
            if let Some(files) = x.files {
                ui.label(RichText::new(format!("a folder of {files} files")).color(t.text_dim));
            }
            if let Some(hash) = &x.hash {
                ui.label(
                    RichText::new(format!("hash {}", hash.get(..12).unwrap_or(hash)))
                        .color(t.text_faint)
                        .small(),
                )
                .on_hover_text(hash);
            }
        });
}
