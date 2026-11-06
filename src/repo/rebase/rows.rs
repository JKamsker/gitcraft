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

/// The keys the editor reads, in the order they are checked.
const KEYS: [Key; 8] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::P,
    Key::R,
    Key::E,
    Key::S,
    Key::F,
    Key::D,
];

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
            (Key::ArrowUp, Some(i)) if alt => {
                if i < top {
                    self.move_entry(i, i + 1);
                }
            }
            (Key::ArrowDown, Some(i)) if alt => {
                if i > 0 {
                    self.move_entry(i, i - 1);
                }
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

    /// Reads this frame's keys when nothing else has the keyboard (`free`); whether one of them
    /// reworded the selected row.
    pub(super) fn keys(&mut self, ui: &Ui, free: bool) -> bool {
        if !free || self.starting {
            return false;
        }
        // In the order they came, each with whether Alt was held; with Ctrl, Cmd or Shift
        // they are someone else's shortcuts.
        let pressed: Vec<(Key, bool)> = ui.input_mut(|i| {
            let mut pressed = Vec::new();
            i.events.retain(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if KEYS.contains(key)
                    && !(modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                    && !modifiers.shift =>
                {
                    pressed.push((*key, modifiers.alt));
                    false
                }
                _ => true,
            });
            pressed
        });
        let mut reworded = false;
        for (key, alt) in pressed {
            reworded |= self.press(key, alt);
        }
        reworded
    }

    /// The rows, newest on top. `focus`: the selected row was just reworded, its message takes
    /// the keyboard.
    pub(super) fn list(&mut self, ui: &mut Ui, focus: bool) {
        let n = self.entries.len();
        let mut swap: Option<(usize, usize)> = None;
        let mut rows: Vec<(usize, Rect)> = Vec::with_capacity(n);
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
                // Newest first, like the graph; `i` indexes git's oldest-first order.
                for i in (0..n).rev() {
                    let top = ui.cursor().top();
                    let picked = self.row(ui, i, focus, dragged == Some(i), &mut swap);
                    if picked {
                        self.selected = Some(i);
                    }
                    ui.add_space(2.0);
                    let rect =
                        Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
                    ui.separator();
                    rows.push((i, rect));
                }
                if let Some(from) = dragged {
                    self.drop_on(ui, from, &rows);
                }
            });
        if let Some((a, b)) = swap {
            self.move_entry(a, b);
        }
    }

    /// While row `from` is dragged over `rows`: the line where it would go, the list scrolled
    /// when the pointer nears its edge, and the row moved there when it is let go.
    fn drop_on(&mut self, ui: &mut Ui, from: usize, rows: &[(usize, Rect)]) {
        let Some(pos) = ui.ctx().pointer_interact_pos() else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let clip = ui.clip_rect();
        const EDGE: f32 = 28.0;
        if pos.y < clip.top() + EDGE {
            ui.scroll_with_delta(egui::vec2(0.0, 8.0));
            ui.ctx().request_repaint();
        } else if pos.y > clip.bottom() - EDGE {
            ui.scroll_with_delta(egui::vec2(0.0, -8.0));
            ui.ctx().request_repaint();
        }
        let Some((target, above)) = row_at(rows, pos.y) else {
            return;
        };
        let to = drop_index(from, target, above);
        if let (Some(_), Some((_, rect))) = (to, rows.iter().find(|(i, _)| *i == target)) {
            let y = if above { rect.top() } else { rect.bottom() };
            ui.painter()
                .hline(rect.x_range(), y, Stroke::new(2.0, t.accent));
        }
        if ui.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            if let Some(to) = to {
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
        i: usize,
        focus: bool,
        lifted: bool,
        swap: &mut Option<(usize, usize)>,
    ) -> bool {
        let t = Tokens::get(ui.ctx());
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
                if lifted {
                    ui.set_opacity(0.4);
                }
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    let id = ui.id().with("handle");
                    let handle = ui.dnd_drag_source(id, DraggedRow { editor, index: i }, grip);
                    picked |= handle.response.drag_started();
                    ui.add_enabled_ui(i + 1 < n, |ui| {
                        if icons::small_button(ui, "chevron-up", "Move up, later (Alt+↑)").clicked()
                        {
                            *swap = Some((i, i + 1));
                            picked = true;
                        }
                    });
                    ui.add_enabled_ui(i > 0, |ui| {
                        if icons::small_button(ui, "chevron-down", "Move down, earlier (Alt+↓)")
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
                                    .on_hover_text(format!("{} ({})", step.explain(), step.key()));
                            }
                        });
                    picked |= e.step != before;
                    let dim = e.step == Step::Drop;
                    let hash = ui.add(
                        egui::Label::new(
                            egui::RichText::new(short(&e.commit.hash))
                                .font(theme::mono(11.5))
                                .color(t.text_faint),
                        )
                        .sense(Sense::click()),
                    );
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::hover());
                    widgets::avatar(ui, rect.center(), 9.0, &e.commit.author, &e.commit.email);
                    let mut text = egui::RichText::new(&e.commit.summary).color(if dim {
                        t.text_faint
                    } else {
                        t.text
                    });
                    if dim {
                        text = text.strikethrough();
                    }
                    let summary = ui.add(egui::Label::new(text).sense(Sense::click()));
                    picked |= hash.clicked() || summary.clicked();
                    if matches!(e.step, Step::Squash | Step::Fixup) {
                        ui.label(
                            egui::RichText::new("↓ into the commit below")
                                .color(t.accent)
                                .small(),
                        );
                    }
                    if e.step == Step::Edit {
                        ui.label(
                            egui::RichText::new("stops here to be changed")
                                .color(t.accent)
                                .small(),
                        );
                    }
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

impl Step {
    /// The key that sets it.
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
    resp.on_hover_text("Drag to move the commit");
}

#[cfg(test)]
mod tests {
    use super::super::Entry;
    use super::*;
    use crate::git::RebaseCommit;
    use egui::Modifiers;

    fn editor(hashes: &[&str]) -> RebaseEditor {
        let commits = hashes
            .iter()
            .map(|h| RebaseCommit {
                hash: (*h).to_owned(),
                summary: format!("summary {h}"),
                message: format!("summary {h}"),
                ..RebaseCommit::default()
            })
            .collect();
        RebaseEditor::new("x".into(), "x".into(), commits)
    }

    fn order(ed: &RebaseEditor) -> Vec<&str> {
        ed.entries.iter().map(|e| e.commit.hash.as_str()).collect()
    }

    fn steps(ed: &RebaseEditor) -> Vec<Step> {
        ed.entries.iter().map(|e: &Entry| e.step).collect()
    }

    #[test]
    fn letters_set_the_selected_rows_action() {
        let mut ed = editor(&["a", "b", "c"]);
        // Nothing selected: letters do nothing.
        assert!(!ed.press(Key::D, false));
        assert_eq!(steps(&ed), [Step::Pick; 3]);
        ed.selected = Some(1);
        for (key, step) in [
            (Key::D, Step::Drop),
            (Key::S, Step::Squash),
            (Key::F, Step::Fixup),
            (Key::E, Step::Edit),
            (Key::P, Step::Pick),
        ] {
            assert!(!ed.press(key, false));
            assert_eq!(ed.entries[1].step, step);
        }
        // Rewording says so once, for the message to take the keyboard.
        assert!(ed.press(Key::R, false));
        assert!(!ed.press(Key::R, false));
        assert_eq!(ed.entries[1].step, Step::Reword);
        // Other keys, or letters with Alt, change nothing.
        assert!(!ed.press(Key::X, false));
        assert!(!ed.press(Key::D, true));
        assert_eq!(steps(&ed), [Step::Pick, Step::Reword, Step::Pick]);
    }

    #[test]
    fn arrows_select_and_alt_arrows_move() {
        let mut ed = editor(&["a", "b", "c"]);
        // The newest (top) row first.
        ed.press(Key::ArrowDown, false);
        assert_eq!(ed.selected, Some(2));
        ed.press(Key::ArrowDown, false);
        ed.press(Key::ArrowDown, false);
        ed.press(Key::ArrowDown, false);
        assert_eq!(ed.selected, Some(0));
        ed.press(Key::ArrowUp, false);
        assert_eq!(ed.selected, Some(1));
        // Alt+↑ moves b up (later), the selection with it; at the top it stays.
        ed.press(Key::ArrowUp, true);
        assert_eq!(order(&ed), ["a", "c", "b"]);
        assert_eq!(ed.selected, Some(2));
        ed.press(Key::ArrowUp, true);
        assert_eq!(order(&ed), ["a", "c", "b"]);
        ed.press(Key::ArrowDown, true);
        ed.press(Key::ArrowDown, true);
        ed.press(Key::ArrowDown, true);
        assert_eq!(order(&ed), ["b", "a", "c"]);
        assert_eq!(ed.selected, Some(0));
    }

    #[test]
    fn nothing_changes_while_the_rebase_is_being_started() {
        let mut ed = editor(&["a", "b"]);
        ed.selected = Some(0);
        ed.starting = true;
        ed.press(Key::D, false);
        ed.press(Key::ArrowUp, true);
        assert_eq!(order(&ed), ["a", "b"]);
        assert_eq!(steps(&ed), [Step::Pick; 2]);
        assert!(!editor(&[]).press(Key::ArrowUp, false));
    }

    #[test]
    fn dropping_on_a_row_puts_it_above_or_below() {
        // Oldest first: a b c d, shown d c b a.
        assert_eq!(drop_index(0, 3, true), Some(3)); // a above d: newest
        assert_eq!(drop_index(0, 3, false), Some(2)); // a below d
        assert_eq!(drop_index(3, 0, false), Some(0)); // d below a: oldest
        assert_eq!(drop_index(3, 0, true), Some(1)); // d above a
        assert_eq!(drop_index(1, 2, false), None); // b below c: where it is
        assert_eq!(drop_index(2, 1, true), None); // c above b: where it is
        assert_eq!(drop_index(1, 1, true), None);
        assert_eq!(drop_index(1, 1, false), None);
        assert_eq!(drop_index(1, 2, true), Some(2));
    }

    #[test]
    fn a_dragged_row_lands_where_dropped_and_stays_selected() {
        let mut ed = editor(&["a", "b", "c", "d"]);
        ed.selected = Some(2);
        let to = drop_index(0, 3, true).unwrap_or_default();
        ed.move_entry(0, to);
        assert_eq!(order(&ed), ["b", "c", "d", "a"]);
        // c moved down with the others.
        assert_eq!(ed.selected, Some(1));
        ed.selected = Some(3);
        ed.move_entry(3, 0);
        assert_eq!(order(&ed), ["a", "b", "c", "d"]);
        assert_eq!(ed.selected, Some(0));
        // Out of range: nothing.
        ed.move_entry(0, 9);
        assert_eq!(order(&ed), ["a", "b", "c", "d"]);
    }

    #[test]
    fn the_row_under_the_pointer() {
        let rect =
            |top: f32| Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(10.0, top + 20.0));
        // Drawn newest first: entry 2 at the top.
        let rows = [(2, rect(0.0)), (1, rect(20.0)), (0, rect(40.0))];
        assert_eq!(row_at(&rows, 5.0), Some((2, true)));
        assert_eq!(row_at(&rows, 15.0), Some((2, false)));
        assert_eq!(row_at(&rows, 31.0), Some((1, false)));
        assert_eq!(row_at(&rows, -50.0), Some((2, true)));
        assert_eq!(row_at(&rows, 500.0), Some((0, false)));
        assert_eq!(row_at(&[], 5.0), None);
    }

    /// Draws the list one frame with `events`, as the editor does.
    fn frame(ctx: &egui::Context, ed: &mut RebaseEditor, events: Vec<egui::Event>) {
        crate::theme::install_fonts(ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let focus = ed.keys(ui, true);
                ed.list(ui, focus);
            });
        });
        out.textures_delta.clear();
    }

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn keys_pressed_over_the_list_act_on_it() {
        let ctx = egui::Context::default();
        let mut ed = editor(&["a", "b", "c"]);
        frame(&ctx, &mut ed, vec![]);
        frame(
            &ctx,
            &mut ed,
            vec![
                key(Key::ArrowDown, Modifiers::NONE),
                key(Key::F, Modifiers::NONE),
                key(Key::ArrowDown, Modifiers::ALT),
            ],
        );
        assert_eq!(order(&ed), ["a", "c", "b"]);
        assert_eq!(ed.entries[1].step, Step::Fixup);
        // With Ctrl held a letter is someone else's shortcut.
        frame(&ctx, &mut ed, vec![key(Key::D, Modifiers::COMMAND)]);
        assert_eq!(ed.entries[1].step, Step::Fixup);
    }

    #[test]
    fn letting_go_of_a_dragged_row_above_the_list_makes_it_the_newest() {
        let ctx = egui::Context::default();
        let mut ed = editor(&["a", "b", "c"]);
        frame(&ctx, &mut ed, vec![]);
        egui::DragAndDrop::set_payload(
            &ctx,
            DraggedRow {
                editor: ed.id,
                index: 0,
            },
        );
        let at = egui::pos2(300.0, 1.0);
        frame(
            &ctx,
            &mut ed,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert_eq!(order(&ed), ["b", "c", "a"]);
        assert_eq!(ed.selected, Some(2));
        // A row of another editor is not taken.
        egui::DragAndDrop::set_payload(
            &ctx,
            DraggedRow {
                editor: ed.id + 1,
                index: 0,
            },
        );
        frame(
            &ctx,
            &mut ed,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert_eq!(order(&ed), ["b", "c", "a"]);
    }

    #[test]
    fn every_action_has_its_key() {
        for step in Step::ALL {
            let key = Key::from_name(&step.key().to_string());
            assert_eq!(key.and_then(Step::for_key), Some(step));
        }
    }
}
