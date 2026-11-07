//! Selecting text in a diff by dragging over it, on one side, and copying it.

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

/// Text selected in a diff by dragging, on one side of it: from where the drag started to
/// where the pointer is, as (line, character) positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) struct TextSel {
    pub(super) side: Side,
    pub(super) anchor: (usize, usize),
    pub(super) cursor: (usize, usize),
}

impl TextSel {
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

/// The selection after this frame's drag over the lines: started where the press hit the text,
/// its end moved to the pointer on the side it started on, or gone after a click.
/// `pointer_hits`: each with whether the pointer is over that line's half.
pub(super) fn drag_selection(
    sel: Option<TextSel>,
    drag: &egui::Response,
    press_hit: Option<Hit>,
    pointer_hits: &[(Hit, bool)],
) -> Option<TextSel> {
    let mut sel = sel;
    if drag.drag_started() {
        sel = press_hit.map(|(i, side, c)| TextSel {
            side,
            anchor: (i, c),
            cursor: (i, c),
        });
    } else if drag.dragged()
        && let Some(s) = &mut sel
        && let Some(((i, _, c), _)) = pointer_hits
            .iter()
            .find(|((_, side, _), _)| *side == s.side)
    {
        s.cursor = (*i, *c);
    } else if drag.clicked() {
        sel = None;
    }
    sel
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
    let copy = sel.is_some()
        && !crate::a11y::keyboard_taken(ui.ctx())
        && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
    if copy && let Some(text) = selected_text() {
        ui.ctx().copy_text(text);
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
    egui::Popup::context_menu(resp)
        .style(crate::theme::menu_style)
        .show(|ui| {
            let selected_text = selected_text();
            if ui
                .add_enabled(selected_text.is_some(), egui::Button::new("Copy"))
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
                        && ui.button("Explain history").clicked()
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
                        && ui.button("Symbol biography").clicked()
                    {
                        acts.push(crate::repo::Action::OpenSymbolBiography(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: commit.clone(),
                                path: path.clone(),
                                line: range.0 as usize,
                            },
                        )));
                    }

                    let b = ui.button("History of these lines").on_hover_text(
                        "Every commit up to this version that changed the selected lines, \
                         followed back through earlier edits and renames",
                    );
                    if b.clicked() {
                        acts.push(action);
                    }
                }
                Err(why) => {
                    ui.add_enabled(false, egui::Button::new("History of these lines"))
                        .on_disabled_hover_text(why);
                }
            }
        });
}
