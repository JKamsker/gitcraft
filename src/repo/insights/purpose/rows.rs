//! A group and its changes as rows: the header with what the group is and why, its menu
//! (rename, reorder, merge, confirm, split off the selection), and each change, which opens
//! in the pull request's diff at its line, with the threads on it.

use std::collections::BTreeSet;

use egui::{Id, RichText, Ui};

use super::arrange::{Arranged, Edit, Placed, Shown};
use super::rules::Purpose;
use super::units::{Body, Unit, Units};
use super::{Act, act};
use crate::pr_threads::Thread;
use crate::pull_request::PrFiles;
use crate::repo::insights::{InsightsAct, Kind};
use crate::repo::{Action, DiffKey};
use crate::theme::Tokens;

/// What every row is drawn with.
pub(super) struct Ctx<'a> {
    pub(super) units: &'a Units,
    pub(super) arranged: &'a Arranged,
    pub(super) files: &'a PrFiles,
    pub(super) threads: &'a [Thread],
    pub(super) selected: &'a BTreeSet<String>,
    pub(super) open: Option<&'a DiffKey>,
}

/// The diff a unit opens in: the pull request's own, as its standard list opens it.
pub(super) fn key_of(files: &PrFiles, unit: &Unit) -> DiffKey {
    DiffKey::Range {
        from: files.from.clone(),
        to: Some(files.to.clone()),
        path: unit.path.clone(),
    }
}

/// Where in that diff: its first new line, or its first old one for a removal.
pub(super) fn line_of(unit: &Unit) -> Option<(bool, u32)> {
    match unit.body {
        Body::Hunk {
            new_len: 0,
            old_start,
            ..
        } => Some((true, old_start.max(1))),
        Body::Hunk { new_start, .. } => Some((false, new_start.max(1))),
        Body::File(_) => None,
    }
}

/// The actions that open `unit`.
pub(super) fn open(files: &PrFiles, unit: &Unit) -> Vec<Action> {
    let key = key_of(files, unit);
    let mut acts = vec![Action::OpenDiff(key.clone()), Action::UnfoldDiff];
    if let Some(at) = line_of(unit) {
        acts.push(act(Act::Jump { key, at }));
    }
    acts
}

/// The open threads on `unit`'s lines (or its file, for a whole file).
pub(super) fn threads_on(threads: &[Thread], unit: &Unit) -> usize {
    threads
        .iter()
        .filter(|t| !t.resolved && !t.outdated)
        .filter(|t| t.path == unit.path || unit.old_path.as_deref() == Some(t.path.as_str()))
        .filter(|t| match (unit.body.clone(), t.line) {
            (Body::File(_), _) | (_, None) => true,
            (
                Body::Hunk {
                    old_start,
                    old_len,
                    new_start,
                    new_len,
                },
                Some(line),
            ) => {
                let (start, len) = if t.side == crate::pull_request::DiffSide::Left {
                    (old_start, old_len)
                } else {
                    (new_start, new_len)
                };
                let from = u64::from(start.max(1));
                line >= from && line < from + u64::from(len.max(1))
            }
        })
        .count()
}

pub(super) fn group(ui: &mut Ui, ctx: &Ctx, g: usize, shown: &Shown, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let id = &shown.group.id;
    let rename = Id::new(("purpose-rename", id));
    let mark = if shown.yours || shown.renamed {
        "  ✎"
    } else {
        ""
    };
    let title = format!("{}  ·  {}{mark}", shown.group.title, shown.group.units.len());
    let header = egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(("purpose-group", id))
        .default_open(true)
        .show(ui, |ui| {
            if let Some(mut text) = ui.data(|d| d.get_temp::<String>(rename)) {
                ui.horizontal(|ui| {
                    let r = ui.text_edit_singleline(&mut text);
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter || ui.button("Rename").clicked() {
                        acts.push(act(Act::Edit(Edit::Rename {
                            group: id.clone(),
                            title: text.clone(),
                        })));
                        ui.data_mut(|d| d.remove::<String>(rename));
                    } else if ui.button("Cancel").clicked() {
                        ui.data_mut(|d| d.remove::<String>(rename));
                    } else {
                        ui.data_mut(|d| d.insert_temp(rename, text));
                    }
                });
            }
            ui.label(RichText::new(&shown.group.why).small().color(t.text_faint));
            if let Some(kind) = insight_of(shown.group.purpose)
                && ui.small_button(format!("Open in {}", kind.title())).clicked()
            {
                acts.push(Action::Insights(InsightsAct::Open(kind)));
            }
            for &u in &shown.group.units {
                unit_row(ui, ctx, u, g, acts);
            }
        });
    header.header_response.context_menu(|ui| {
        if ui.button("Rename…").clicked() {
            ui.data_mut(|d| d.insert_temp(rename, shown.group.title.clone()));
            ui.close();
        }
        for (label, up) in [("Move up", true), ("Move down", false)] {
            if ui.button(label).clicked() {
                acts.push(act(Act::Edit(Edit::Shift {
                    group: id.clone(),
                    up,
                })));
                ui.close();
            }
        }
        if ui.button("Confirm these changes belong here").clicked() {
            acts.push(act(Act::Edit(Edit::Confirm { group: id.clone() })));
            ui.close();
        }
        let picked: Vec<String> = shown
            .group
            .units
            .iter()
            .map(|&u| ctx.units.units[u].key.clone())
            .filter(|k| ctx.selected.contains(k))
            .collect();
        let split = format!("Split the {} selected into a new group", picked.len());
        if ui.add_enabled(!picked.is_empty(), egui::Button::new(split)).clicked() {
            acts.push(act(Act::Edit(Edit::Split {
                units: picked,
                from: id.clone(),
            })));
            ui.close();
        }
        ui.menu_button("Merge into", |ui| {
            for other in ctx.arranged.groups.iter().filter(|o| &o.group.id != id) {
                if ui.button(&other.group.title).clicked() {
                    acts.push(act(Act::Edit(Edit::Merge {
                        from: id.clone(),
                        into: other.group.id.clone(),
                    })));
                    ui.close();
                }
            }
        });
    });
}

/// The insight that shows more of a group's kind of change.
fn insight_of(purpose: Option<Purpose>) -> Option<Kind> {
    match purpose? {
        Purpose::Repeated | Purpose::Formatting => Some(Kind::Transform),
        Purpose::Api | Purpose::Logic => Some(Kind::Impact),
        Purpose::Tests => Some(Kind::Coverage),
        _ => None,
    }
}

fn unit_row(ui: &mut Ui, ctx: &Ctx, u: usize, g: usize, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let unit = &ctx.units.units[u];
    let placed = &ctx.arranged.placed[u];
    ui.horizontal(|ui| {
        let mut on = ctx.selected.contains(&unit.key);
        if ui
            .checkbox(&mut on, "")
            .on_hover_text("Select, to split off with others")
            .changed()
        {
            acts.push(act(Act::Select(unit.key.clone(), on)));
        }
        let name = match &unit.old_path {
            Some(old) if old != &unit.path => format!("{old} → {}", unit.path),
            _ => unit.path.clone(),
        };
        let place = match (&unit.body, line_of(unit)) {
            (Body::File(why), _) => format!("whole file, {}", why.label()),
            (_, Some((old, n))) => format!("{}{n}  {}", if old { "old line " } else { "line " }, unit.preview),
            _ => String::new(),
        };
        let mut text = format!("{name}  +{} −{}", unit.added, unit.removed);
        let threads = threads_on(ctx.threads, unit);
        if threads > 0 {
            text.push_str(&format!("  · {threads} open thread{}", if threads == 1 { "" } else { "s" }));
        }
        if *placed == Placed::Yours {
            text.push_str("  ✓");
        }
        let selected = ctx.open == Some(&key_of(ctx.files, unit));
        let why = match placed {
            Placed::Rule(why) => format!("Placed by the rules: {why}"),
            Placed::Yours => "Placed here by you".to_owned(),
        };
        let r = ui
            .vertical(|ui| {
                let r = ui.selectable_label(selected, RichText::new(text).small());
                ui.label(RichText::new(place).small().color(t.text_faint));
                r
            })
            .inner
            .on_hover_text(why);
        if r.clicked() {
            acts.extend(open(ctx.files, unit));
        }
        r.context_menu(|ui| {
            ui.menu_button("Move to", |ui| {
                for (o, other) in ctx.arranged.groups.iter().enumerate().filter(|(o, _)| *o != g) {
                    if ui.button(&other.group.title).clicked() {
                        acts.push(act(Act::Edit(Edit::Move {
                            units: vec![unit.key.clone()],
                            to: ctx.arranged.groups[o].group.id.clone(),
                        })));
                        ui.close();
                    }
                }
            });
            if ui.button("Move to a new group").clicked() {
                acts.push(act(Act::Edit(Edit::Split {
                    units: vec![unit.key.clone()],
                    from: ctx.arranged.groups[g].group.id.clone(),
                })));
                ui.close();
            }
        });
    });
}
