//! The comparison of two versions in the big area: the pairs, old commit on the left and new on
//! the right, those that didn't change faded; below them, how the patch of the pair clicked
//! changed.

use egui::{Align2, Color32, Rect, RichText, Sense, Ui, pos2, vec2};

use super::{VersionsAct, VersionsView, summary};
use crate::git::{Pair, PairKind, SeriesCommit};
use crate::icons;
use crate::repo::{Action, RepoTab, diff, short, widgets};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 26.0;
/// The column between the two sides, with the pair's mark.
const MARK_W: f32 = 76.0;

impl RepoTab {
    /// The big area while a comparison of versions is open.
    pub(in crate::repo) fn versions_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        // The pair just clicked is scrolled to (the list shrinks for its patch below).
        let reveal = self
            .versions
            .view
            .as_mut()
            .is_some_and(|v| std::mem::take(&mut v.reveal));
        let Some(view) = &self.versions.view else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        header(ui, view, acts);
        labels(ui, view);
        match &view.data {
            None => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
            }
            Some(Err(e)) => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("Couldn't compare the versions: {e}"))
                                .color(t.danger),
                        )
                        .wrap(),
                    )
                });
            }
            Some(Ok(data)) if data.pairs.is_empty() => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Neither version has commits of its own.").color(t.text_dim),
                    )
                });
            }
            Some(Ok(data)) => {
                // The pairs take the top when one is open below them, else all of it.
                let list_h = if view.selected.is_some() {
                    (ui.available_height() * 0.4).max(ROW_H * 3.0)
                } else {
                    ui.available_height()
                };
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("versions-pairs")
                    .max_height(list_h)
                    .auto_shrink([false, true]);
                if let Some(i) = view.selected.filter(|_| reveal) {
                    let at = i as f32 * ROW_H - (list_h - ROW_H) / 2.0;
                    scroll = scroll.vertical_scroll_offset(at.max(0.0));
                }
                scroll.show_rows(ui, ROW_H, data.pairs.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in range {
                        let selected = view.selected == Some(i);
                        pair_row(ui, &data.pairs[i], i, selected, view.sides, acts);
                    }
                });
                if let Some(pair) = view.selected.and_then(|i| data.pairs.get(i)) {
                    ui.separator();
                    interdiff(ui, pair, view.interdiff.as_ref());
                }
            }
        }
    }
}

/// The title bar: what is compared, how it came out, and the button to leave.
fn header(ui: &mut Ui, view: &VersionsView, acts: &mut Vec<Action>) {
    widgets::panel_header(ui, &view.title, |ui| {
        if icons::small_button(ui, "x", "Close the comparison (Esc)").clicked() {
            acts.push(Action::Versions(VersionsAct::Close));
        }
    });
}

/// Which version is which, and the summary of the pairs.
fn labels(ui: &mut Ui, view: &VersionsView) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let side = |ui: &mut Ui, label: &str, base: &str, tip: &str| {
                    ui.label(RichText::new(label).font(theme::medium(12.5)).color(t.text))
                        .on_hover_text(format!("{}..{}", short(base), short(tip)));
                };
                side(ui, &view.old_label, &view.old.base, &view.old.tip);
                ui.label(RichText::new("→").color(t.text_faint));
                side(ui, &view.new_label, &view.new.base, &view.new.tip);
                if let Some(Ok(data)) = &view.data {
                    ui.add_space(12.0);
                    let counts = data.counts();
                    let color = if counts.identical() || view.sides {
                        t.text_dim
                    } else {
                        t.warning
                    };
                    ui.label(RichText::new(summary(&counts)).color(color));
                }
            });
        });
}

/// What a pair's mark says, and its colour. `sides`: two branches neither of which is the old
/// one, so a commit is only on the left or the right rather than added or dropped.
fn mark(pair: &Pair, sides: bool, t: &Tokens) -> (&'static str, Color32, &'static str) {
    match pair.kind {
        PairKind::Same if pair.moved => ("moved", t.accent, "The same patch, in another place"),
        PairKind::Same => ("=", t.text_faint, "The same patch and message"),
        PairKind::Changed if pair.moved => (
            "changed ↕",
            t.warning,
            "The patch or the message changed, and it is in another place",
        ),
        PairKind::Changed => ("changed", t.warning, "The patch or the message changed"),
        PairKind::Added if sides => ("right only", t.success, "Only on the right"),
        PairKind::Added => ("added", t.success, "Only in the new version"),
        PairKind::Dropped if sides => ("left only", t.danger, "Only on the left"),
        PairKind::Dropped => ("dropped", t.danger, "Only in the old version"),
    }
}

/// A commit as a tooltip names it.
fn named(c: Option<&SeriesCommit>) -> String {
    c.map_or_else(
        || "—".to_owned(),
        |c| {
            let empty = if c.empty { " (changes nothing)" } else { "" };
            format!("{} {}{empty}", short(&c.hash), c.summary)
        },
    )
}

/// One pair: the old commit, the mark, the new commit. Unchanged ones fade.
fn pair_row(
    ui: &mut Ui,
    pair: &Pair,
    i: usize,
    selected: bool,
    sides: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let (rect, r) = widgets::keyed_row(ui, ROW_H, ("versions-pair", i));
    if selected {
        ui.painter().rect_filled(rect, 0.0, t.row_selected);
    } else if r.hovered() {
        ui.painter().rect_filled(rect, 0.0, t.hover);
    }
    let faded = pair.kind == PairKind::Same && !pair.moved;
    let (word, color, tip) = mark(pair, sides, &t);
    let half = ((rect.width() - MARK_W) / 2.0).max(0.0);
    let left = Rect::from_min_size(rect.min, vec2(half, ROW_H));
    let middle = Rect::from_min_size(pos2(left.right(), rect.top()), vec2(MARK_W, ROW_H));
    let right = Rect::from_min_max(pos2(middle.right(), rect.top()), rect.max);
    commit_cell(ui, left, pair.old.as_ref(), faded);
    ui.painter().text(
        middle.center(),
        Align2::CENTER_CENTER,
        word,
        theme::medium(11.0),
        color,
    );
    commit_cell(ui, right, pair.new.as_ref(), faded);
    let action = if selected {
        "Click to hide its patch"
    } else {
        "Click to see how its patch changed"
    };
    let r = r.on_hover_text(format!(
        "{tip}\nLeft: {}\nRight: {}\n{action}",
        named(pair.old.as_ref()),
        named(pair.new.as_ref())
    ));
    if r.clicked() {
        acts.push(Action::Versions(VersionsAct::Select(i)));
    }
}

/// A commit of one side: its place in its series, hash and summary (nothing for none).
fn commit_cell(ui: &Ui, rect: Rect, commit: Option<&SeriesCommit>, faded: bool) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter_at(rect.shrink2(vec2(8.0, 0.0)));
    let y = rect.center().y;
    let Some(c) = commit else {
        p.text(
            pos2(rect.left() + 12.0, y),
            Align2::LEFT_CENTER,
            "—",
            theme::mono(11.5),
            t.text_faint,
        );
        return;
    };
    let (text, dim) = if faded {
        (t.text_faint, t.text_faint)
    } else {
        (t.text, t.text_dim)
    };
    let number = p.text(
        pos2(rect.left() + 12.0, y),
        Align2::LEFT_CENTER,
        format!("{}.", c.index),
        theme::mono(11.0),
        t.text_faint,
    );
    let hash = p.text(
        pos2(number.right() + 6.0, y),
        Align2::LEFT_CENTER,
        short(&c.hash),
        theme::mono(11.5),
        dim,
    );
    // Before the summary, which is what gets cut when there is no room.
    let summary = if c.empty {
        format!("(empty) {}", c.summary)
    } else {
        c.summary.clone()
    };
    p.text(
        pos2(hash.right() + 8.0, y),
        Align2::LEFT_CENTER,
        summary,
        egui::FontId::proportional(12.5),
        text,
    );
}

/// How the patch of the pair clicked changed.
fn interdiff(ui: &mut Ui, pair: &Pair, data: Option<&Result<diff::DiffData, String>>) {
    let t = Tokens::get(ui.ctx());
    let what = match pair.kind {
        PairKind::Same => "Its patch is the same in both versions",
        PairKind::Changed => "How its patch changed",
        PairKind::Added => "Its patch, only in the new version",
        PairKind::Dropped => "Its patch, only in the old version",
    };
    let summary = pair
        .new
        .as_ref()
        .or(pair.old.as_ref())
        .map(|c| c.summary.as_str())
        .unwrap_or_default();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(what).font(theme::medium(12.5)).color(t.text));
                ui.add(egui::Label::new(RichText::new(summary).color(t.text_dim)).truncate());
            });
        });
    match data {
        None => {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, format!("Couldn't read the patch: {e}"));
        }
        Some(Ok(data)) if data.lines.is_empty() => {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                let note = if pair.kind == PairKind::Same {
                    "Nothing changed: the same message and the same changes."
                } else {
                    "No changes to show."
                };
                ui.label(RichText::new(note).color(t.text_faint));
            });
        }
        Some(Ok(data)) => {
            let id = (
                pair.old.as_ref().map(|c| c.hash.clone()),
                pair.new.as_ref().map(|c| c.hash.clone()),
            );
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            diff::interdiff_rows(&mut inner, data, id);
        }
    }
}
