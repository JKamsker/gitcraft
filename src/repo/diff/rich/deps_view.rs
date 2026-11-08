//! Dependency changes shown: a summary, then one row per package upgraded, downgraded, added,
//! removed or relicensed, with links to its registry page and to what changed between its
//! versions. A click on the package goes to its line in the lockfile.

use egui::{Color32, RichText, Ui};

use super::deps::{Change, DepChange, DepsDiff};
use crate::repo::Action;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 24.0;

pub(super) fn show(
    ui: &mut Ui,
    diff: &DepsDiff,
    metadata: &mut super::dependency_metadata::State,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let count = |c: Change| diff.changes.iter().filter(|d| d.change == c).count();
    let licences = diff
        .changes
        .iter()
        .filter(|d| matches!(&d.license, Some((Some(_), _))))
        .count();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("Dependencies · {}", diff.file))
                        .font(theme::medium(12.5))
                        .color(t.text),
                );
                let parts: Vec<String> = [
                    (count(Change::Upgraded), "upgraded"),
                    (count(Change::Downgraded), "downgraded"),
                    (count(Change::Added), "added"),
                    (count(Change::Removed), "removed"),
                    (licences, "with a new licence"),
                ]
                .iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, what)| format!("{n} {what}"))
                .collect();
                let summary = if parts.is_empty() {
                    "the same packages and versions".to_owned()
                } else {
                    parts.join(" · ")
                };
                ui.label(RichText::new(summary).color(t.text_dim));
            });
        });
    metadata.show(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::both()
        .id_salt("rich_deps")
        .auto_shrink(false)
        .show_rows(ui, ROW_H, diff.changes.len(), |ui, range| {
            for (k, d) in diff.changes[range.clone()].iter().enumerate() {
                ui.push_id(range.start + k, |ui| row(ui, &t, diff, d, metadata, acts));
            }
        });
}

fn row(
    ui: &mut Ui,
    t: &Tokens,
    diff: &DepsDiff,
    d: &DepChange,
    metadata: &mut super::dependency_metadata::State,
    acts: &mut Vec<Action>,
) {
    let (mark, color, fill) = match d.change {
        Change::Upgraded => ("↑", t.accent, Color32::TRANSPARENT),
        Change::Downgraded => ("↓", t.warning, Color32::TRANSPARENT),
        Change::Added => ("+", t.success, t.diff_add_bg),
        Change::Removed => ("−", t.danger, t.diff_del_bg),
        Change::Relicensed => ("§", t.warning, Color32::TRANSPARENT),
    };
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, fill);
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(16.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut inner;
    ui.label(RichText::new(mark).font(theme::mono(12.5)).color(color));
    let name = ui
        .add(
            egui::Label::new(RichText::new(&d.name).font(theme::mono(12.5)).color(t.text))
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let versions = match (&d.old, &d.new) {
        (Some(o), Some(n)) if o != n => format!("{o} → {n}"),
        (_, Some(n)) => n.clone(),
        (Some(o), None) => o.clone(),
        _ => String::new(),
    };
    ui.label(
        RichText::new(versions)
            .font(theme::mono(12.0))
            .color(t.text_dim),
    );
    if let Some((old, new)) = &d.license {
        let text = match (old, new) {
            (Some(o), Some(n)) => format!("licence {o} → {n}"),
            (None, Some(n)) => format!("licence {n}"),
            (Some(o), None) => format!("licence {o} → none given"),
            _ => String::new(),
        };
        let c = if old.is_some() {
            t.warning
        } else {
            t.text_faint
        };
        ui.label(RichText::new(text).color(c));
    }
    let tip = match (d.change, d.old_line, d.new_line) {
        (Change::Removed, Some(o), _) => {
            format!("Go to line {o} of the lockfile before the change")
        }
        (_, _, Some(n)) => format!("Go to line {n} of the lockfile"),
        _ => String::new(),
    };
    if name.on_hover_text(tip).clicked() {
        let (old, new) = match d.change {
            Change::Removed => (d.old_line, None),
            _ => (None, d.new_line),
        };
        acts.push(Action::DiffGoToLine { old, new });
    }
    ui.add_space(8.0);
    // Links for packages of a registry, not the project's own.
    let (old_reg, new_reg) = &d.registry;
    if let Some((name, v)) = new_reg.as_ref().or(old_reg.as_ref()) {
        let (url, site) = diff.ecosystem.registry(name, v);
        if ui.link(site).on_hover_text(&url).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
    if let (Some((on, o)), Some((nn, n))) = (old_reg, new_reg)
        && on == nn
        && o != n
    {
        let (url, site) = diff.ecosystem.changes(nn, o, n);
        if ui
            .link("what changed")
            .on_hover_text(format!("{site}: {url}"))
            .clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
    if matches!(
        diff.ecosystem,
        super::deps::Ecosystem::Cargo | super::deps::Ecosystem::Npm
    ) && (d.registry.0.is_some() || d.registry.1.is_some())
        && ui.link("Changelog & licences").clicked()
    {
        metadata.select(diff.ecosystem, d, ui.ctx());
    }
}
