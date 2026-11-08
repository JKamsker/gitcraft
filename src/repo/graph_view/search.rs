//! The search bar above the graph: messages or code, in all history, with operators
//! (`author:`, `path:`, `after:`…) and their suggestions.

use egui::{Key, Modifiers, Stroke, Ui};

use super::suggest::{self, Listing};
use crate::git::search_query::{Operator, date_formats};
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
                    search_icon(ui, &t);
                    self.search_field(ui, acts);
                    self.search_where(ui, acts);
                    crate::repo::authorship_view::filter_toggle(ui, self.authorship.filter, acts);
                    ui.label(
                        egui::RichText::new(self.search_summary())
                            .color(t.text_dim)
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::ToggleSearch);
                        }
                        if icons::small_button(
                            ui,
                            "chevron-down",
                            crate::i18n::text("repo-core-2712"),
                        )
                        .clicked()
                        {
                            acts.push(Action::SearchStep(1));
                        }
                        if icons::small_button(
                            ui,
                            "chevron-up",
                            crate::i18n::text("repo-core-2713"),
                        )
                        .clicked()
                        {
                            acts.push(Action::SearchStep(-1));
                        }
                    });
                });
            });
    }

    /// The text field, with its suggestions below it while an operator is typed.
    fn search_field(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let id = ui.make_persistent_id("graph-search-field");
        // Not while the manual, a dialog or the palette is over the tab: its keys are its own.
        let free = self.dialog.is_none() && !self.keys_blocked;
        let listing_open = free && self.search.suggest.open && ui.memory(|m| m.has_focus(id));
        let keys = if listing_open {
            self.take_list_keys(ui)
        } else {
            ListKeys::default()
        };
        let code = self.search.code;
        let out = egui::TextEdit::singleline(&mut self.search.query)
            .id(id)
            .hint_text(if code {
                crate::i18n::text("repo-core-2715")
            } else {
                crate::i18n::text("repo-core-2716")
            })
            .desired_width(ui.available_width() - 400.0)
            .margin(egui::Margin::symmetric(8, 4))
            // While the list is open, Tab picks a suggestion and Esc closes the list, instead
            // of egui taking the focus off the field for them.
            .event_filter(egui::EventFilter {
                tab: listing_open,
                escape: listing_open,
                horizontal_arrows: true,
                vertical_arrows: true,
            })
            .show(ui);
        let edit = out.response.response.clone();
        if std::mem::take(&mut self.search.focus) {
            edit.request_focus();
        }
        if edit.changed() {
            self.authorship.filter = false;
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
                let query = self.search.query.clone();
                self.search.suggest.listing_for(&query, c)
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
        if self.search.suggest.popup.is_none() {
            self.problem_hint(ui, edit.rect);
        }
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

    /// Takes the keys the open list uses away from the field: the arrows, Tab and Esc (which
    /// closes the list, not the search), and Enter once a suggestion is highlighted (else it
    /// goes to the next result).
    fn take_list_keys(&self, ui: &mut Ui) -> ListKeys {
        let chosen = self.search.suggest.chosen.is_some();
        ui.input_mut(|i| ListKeys {
            down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
            up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
            tab: i.consume_key(Modifiers::NONE, Key::Tab),
            enter: chosen && i.consume_key(Modifiers::NONE, Key::Enter),
            esc: i.consume_key(Modifiers::NONE, Key::Escape),
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
        s.open = n > 0 || listing.status.is_some();
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

    /// Why the query isn't searched, floating under the field (the graph doesn't move).
    fn problem_hint(&self, ui: &Ui, field: egui::Rect) {
        let Some(problem) = &self.search.problem else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        egui::Area::new(ui.id().with("search-problem"))
            .order(egui::Order::Foreground)
            .fixed_pos(field.left_bottom() + egui::vec2(0.0, 4.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.warning))
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_max_width(field.width().max(320.0));
                        ui.horizontal(|ui| {
                            icons::inline(ui, "circle-alert", 13.0, t.warning);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(problem).color(t.text).small(),
                                )
                                .wrap(),
                            );
                        });
                    });
            });
    }

    /// Messages or Code: where free text is looked for.
    fn search_where(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let code = self.search.code;
        for (label, on, tip) in [
            (
                crate::i18n::text("repo-core-2718"),
                false,
                crate::i18n::text("repo-core-2719"),
            ),
            (
                crate::i18n::text("repo-core-2720"),
                true,
                crate::i18n::text("repo-core-2721"),
            ),
        ] {
            if ui
                .selectable_label(code == on, label)
                .on_hover_text(tip)
                .clicked()
            {
                self.authorship.filter = false;
                acts.push(Action::SearchCode(on));
            }
        }
    }

    /// Where the search stands: running, no results, or which result is shown.
    fn search_summary(&self) -> String {
        let n = self.search.results.len();
        if self.search.running {
            crate::i18n::text("repo-core-2722").to_owned()
        } else if self.search.searched.is_empty() {
            String::new()
        } else if n == 0 {
            crate::i18n::text("repo-core-2723").to_owned()
        } else {
            crate::i18n::message(
                "repo-core-2724",
                &[
                    ("current", format!("{}", self.search.current + 1).into()),
                    ("n", (n).into()),
                ],
            )
        }
    }
}

/// The magnifier, its tooltip every operator and what it finds.
fn search_icon(ui: &mut Ui, t: &Tokens) {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(15.0), egui::Sense::hover());
    icons::paint(ui, rect, "search", 15.0, t.text_faint);
    resp.on_hover_ui(|ui| {
        ui.set_max_width(460.0);
        ui.label(operators_help());
    });
}

/// The field's help: every operator and what it finds.
fn operators_help() -> String {
    let mut help = String::from(crate::i18n::text("repo-core-2725"));
    for op in Operator::ALL {
        help.push_str(&format!("\n{}:  {}", op.name(), op.help()));
    }
    help.push_str(&crate::i18n::message(
        "repo-core-2726",
        &[("DATE-FORMATS", date_formats().into())],
    ));
    help
}
