//! The command palette (Ctrl+P): app commands, themes, the open repository's actions and what
//! can be done with its selection, ranked by how well they match what is typed. A verb that
//! needs a branch (merge, rebase, check out…) lists the branches in a second step.

pub(super) mod items;
mod rank;
pub(super) mod recent;
mod row;
mod score;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use egui::{Key, RichText, Stroke, vec2};

use super::{App, Tab};
use crate::repo::{PaletteGeneration, Pick};
use crate::theme::{self, Tokens};
use items::{Entry, Item};
use rank::{Shown, rank};
use row::{ROW_H, Row, palette_row};

/// How tall the list gets before it scrolls.
const LIST_H: f32 = 380.0;

/// The command palette (Ctrl+P): type to filter, Enter runs.
pub(super) struct Palette {
    query: String,
    selected: usize,
    /// The first step's entries, made once rather than every frame, and what they were made
    /// from.
    entries: Option<(EntriesKey, Vec<Item>)>,
    /// The verb chosen, whose branch is being picked.
    step: Option<Step>,
    /// The entries listed for the query, and what they were ranked for.
    shown: Option<(RankedFor, Vec<Shown>)>,
    /// How far the list is scrolled.
    scroll: f32,
}

/// The second step: what to do it with.
struct Step {
    /// The verb's label, shown above the field.
    title: String,
    pick: Pick,
    choices: Vec<Item>,
    /// What was typed in the first step, back when going back.
    query: String,
}

/// What the palette's entries were made from: the active tab, and its repository (a tab's
/// view can change to another worktree) and what that repository's entries are made from.
type EntriesKey = (usize, Option<(PathBuf, PaletteGeneration)>);

/// What the list shown was ranked for: the query, the step, and the entries' key.
type RankedFor = (String, bool, EntriesKey);

/// What a frame of the palette asked for.
enum Outcome {
    Stay,
    Close,
    Back,
    /// Run (or step into) the entry at this index of the list in use.
    Choose(usize),
}

impl App {
    pub(super) fn open_palette(&mut self) {
        self.palette = Some(Palette {
            query: String::new(),
            selected: 0,
            entries: None,
            step: None,
            shown: None,
            scroll: 0.0,
        });
    }

    fn entries_key(&self) -> EntriesKey {
        (
            self.active,
            match self.tabs.get(self.active) {
                Some(Tab::Repo(repo)) => Some((repo.path.clone(), repo.palette_generation())),
                _ => None,
            },
        )
    }

    /// Makes the entries again when what they are made from changed, and the second step's
    /// choices with them (a branch deleted meanwhile is gone from them).
    fn refresh_palette(&mut self, key: &EntriesKey) {
        let Some(palette) = &self.palette else {
            return;
        };
        if palette
            .entries
            .as_ref()
            .is_some_and(|(made, _)| made == key)
        {
            return;
        }
        let entries = self.palette_entries();
        let choices = palette.step.as_ref().map(|s| self.palette_choices(&s.pick));
        if let Some(palette) = &mut self.palette {
            palette.entries = Some((key.clone(), entries));
            if let (Some(step), Some(choices)) = (&mut palette.step, choices) {
                step.choices = choices;
            }
        }
    }

    pub(super) fn palette_ui(&mut self, ctx: &egui::Context) {
        if self.palette.is_none() {
            return;
        }
        let key = self.entries_key();
        self.refresh_palette(&key);
        let outcome = match &mut self.palette {
            Some(palette) => palette.show(ctx, &key, &self.palette_recent),
            None => return,
        };
        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.palette = None,
            Outcome::Back => {
                if let Some(p) = &mut self.palette
                    && let Some(step) = p.step.take()
                {
                    p.query = step.query;
                    p.go_to_top();
                }
            }
            Outcome::Choose(i) => self.choose(i, ctx),
        }
    }

    /// Runs the `i`th entry of the list in use, or opens the second step for a verb.
    fn choose(&mut self, i: usize, ctx: &egui::Context) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        let list = match &mut palette.step {
            Some(step) => &mut step.choices,
            None => match &mut palette.entries {
                Some((_, entries)) => entries,
                None => return,
            },
        };
        let Some(item) = list.get(i) else {
            return;
        };
        self.palette_recent.used(&item.key);
        if let Entry::Pick(pick) = &item.run {
            let (title, pick) = (item.label.clone(), pick.clone());
            let choices = self.palette_choices(&pick);
            if let Some(p) = &mut self.palette {
                p.step = Some(Step {
                    title,
                    pick,
                    choices,
                    query: std::mem::take(&mut p.query),
                });
                p.go_to_top();
            }
            return;
        }
        let mut list = match self.palette.take() {
            Some(Palette {
                step: Some(step), ..
            }) => step.choices,
            Some(Palette {
                entries: Some((_, entries)),
                ..
            }) => entries,
            _ => return,
        };
        if i < list.len() {
            let item = list.swap_remove(i);
            self.run_entry(item.run, ctx);
        }
    }
}

impl Palette {
    fn go_to_top(&mut self) {
        self.selected = 0;
        self.scroll = 0.0;
        self.shown = None;
    }

    /// The entries of the step in use.
    fn list(&self) -> &[Item] {
        match &self.step {
            Some(step) => &step.choices,
            None => self.entries.as_ref().map_or(&[], |(_, e)| e),
        }
    }

    /// Ranks the list for the query when either changed.
    fn ranked(&mut self, key: &EntriesKey, recent: &recent::Recent) {
        let wanted = (self.query.clone(), self.step.is_some(), key.clone());
        if self.shown.as_ref().is_some_and(|(made, _)| *made == wanted) {
            return;
        }
        let shown = rank(self.list(), &self.query, recent);
        self.shown = Some((wanted, shown));
    }

    /// Draws the palette and handles its keys.
    fn show(&mut self, ctx: &egui::Context, key: &EntriesKey, recent: &recent::Recent) -> Outcome {
        let t = Tokens::get(ctx);
        let (up, down, enter, esc, backspace) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Backspace),
            )
        });
        if self.step.is_some() && (esc || (backspace && self.query.is_empty())) {
            return Outcome::Back;
        }
        self.ranked(key, recent);
        let count = self.shown.as_ref().map_or(0, |(_, s)| s.len());
        let last = count.saturating_sub(1);
        self.selected = self.selected.min(last);
        if down {
            self.selected = (self.selected + 1).min(last);
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        let moved = up || down;
        let mut outcome = if esc { Outcome::Close } else { Outcome::Stay };
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(10))
            .shadow(ctx.global_style().visuals.window_shadow);
        let area = egui::Area::new(egui::Id::new("palette"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 70.0));
        let resp = area.show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(560.0);
                self.field(ui, &t);
                ui.add_space(6.0);
                if let Some(i) = self.list_ui(ui, &t, count, moved) {
                    outcome = Outcome::Choose(i);
                }
            });
        });
        if enter
            && let Some((_, shown)) = &self.shown
            && let Some(s) = shown.get(self.selected)
        {
            outcome = Outcome::Choose(s.index);
        }
        if resp.response.clicked_elsewhere()
            && ctx.input(|i| i.pointer.any_pressed())
            && matches!(outcome, Outcome::Stay)
        {
            outcome = Outcome::Close;
        }
        outcome
    }

    /// The verb chosen, if any, and the field typed in.
    fn field(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let hint = match &self.step {
            Some(step) => {
                ui.label(
                    RichText::new(format!("{} ›", step.title.trim_end_matches('…')))
                        .font(theme::semibold(12.0))
                        .color(t.text_dim),
                );
                ui.add_space(4.0);
                step.pick.placeholder()
            }
            None => "Type a command or a branch…",
        };
        let edit = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(10, 8))
                .font(theme::medium(14.0)),
        );
        // The field has the keyboard while the palette is open: what is clicked or selected
        // underneath doesn't take it.
        if !edit.has_focus() {
            edit.request_focus();
        }
        if edit.changed() {
            self.selected = 0;
            self.scroll = 0.0;
        }
    }

    /// The rows in view (thousands of branches are not all laid out); returns the one clicked.
    fn list_ui(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        count: usize,
        moved: bool,
    ) -> Option<usize> {
        if count == 0 {
            let text = if self.step.is_some() && self.query.is_empty() {
                "Nothing to pick"
            } else {
                "Nothing matches"
            };
            ui.label(RichText::new(text).color(t.text_faint));
            return None;
        }
        let view = LIST_H.min(count as f32 * ROW_H);
        if moved {
            self.scroll = scroll_to_show(self.selected, self.scroll, view);
        }
        let mut clicked = None;
        let (list, shown) = match (&self.step, &self.entries, &self.shown) {
            (Some(step), _, Some((_, shown))) => (&step.choices[..], shown),
            (None, Some((_, entries)), Some((_, shown))) => (&entries[..], shown),
            _ => return None,
        };
        let selected = self.selected;
        let output = ui
            .scope(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical()
                    .max_height(LIST_H)
                    .min_scrolled_height(view)
                    .auto_shrink([false, true])
                    .vertical_scroll_offset(self.scroll)
                    .show_rows(ui, ROW_H, count, |ui, range| {
                        for k in range {
                            let Some(s) = shown.get(k) else { continue };
                            let Some(item) = list.get(s.index) else {
                                continue;
                            };
                            let row = Row {
                                label: &item.label,
                                matched: &s.positions,
                                hint: &item.hint,
                                shortcut: item.shortcut.as_deref(),
                            };
                            if palette_row(ui, t, &row, k == selected) {
                                clicked = Some(s.index);
                            }
                        }
                    })
            })
            .inner;
        self.scroll = output.state.offset.y;
        clicked
    }
}

/// How far to scroll a list `view` tall, scrolled `scroll`, for its row `k` to be in view.
fn scroll_to_show(k: usize, scroll: f32, view: f32) -> f32 {
    let top = k as f32 * ROW_H;
    if top < scroll {
        top
    } else if top + ROW_H > scroll + view {
        top + ROW_H - view
    } else {
        scroll
    }
}
