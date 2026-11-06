//! The search bar above the graph: messages or code, in all history, with operators
//! (`author:`, `path:`, `after:`…) and their suggestions.

use egui::{Key, Modifiers, Stroke, Ui};

use super::suggest::{self, Listing};
use crate::git::search_query::Operator;
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// The keys the suggestions took this frame (before the field could).
#[derive(Default)]
struct ListKeys {
    down: bool,
    up: bool,
    tab: bool,
    enter: bool,
    esc: bool,
}

impl RepoTab {
    pub(super) fn search_bar(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "search", 15.0, t.text_faint);
                    self.search_field(ui, acts);
                    self.search_where(ui, acts);
                    ui.label(
                        egui::RichText::new(self.search_summary())
                            .color(t.text_dim)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                            acts.push(Action::ToggleSearch);
                        }
                        if icons::small_button(ui, "chevron-down", "Next (Enter)").clicked() {
                            acts.push(Action::SearchStep(1));
                        }
                        if icons::small_button(ui, "chevron-up", "Previous (Shift+Enter)").clicked()
                        {
                            acts.push(Action::SearchStep(-1));
                        }
                    });
                });
                if let Some(problem) = &self.search.problem {
                    ui.horizontal(|ui| {
                        icons::inline(ui, "circle-alert", 13.0, t.warning);
                        ui.add(
                            egui::Label::new(egui::RichText::new(problem).color(t.warning).small())
                                .truncate(),
                        )
                        .on_hover_text(problem);
                    });
                }
            });
    }

    /// The text field, with its suggestions below it while an operator is typed.
    fn search_field(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let id = ui.make_persistent_id("graph-search-field");
        let listing_open = self.search.suggest.shown > 0 && ui.memory(|m| m.has_focus(id));
        let keys = if listing_open {
            self.take_list_keys(ui)
        } else {
            ListKeys::default()
        };
        let code = self.search.code;
        let out = egui::TextEdit::singleline(&mut self.search.query)
            .id(id)
            .hint_text(if code {
                "Search code added or removed by a commit; or author:, path:, after:…"
            } else {
                "Search messages, authors and hashes; or author:, path:, after:…"
            })
            .desired_width(ui.available_width() - 290.0)
            .margin(egui::Margin::symmetric(8, 4))
            // Tab picks a suggestion instead of leaving the field.
            .lock_focus(listing_open)
            .show(ui);
        let edit = out
            .response
            .response
            .clone()
            .on_hover_text(operators_help());
        if std::mem::take(&mut self.search.focus) {
            edit.request_focus();
        }
        if edit.changed() {
            self.search.edited = Some(std::time::Instant::now());
            self.search.suggest.dismissed = None;
        }
        let on_list = self.search.suggest.popup.is_some_and(|r| {
            ui.input(|i| {
                i.pointer.interact_pos().is_some_and(|p| r.contains(p))
                    && (i.pointer.any_down() || i.pointer.any_released())
            })
        });
        let cursor = (edit.has_focus() || on_list)
            .then(|| out.state.cursor.char_range())
            .flatten()
            .map(|r| r.primary.index.0);
        let listing = match cursor {
            Some(c) if self.search.suggest.dismissed.as_ref() != Some(&self.search.query) => {
                suggest::listing(&self.search.query, c, &self.search.suggest)
            }
            _ => Listing::default(),
        };
        if let Some(kind) = listing.load
            && self.search.suggest.source_idle(kind)
        {
            self.load_suggestions(kind, ui.ctx());
        }
        let accepted = self.list_keys(&listing, &keys);
        let clicked = self.suggestions_popup(ui, edit.rect, &listing);
        if let Some(i) = accepted.or(clicked) {
            self.accept_suggestion(ui.ctx(), id, &listing, i);
            edit.request_focus();
        } else if keys.esc {
            self.search.suggest.dismissed = Some(self.search.query.clone());
            edit.request_focus();
        }
        let (enter, shift, esc) = ui.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.modifiers.shift,
                i.key_pressed(Key::Escape),
            )
        });
        if edit.lost_focus() && enter {
            acts.push(Action::SearchStep(if shift { -1 } else { 1 }));
            edit.request_focus();
        }
        if edit.has_focus() && esc {
            acts.push(Action::ToggleSearch);
        }
    }

    /// Takes the keys the open list uses away from the field: the arrows and Tab always,
    /// Enter and Esc only once a suggestion is highlighted (else they search and close).
    fn take_list_keys(&self, ui: &mut Ui) -> ListKeys {
        let chosen = self.search.suggest.chosen.is_some();
        ui.input_mut(|i| ListKeys {
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            tab: i.consume_key(Modifiers::NONE, Key::Tab),
            enter: chosen && i.consume_key(Modifiers::NONE, Key::Enter),
            esc: chosen && i.consume_key(Modifiers::NONE, Key::Escape),
        })
    }

    /// Moves the highlight with the keys taken; the suggestion picked, if any.
    fn list_keys(&mut self, listing: &Listing, keys: &ListKeys) -> Option<usize> {
        let s = &mut self.search.suggest;
        let n = listing.items.len();
        if s.listed_for != listing.key || s.shown != n {
            s.chosen = None;
            s.listed_for.clone_from(&listing.key);
        }
        s.shown = n;
        if n == 0 {
            return None;
        }
        s.chosen = match (s.chosen, keys.down, keys.up) {
            (None, true, _) => Some(0),
            (Some(c), true, _) => Some((c + 1) % n),
            (None, _, true) => Some(n - 1),
            (Some(c), _, true) => Some((c + n - 1) % n),
            (c, ..) => c.filter(|&c| c < n),
        };
        if keys.tab {
            Some(s.chosen.unwrap_or(0))
        } else if keys.enter {
            s.chosen
        } else {
            None
        }
    }

    /// Puts suggestion `i` in the query in place of the piece it was for, the cursor after it.
    fn accept_suggestion(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        listing: &Listing,
        i: usize,
    ) {
        let Some(item) = listing.items.get(i) else {
            return;
        };
        let (query, cursor) = suggest::accept(&self.search.query, &listing.range, item);
        self.search.query = query;
        self.search.edited = Some(std::time::Instant::now());
        self.search.suggest.chosen = None;
        if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
            let at = egui::text::CCursor::new(cursor);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(at)));
            state.store(ctx, id);
        }
    }

    /// Messages or Code: where free text is looked for.
    fn search_where(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let code = self.search.code;
        for (label, on, tip) in [
            (
                "Messages",
                false,
                "Search commit messages, authors and hashes",
            ),
            (
                "Code",
                true,
                "Search the commits that added or removed this text in a file \
                 (git log -S)",
            ),
        ] {
            if ui
                .selectable_label(code == on, label)
                .on_hover_text(tip)
                .clicked()
            {
                acts.push(Action::SearchCode(on));
            }
        }
    }

    /// Where the search stands: running, no results, or which result is shown.
    fn search_summary(&self) -> String {
        let n = self.search.results.len();
        if self.search.running {
            "searching…".to_owned()
        } else if self.search.searched.is_empty() {
            String::new()
        } else if n == 0 {
            "no results".to_owned()
        } else {
            format!("{} of {n}", self.search.current + 1)
        }
    }
}

/// The field's tooltip: every operator and what it finds.
fn operators_help() -> String {
    let mut help = String::from("Operators, combined with each other and with text:\n");
    for op in Operator::ALL {
        help.push_str(&format!("\n{}:  {}", op.name(), op.help()));
    }
    help.push_str(
        "\n\nQuote values with spaces: author:\"Jane Doe\". \
         Dates: 2024-05-01, 2024-05, 2w, 3m, 1y, today.",
    );
    help
}
