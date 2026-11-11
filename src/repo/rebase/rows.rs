//! The rows of the interactive rebase editor, newest on top like the graph: each commit with
//! its action, a handle to drag it to another place, and the keys that act on the selected row
//! (↑ / ↓ select, Alt+↑ / Alt+↓ move it, P R E S F D set its action as git's todo list
//! abbreviates them).

use egui::{Key, Rect, Sense, Stroke, Ui};

use super::{RebaseEditor, Step, short};
use crate::icons;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

/// What a dragged row carries: the editor it belongs to and its place in `entries`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DraggedRow {
    editor: u64,
    index: usize,
}

impl Step {
    pub(super) fn command(self) -> crate::keymap::Command {
        use crate::keymap::Command;
        match self {
            Self::Pick => Command::RebasePick,
            Self::Reword => Command::RebaseReword,
            Self::Edit => Command::RebaseEdit,
            Self::Squash => Command::RebaseSquash,
            Self::Fixup => Command::RebaseFixup,
            Self::Drop => Command::RebaseDrop,
        }
    }
    /// The action a key sets, as git's todo list abbreviates it.
    pub(super) fn for_key(key: Key) -> Option<Step> {
        Some(match key {
            Key::P => Step::Pick,
            Key::R => Step::Reword,
            Key::E => Step::Edit,
            Key::S => Step::Squash,
            Key::F => Step::Fixup,
            Key::D => Step::Drop,
            _ => return None,
        })
    }
}

/// Where entry `from` ends up when dropped on entry `target`, on its upper half (`above`) or
/// its lower one; `None` when that is where it already is. `entries` run oldest first and the
/// list shows them newest on top, so above a row is after it.
pub(super) fn drop_index(from: usize, target: usize, above: bool) -> Option<usize> {
    let slot = if above { target + 1 } else { target };
    let to = if from < slot { slot - 1 } else { slot };
    (to != from).then_some(to)
}

/// Where the row at `i` is once the row at `from` has moved to `to`.
fn follow(i: usize, from: usize, to: usize) -> usize {
    if i == from {
        to
    } else if from < i && i <= to {
        i - 1
    } else if to <= i && i < from {
        i + 1
    } else {
        i
    }
}

/// The row under height `y` among `rows` (each entry's index and where it is drawn), and
/// whether `y` is in its upper half. Past the first or the last row, that row.
fn row_at(rows: &[(usize, Rect)], y: f32) -> Option<(usize, bool)> {
    let (first, last) = (rows.first()?, rows.last()?);
    if y < first.1.top() {
        return Some((first.0, true));
    }
    if y > last.1.bottom() {
        return Some((last.0, false));
    }
    rows.iter()
        .find(|(_, r)| y >= r.top() && y <= r.bottom())
        .map(|(i, r)| (*i, y < r.center().y))
}

impl RebaseEditor {
    /// Moves entry `from` to `to`, those between shifting by one; the selection follows the
    /// rows.
    pub(super) fn move_entry(&mut self, from: usize, to: usize) {
        let n = self.entries.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        self.selected = self.selected.map(|s| follow(s, from, to));
    }

    /// What `key` (with Alt held: `alt`) does to the selected row: ↑ / ↓ select the row above
    /// or below (the newest when none is), Alt+↑ / Alt+↓ move the row up or down, a letter sets
    /// its action. Whether it reworded the row now, so its message can take the keyboard.
    pub(in crate::repo) fn press(&mut self, key: Key, alt: bool) -> bool {
        let n = self.entries.len();
        if n == 0 || self.starting {
            return false;
        }
        let top = n - 1;
        match (key, self.selected) {
            (Key::ArrowUp | Key::ArrowDown, None) => self.selected = Some(top),
            (Key::ArrowUp, Some(i)) if alt && i < top => {
                self.move_entry(i, i + 1);
            }
            (Key::ArrowDown, Some(i)) if alt && i > 0 => {
                self.move_entry(i, i - 1);
            }
            (Key::ArrowUp, Some(i)) => self.selected = Some((i + 1).min(top)),
            (Key::ArrowDown, Some(i)) => self.selected = Some(i.saturating_sub(1)),
            (_, Some(i)) if !alt => {
                if let (Some(step), Some(e)) = (Step::for_key(key), self.entries.get_mut(i)) {
                    let reworded = step == Step::Reword && e.step != Step::Reword;
                    e.step = step;
                    return reworded;
                }
            }
            _ => {}
        }
        false
    }

    /// Reads this frame's keys when nothing else has the keyboard (`free`): what they did for
    /// the list to follow.
    pub(super) fn keys(&mut self, ui: &Ui, free: bool) -> Keys {
        if !free || self.starting {
            return Keys::default();
        }
        use crate::keymap::{self, Scope};
        keymap::active(
            ui.ctx(),
            vec![Scope::Window, Scope::Repository, Scope::Rebase],
        );
        let pressed = keymap::pressed(ui.ctx(), Scope::Rebase);
        let mut keys = Keys {
            reveal: !pressed.is_empty(),
            ..Keys::default()
        };
        for command in pressed {
            use crate::keymap::Command;
            let (key, alt) = match command {
                Command::RebasePrevious => (Key::ArrowUp, false),
                Command::RebaseNext => (Key::ArrowDown, false),
                Command::RebaseMoveUp => (Key::ArrowUp, true),
                Command::RebaseMoveDown => (Key::ArrowDown, true),
                Command::RebasePick => (Key::P, false),
                Command::RebaseReword => (Key::R, false),
                Command::RebaseEdit => (Key::E, false),
                Command::RebaseSquash => (Key::S, false),
                Command::RebaseFixup => (Key::F, false),
                Command::RebaseDrop => (Key::D, false),
                _ => continue,
            };
            keys.reworded |= self.press(key, alt);
        }
        keys
    }

    /// The rows, newest on top; those out of sight only take their place.
    pub(super) fn list(&mut self, ui: &mut Ui, keys: Keys) {
        let n = self.entries.len();
        let mut swap: Option<(usize, usize)> = None;
        let mut rows: Vec<(usize, Rect)> = Vec::with_capacity(n);
        let t = Tokens::get(ui.ctx());
        // What is being started is what is shown.
        let starting = self.starting;
        let editor = self.id;
        let dragged = egui::DragAndDrop::payload::<DraggedRow>(ui.ctx())
            .filter(|d| d.editor == editor)
            .map(|d| d.index);
        egui::ScrollArea::vertical()
            .id_salt("rebase_list")
            .auto_shrink(false)
            .show(ui, |ui| {
                if starting {
                    ui.disable();
                }
                ui.add_space(6.0);
                let width = ui.available_width();
                // Newest first, like the graph; `i` indexes git's oldest-first order.
                for i in (0..n).rev() {
                    let top = ui.cursor().top();
                    let selected = self.selected == Some(i);
                    let hash = self.entries[i].commit.hash.clone();
                    let known = self.heights.get(&hash).copied();
                    let reveal = keys.reveal && selected;
                    let unseen = known.is_some_and(|h| {
                        !ui.is_rect_visible(Rect::from_min_size(
                            egui::pos2(ui.cursor().left(), top),
                            egui::vec2(width, h),
                        ))
                    });
                    if unseen && !reveal && dragged != Some(i) {
                        ui.allocate_space(egui::vec2(width, known.unwrap_or(ROW_H)));
                    } else {
                        let lifted = dragged == Some(i);
                        if self.row(ui, &t, i, keys.reworded, lifted, &mut swap) {
                            self.selected = Some(i);
                        }
                        ui.add_space(2.0);
                        ui.separator();
                        self.heights.insert(hash, ui.cursor().top() - top);
                    }
                    let rect =
                        Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
                    if reveal {
                        ui.scroll_to_rect(rect, None);
                    }
                    rows.push((i, rect));
                }
                if let Some(from) = dragged {
                    self.drop_on(ui, &t, from, &rows);
                }
            });
        if let Some((a, b)) = swap {
            self.move_entry(a, b);
        }
    }

    /// While row `from` is dragged over `rows`: the line where it would go, the list scrolled
    /// when the pointer nears its edge, and the row moved there when it is let go over the
    /// list. Let go anywhere else, it stays where it was.
    fn drop_on(&mut self, ui: &mut Ui, t: &Tokens, from: usize, rows: &[(usize, Rect)]) {
        let Some(pos) = ui.ctx().pointer_interact_pos() else {
            return;
        };
        if let Some(e) = self.entries.get(from) {
            ghost(ui, t, pos, &e.commit.hash, &e.commit.summary);
        }
        let clip = ui.clip_rect();
        let over =
            clip.x_range().contains(pos.x) && clip.expand2(egui::vec2(0.0, 40.0)).contains(pos);
        const EDGE: f32 = 28.0;
        if over && pos.y < clip.top() + EDGE {
            ui.scroll_with_delta(egui::vec2(0.0, 8.0));
            ui.ctx().request_repaint();
        } else if over && pos.y > clip.bottom() - EDGE {
            ui.scroll_with_delta(egui::vec2(0.0, -8.0));
            ui.ctx().request_repaint();
        }
        let to = over
            .then(|| row_at(rows, pos.y))
            .flatten()
            .and_then(|(target, above)| {
                let to = drop_index(from, target, above)?;
                let rect = rows.iter().find(|(i, _)| *i == target)?.1;
                Some((to, if above { rect.top() } else { rect.bottom() }, rect))
            });
        if let Some((_, y, rect)) = to {
            ui.painter()
                .hline(rect.x_range(), y, Stroke::new(2.0, t.accent));
        }
        if ui.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            if let Some((to, _, _)) = to {
                self.move_entry(from, to);
                self.selected = Some(to);
            }
        }
    }

    /// Row `i`: its handle, the buttons moving it, its action, the commit, and the new message
    /// when reworded. Whether it was clicked (or acted on), which selects it.
    fn row(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        i: usize,
        focus: bool,
        lifted: bool,
        swap: &mut Option<(usize, usize)>,
    ) -> bool {
        let n = self.entries.len();
        let selected = self.selected == Some(i);
        let editor = self.id;
        let Some(e) = self.entries.get_mut(i) else {
            return false;
        };
        let mut picked = false;
        let fill = if selected {
            t.row_selected
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.push_id(&e.commit.hash, |ui| {
            egui::Frame::new().fill(fill).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if lifted {
                    ui.set_opacity(0.4);
                }
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    let id = ui.id().with("handle");
                    let handle = ui.dnd_drag_source(id, DraggedRow { editor, index: i }, grip);
                    picked |= handle.response.drag_started();
                    ui.add_enabled_ui(i + 1 < n, |ui| {
                        if icons::small_button(
                            ui,
                            "chevron-up",
                            &crate::keymap::tip(
                                ui.ctx(),
                                crate::i18n::text("repo-core-3246"),
                                crate::keymap::Command::RebaseMoveUp,
                            ),
                        )
                        .clicked()
                        {
                            *swap = Some((i, i + 1));
                            picked = true;
                        }
                    });
                    ui.add_enabled_ui(i > 0, |ui| {
                        if icons::small_button(
                            ui,
                            "chevron-down",
                            &crate::keymap::tip(
                                ui.ctx(),
                                crate::i18n::text("repo-core-3247"),
                                crate::keymap::Command::RebaseMoveDown,
                            ),
                        )
                        .clicked()
                        {
                            *swap = Some((i, i - 1));
                            picked = true;
                        }
                    });
                    let before = e.step;
                    egui::ComboBox::from_id_salt("step")
                        .width(84.0)
                        .selected_text(e.step.label())
                        .show_ui(ui, |ui| {
                            for step in Step::ALL {
                                ui.selectable_value(&mut e.step, step, step.label())
                                    .on_hover_text(format!(
                                        "{} ({})",
                                        step.explain(),
                                        crate::keymap::label(ui.ctx(), step.command())
                                    ));
                            }
                        });
                    picked |= e.step != before;
                    super::weather::badge(ui, self.weather.rows.get(i), e.step == Step::Drop, t);
                    let dim = e.step == Step::Drop;
                    let hash = ui.add(
                        egui::Label::new(
                            egui::RichText::new(short(&e.commit.hash))
                                .font(theme::mono(11.5))
                                .color(t.text_faint),
                        )
                        .selectable(false)
                        .sense(Sense::click()),
                    );
                    let (rect, avatar) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
                    widgets::avatar(ui, rect.center(), 9.0, &e.commit.author, &e.commit.email);
                    let mut text = egui::RichText::new(&e.commit.summary).color(if dim {
                        t.text_faint
                    } else {
                        t.text
                    });
                    if dim {
                        text = text.strikethrough();
                    }
                    let summary = ui.add(
                        egui::Label::new(text)
                            .selectable(false)
                            .sense(Sense::click()),
                    );
                    let label = || {
                        crate::i18n::message(
                            "repo-core-rebase-commit-accessible",
                            &[
                                ("hash", short(&e.commit.hash).into()),
                                ("summary", e.commit.summary.as_str().into()),
                                ("author", e.commit.author.as_str().into()),
                                ("step", e.step.label().into()),
                            ],
                        )
                    };
                    for response in [&hash, &avatar, &summary] {
                        crate::a11y::a11y_selected(response, label, selected);
                    }
                    picked |= hash.clicked() || avatar.clicked() || summary.clicked();
                    if matches!(e.step, Step::Squash | Step::Fixup) {
                        ui.label(
                            egui::RichText::new(crate::i18n::text("repo-core-3250"))
                                .color(t.accent_fg)
                                .small(),
                        );
                    }
                    if e.step == Step::Edit {
                        ui.label(
                            egui::RichText::new(crate::i18n::text("repo-core-3251"))
                                .color(t.accent_fg)
                                .small(),
                        );
                    }
                    // The rest of the row selects it too.
                    let rest = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 20.0),
                        Sense::click(),
                    );
                    crate::a11y::a11y_selected(&rest.1, label, selected);
                    picked |= rest.1.clicked();
                });
                if e.step == Step::Reword {
                    ui.horizontal(|ui| {
                        ui.add_space(92.0);
                        let message = ui.add(
                            egui::TextEdit::multiline(&mut e.message)
                                .desired_rows(3)
                                .desired_width(ui.available_width() - 16.0),
                        );
                        if focus && selected {
                            message.request_focus();
                        }
                    });
                }
            });
        });
        picked
    }
}

/// What the keys pressed this frame did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Keys {
    /// The selected row was just reworded: its message takes the keyboard.
    pub reworded: bool,
    /// The selection moved (or its row did): the list scrolls to it.
    pub reveal: bool,
}

/// The height a row is taken to have before it was first drawn.
const ROW_H: f32 = 28.0;

/// The commit being dragged, beside the pointer.
fn ghost(ui: &Ui, t: &Tokens, pos: egui::Pos2, hash: &str, summary: &str) {
    egui::Area::new(egui::Id::new("rebase_row_ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 6.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(short(hash))
                            .font(theme::mono(11.5))
                            .color(t.text_faint),
                    );
                    ui.label(egui::RichText::new(summary).color(t.text));
                });
            });
        });
}

impl Step {
    /// The default Git todo abbreviation, checked against the registry in tests.
    #[cfg(test)]
    fn key(self) -> char {
        match self {
            Step::Pick => 'P',
            Step::Reword => 'R',
            Step::Edit => 'E',
            Step::Squash => 'S',
            Step::Fixup => 'F',
            Step::Drop => 'D',
        }
    }
}

/// The handle a row is dragged by: two columns of dots.
fn grip(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 20.0), Sense::hover());
    let color = if resp.hovered() { t.text } else { t.text_faint };
    for col in [-2.5, 2.5] {
        for row in [-5.0, 0.0, 5.0] {
            ui.painter()
                .circle_filled(rect.center() + egui::vec2(col, row), 1.4, color);
        }
    }
    resp.on_hover_text(crate::i18n::text("repo-core-4240"));
}

#[cfg(test)]
mod tests;
