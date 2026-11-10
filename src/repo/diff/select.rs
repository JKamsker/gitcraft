//! Caret, word, line and drag selection on one side of a read-only diff.

use std::collections::HashMap;
use std::ops::Range;

use egui::Ui;

use super::{DiffLine, Kind};
use crate::repo::highlight::MAX_LINE_CHARS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Unified,
    Old,
    New,
    /// A hunk header across both halves of the split view.
    Both,
}

impl Side {
    /// Whether a line of `kind` shows on this side.
    pub(super) fn shows(self, kind: Kind) -> bool {
        match self {
            Side::Unified => matches!(kind, Kind::Context | Kind::Add | Kind::Del),
            Side::Old => matches!(kind, Kind::Context | Kind::Del),
            Side::New => matches!(kind, Kind::Context | Kind::Add),
            Side::Both => false,
        }
    }
}

/// A place in the text of a line where it shows: (line, side, character).
pub(super) type Hit = (usize, Side, usize);

#[derive(Clone, Copy)]
struct ClickSequence {
    line: usize,
    side: Side,
    pos: egui::Pos2,
    time: f64,
    count: u8,
}

impl ClickSequence {
    fn register(ui: &Ui, response: &egui::Response, hit: Hit) -> u8 {
        let Some(pos) = response.interact_pointer_pos() else {
            return 1;
        };
        let id = response.id.with("text-click-sequence");
        let previous: Option<Self> = ui.data(|d| d.get_temp(id));
        let time = ui.input(|i| i.time);
        let options = ui.ctx().options(|o| o.input_options);
        // Count clicks within this code target, rather than egui's global triple-click
        // sequence, which can include a click on a different row or in another control.
        let count = match previous {
            Some(p)
                if p.line == hit.0
                    && p.side == hit.1
                    && time - p.time < options.max_double_click_delay
                    && pos.distance_sq(p.pos) < options.max_click_dist * options.max_click_dist =>
            {
                (p.count + 1).min(3)
            }
            _ => 1,
        };
        ui.data_mut(|d| {
            d.insert_temp(
                id,
                Self {
                    line: hit.0,
                    side: hit.1,
                    pos,
                    time,
                    count,
                },
            )
        });
        count
    }
}

/// Text selected in a diff by dragging, on one side of it: from where the drag started to
/// where the pointer is, as (line, character) positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) struct TextSel {
    pub(super) side: Side,
    pub(super) anchor: (usize, usize),
    pub(super) cursor: (usize, usize),
}

impl TextSel {
    pub(super) fn caret((i, side, c): Hit) -> Self {
        Self {
            side,
            anchor: (i, c),
            cursor: (i, c),
        }
    }

    pub(super) fn is_empty(self) -> bool {
        self.anchor == self.cursor
    }

    fn clicked(lines: &[DiffLine], hit: Hit, word: bool, line: bool) -> Self {
        let mut sel = Self::caret(hit);
        if line {
            sel.anchor.1 = 0;
            sel.cursor.1 = usize::MAX;
        } else if word {
            // Use the same Unicode word boundaries as egui's text editors.
            use egui::text::CCursor;
            use egui::text_selection::text_cursor_state::{
                ccursor_next_word, ccursor_previous_word,
            };
            let text = &lines[hit.0].text;
            let cursor = CCursor::new(hit.2);
            sel.anchor.1 = ccursor_previous_word(text, cursor).index.0;
            sel.cursor.1 = ccursor_next_word(text, cursor).index.0;
        }
        sel
    }

    pub(super) fn all(lines: &[DiffLine], side: Side) -> Option<Self> {
        let mut shown = lines.iter().enumerate().filter(|(_, l)| side.shows(l.kind));
        let (first, _) = shown.next()?;
        let last = shown.next_back().map_or(first, |(i, _)| i);
        Some(Self {
            side,
            anchor: (first, 0),
            cursor: (last, usize::MAX),
        })
    }

    /// Whether it was made in the side-by-side view (`split`) or the unified one.
    pub(super) fn made_in(&self, split: bool) -> bool {
        (self.side == Side::Unified) != split
    }

    /// The first and last rows it is on.
    pub(super) fn rows(&self) -> (usize, usize) {
        let (start, end) = self.ordered();
        (start.0, end.0)
    }

    fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    /// The characters of line `i` selected where it shows on `side` (`usize::MAX`: through
    /// the end of the line, line break included).
    pub(super) fn chars_of(&self, i: usize, side: Side) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if side != self.side || i < start.0 || i > end.0 {
            return None;
        }
        let from = if i == start.0 { start.1 } else { 0 };
        let to = if i == end.0 { end.1 } else { usize::MAX };
        (from < to).then_some(from..to)
    }

    /// The selected text, one line of the side per line; a line cut to be shown is copied
    /// whole from `uncut` (by line).
    pub(super) fn text(&self, lines: &[DiffLine], uncut: &HashMap<usize, String>) -> String {
        let (start, end) = self.ordered();
        let mut out: Vec<String> = Vec::new();
        for i in start.0..=end.0 {
            let Some(line) = lines.get(i).filter(|l| self.side.shows(l.kind)) else {
                continue;
            };
            let mut from = if i == start.0 { start.1 } else { 0 };
            let mut to = if i == end.0 { end.1 } else { usize::MAX };
            let text = match uncut.get(&i) {
                // A line cut to be shown: the places are in what is shown, the cut part and
                // then the note of what was left out, which stands for the rest of the line.
                Some(whole) => {
                    from = from.min(MAX_LINE_CHARS);
                    if to > MAX_LINE_CHARS {
                        to = usize::MAX;
                    }
                    whole
                }
                None => &line.text,
            };
            out.push(
                text.chars()
                    .skip(from)
                    .take(to.saturating_sub(from))
                    .collect(),
            );
        }
        out.join("\n")
    }
}

/// A press replaces the previous selection immediately. Dragging extends that caret on its
/// original side, even after scrolling its anchor out of the visible rows.
/// `pointer_hits`: each with whether the pointer is over that line's half.
pub(super) fn drag_selection(
    ui: &Ui,
    sel: Option<TextSel>,
    drag: &egui::Response,
    press_hit: Option<Hit>,
    pointer_hits: &[(Hit, bool)],
    lines: &[DiffLine],
) -> Option<TextSel> {
    let mut sel = sel;
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let code_press = press_hit.is_some() && drag.is_pointer_button_down_on();
    if ui.input(|i| {
        i.pointer.secondary_pressed()
            || i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
    }) || drag.drag_started_by(egui::PointerButton::Primary)
        || primary_pressed && !code_press && !drag.clicked()
    {
        ui.data_mut(|d| d.remove::<ClickSequence>(drag.id.with("text-click-sequence")));
    }
    if primary_pressed && !egui::Popup::is_any_open(ui.ctx()) {
        sel = press_hit
            .filter(|_| drag.is_pointer_button_down_on())
            .map(TextSel::caret);
    }
    if drag.clicked() {
        sel = pointer_hits
            .iter()
            .find(|(_, over)| *over)
            .map(|&(hit, _)| {
                let count = ClickSequence::register(ui, drag, hit);
                TextSel::clicked(lines, hit, count == 2, count == 3)
            });
    } else if (drag.dragged_by(egui::PointerButton::Primary)
        || drag.drag_stopped_by(egui::PointerButton::Primary))
        && let Some(s) = &mut sel
        && let Some(((i, _, c), _)) = pointer_hits
            .iter()
            .find(|((_, side, _), _)| *side == s.side)
    {
        s.cursor = (*i, *c);
    }
    sel
}

/// egui suppresses wheel scrolling while another widget owns a drag. Forward the wheel
/// inside this ScrollArea so text can still be selected beyond its initial viewport.
pub(super) fn scroll_selection(ui: &Ui, drag: &egui::Response, wrap: bool) {
    if !drag.dragged_by(egui::PointerButton::Primary)
        || !ui.input(|i| {
            i.pointer
                .interact_pos()
                .is_some_and(|p| ui.clip_rect().contains(p))
        })
    {
        return;
    }
    let delta = ui.input_mut(|i| {
        let delta = egui::vec2(
            if wrap { 0.0 } else { i.smooth_scroll_delta.x },
            i.smooth_scroll_delta.y,
        );
        i.smooth_scroll_delta.y = 0.0;
        if !wrap {
            i.smooth_scroll_delta.x = 0.0;
        }
        delta
    });
    if delta != egui::Vec2::ZERO {
        ui.scroll_with_delta_animation(delta, egui::style::ScrollAnimation::none());
    }
}

/// Ctrl+C and the "Copy" of the context menu on the lines (`drag`, and the changed lines that
/// can be clicked, `picks`, which take the right-click over them) copy the selected text. The
/// menu also has "History of these lines": `history` makes its action, or says why there is
/// none.
#[allow(clippy::too_many_arguments)]
pub(super) fn copy_selection(
    ui: &Ui,
    drag: &egui::Response,
    picks: &[egui::Response],
    sel: Option<TextSel>,
    lines: &[DiffLine],
    uncut: &HashMap<usize, String>,
    mut more: impl FnMut(&mut Ui, &mut Vec<crate::repo::Action>),
    history: &dyn Fn() -> Result<crate::repo::Action, &'static str>,
    acts: &mut Vec<crate::repo::Action>,
) {
    // Put together only when asked for: a selection can span thousands of lines.
    let selected_text = || sel.map(|s| s.text(lines, uncut)).filter(|s| !s.is_empty());
    if crate::keymap::copy_requested(ui.ctx())
        && let Some(text) = selected_text()
    {
        ui.ctx().copy_text(text);
    }
    if !crate::a11y::keyboard_taken(ui.ctx())
        && crate::keymap::take(ui.ctx(), crate::keymap::Command::SelectionHistory)
        && let Ok(action) = history()
    {
        acts.push(action);
    }
    for resp in std::iter::once(drag).chain(picks) {
        lines_menu(resp, &selected_text, history, acts, &mut more);
    }
}

/// The right-click menu of the lines: copy what is selected, or read the lines' history.
fn lines_menu(
    resp: &egui::Response,
    selected_text: &dyn Fn() -> Option<String>,
    history: &dyn Fn() -> Result<crate::repo::Action, &'static str>,
    acts: &mut Vec<crate::repo::Action>,
    more: &mut dyn FnMut(&mut Ui, &mut Vec<crate::repo::Action>),
) {
    crate::menu_item::context(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            let selected_text = selected_text();
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-1133"))
                .icon("copy")
                .keys(crate::keymap::Command::SelectionCopy)
                .refused(
                    selected_text
                        .is_none()
                        .then_some(crate::i18n::text("repo-core-2324")),
                )
                .show(ui)
                .clicked()
                && let Some(text) = &selected_text
            {
                ui.ctx().copy_text(text.clone());
            }
            more(ui, acts);
            match history() {
                Ok(action) => {
                    if let crate::repo::Action::LineHistory {
                        path,
                        at: crate::git::LinesAt::Rev(commit),
                        range,
                    } = &action
                        && crate::git::investigations::oid(commit)
                        && ui.button(crate::i18n::text("repo-core-2325")).clicked()
                    {
                        acts.push(crate::repo::Action::OpenLineDossier(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: commit.clone(),
                                path: path.clone(),
                                line: range.0 as usize,
                            },
                        )));
                    }
                    if let crate::repo::Action::LineHistory {
                        path,
                        at: crate::git::LinesAt::Rev(commit),
                        range,
                    } = &action
                        && crate::git::investigations::oid(commit)
                        && ui.button(crate::i18n::text("repo-core-2326")).clicked()
                    {
                        acts.push(crate::repo::Action::OpenSymbolBiography(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: commit.clone(),
                                path: path.clone(),
                                line: range.0 as usize,
                            },
                        )));
                    }

                    let b = crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2327"))
                        .icon("history")
                        .keys(crate::keymap::Command::SelectionHistory)
                        .show(ui)
                        .on_hover_text(crate::i18n::text("repo-core-2328"));
                    if b.clicked() {
                        acts.push(action);
                    }
                }
                Err(why) => {
                    crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2327"))
                        .icon("history")
                        .keys(crate::keymap::Command::SelectionHistory)
                        .refused(Some(why))
                        .show(ui)
                        .on_disabled_hover_text(why);
                }
            }
        });
}

#[cfg(test)]
mod registry_menu_tests {
    use super::*;
    use crate::keymap::{Command, Stroke};
    use egui::{Key, Modifiers};
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn actual_selection_menu_shows_configured_keys_and_refusal() {
        let mut h = Harness::new_ui_state(
            |ui, installed: &mut bool| {
                if !std::mem::replace(installed, true) {
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings::default();
                    for (cmd, key) in [
                        (Command::SelectionCopy, Key::F8),
                        (Command::SelectionHistory, Key::F9),
                    ] {
                        settings
                            .key_bindings
                            .insert(cmd.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
                    }
                    settings.put_for_test(ui.ctx());
                }
                let response = ui.button("Diff lines");
                response.request_focus();
                lines_menu(
                    &response,
                    &|| Some("selected".into()),
                    &|| Err("No committed lines here"),
                    &mut Vec::new(),
                    &mut |_, _| {},
                );
            },
            false,
        );
        h.run();
        h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
        h.run();
        h.get_by_label("Copy F8");
        assert!(
            h.get_by_label("History of these lines F9")
                .accesskit_node()
                .is_disabled()
        );
        h.get_by_label("No committed lines here");
        fn texts(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::Shape::Vec(v) => {
                    for s in v {
                        texts(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for s in &h.output().shapes {
            texts(&s.shape, &mut labels);
        }
        assert!(
            labels.iter().any(|s| s == "F8"),
            "actual Copy menu key: {labels:?}"
        );
        assert!(
            labels.iter().any(|s| s == "F9"),
            "actual History menu key: {labels:?}"
        );
    }
}
