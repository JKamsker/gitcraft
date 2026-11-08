//! The settings tab, along the window's left edge: its pages listed under a search field, the
//! chosen page beside them; while searching, what the search matches on every page instead.

use egui::{Align2, Id, Key, RichText, Sense, Stroke, Ui, vec2};

use super::layout::Filter;
use super::preview;
use super::{Page, Settings, SettingsView};
use crate::theme::{self, Tokens};

/// Width of the list of pages, and the room between it and the page; less in a small window.
fn nav_size(available: f32) -> (f32, f32) {
    if available < 1000.0 {
        (170.0, 20.0)
    } else {
        (210.0, 36.0)
    }
}
/// Widest a page gets.
const CONTENT_WIDTH: f32 = 720.0;
/// Room left of the list: the settings keep to the window's left edge.
const SIDE: f32 = 24.0;
/// Room above the page's title and the list's.
const TOP: f32 = 32.0;

/// Which page shows, and what is being searched for. Kept while the app runs.
#[derive(Clone, Default)]
struct View {
    page: Page,
    query: String,
}

fn view_id() -> Id {
    Id::new("settings-view")
}

/// Has the settings tab show `page` next, with no search.
pub(crate) fn show(ctx: &egui::Context, page: Page) {
    ctx.data_mut(|d| {
        d.insert_temp(
            view_id(),
            View {
                page,
                query: String::new(),
            },
        );
    });
}

fn search_id() -> Id {
    Id::new("settings-search")
}

impl Settings {
    pub(crate) fn search_has_focus(ctx: &egui::Context) -> bool {
        ctx.memory(|memory| memory.focused()) == Some(search_id())
    }

    /// The settings tab. Returns true when a setting changed; what the Profile page asks the
    /// app to do is left in `view`.
    pub fn page(&mut self, ui: &mut Ui, app: &mut SettingsView) -> bool {
        let before = self.clone();
        let mut view: View = ui.data(|d| d.get_temp(view_id())).unwrap_or_default();
        keys(ui, &mut view, app.dialog_open);
        // Another search finds other rows: the preview of one found before goes.
        let searched = Id::new("settings-preview-query");
        if ui.data(|d| d.get_temp::<String>(searched)).as_deref() != Some(view.query.as_str()) {
            preview::forget(ui.ctx());
            ui.data_mut(|d| d.insert_temp(searched, view.query.clone()));
        }
        // How many settings the search finds on each page.
        let counts: Vec<usize> = if view.query.trim().is_empty() {
            Vec::new()
        } else {
            Page::ALL
                .iter()
                .map(|&page| {
                    let filter = Filter::new(&view.query, page.title(), true);
                    self.page_ui(ui, page, &filter, app);
                    filter.found()
                })
                .collect()
        };

        if ui.available_width() < 760.0 {
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                compact_page_picker(ui, &mut view);
                search_field(ui, &mut view.query);
            });
            let above = counts
                .is_empty()
                .then(|| preview::shown(ui.ctx(), Some(view.page)))
                .flatten();
            self.page_column(ui, &mut view, &counts, app, above);
            ui.data_mut(|d| d.insert_temp(view_id(), view));
            return *self != before;
        }
        let (nav_width, gap) = nav_size(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.add_space(SIDE);
            ui.vertical(|ui| {
                ui.set_width(nav_width);
                nav(ui, &mut view, &counts);
            });
            ui.add_space(gap);
            // What the setting pointed at does, beside the page; above it in a narrow window.
            let at = counts.is_empty().then_some(view.page);
            // Nothing found, nothing to preview.
            let found = counts.is_empty() || counts.iter().any(|&n| n > 0);
            let shown = preview::shown(ui.ctx(), at).filter(|_| found);
            let room = ui.available_width() - preview::SIDE_MARGIN;
            let height = ui.available_height();
            match (shown, shown.and_then(|_| preview::side_width(room))) {
                (Some(shown), Some(side)) => {
                    let page_w = room - side - preview::SIDE_GAP;
                    ui.allocate_ui_with_layout(
                        vec2(page_w, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.page_column(ui, &mut view, &counts, app, None),
                    );
                    ui.add_space(preview::SIDE_GAP - ui.spacing().item_spacing.x);
                    ui.allocate_ui_with_layout(
                        vec2(side, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("settings-preview-side")
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.add_space(TOP + 38.0);
                                    preview::panel(ui, shown, self, &app.theme);
                                    ui.add_space(24.0);
                                });
                        },
                    );
                }
                (Some(shown), None) => self.page_column(ui, &mut view, &counts, app, Some(shown)),
                _ => self.page_column(ui, &mut view, &counts, app, None),
            }
        });
        ui.data_mut(|d| d.insert_temp(view_id(), view));
        *self != before
    }

    /// The page, or what the search finds, scrolling.
    fn page_column(
        &mut self,
        ui: &mut Ui,
        view: &mut View,
        counts: &[usize],
        app: &mut SettingsView,
        above_preview: Option<preview::Preview>,
    ) {
        let salt = if counts.is_empty() {
            Id::new(("settings-page", view.page as u8))
        } else {
            Id::new("settings-results")
        };
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = CONTENT_WIDTH.min(ui.available_width() - 16.0);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.add_space(TOP);
                    // An expanded preview shares the page's scroll range. Keeping it above
                    // this ScrollArea could consume all height at high zoom, leaving even
                    // scroll_to_me unable to bring the settings into the viewport.
                    if let Some(shown) = above_preview {
                        egui::CollapsingHeader::new(crate::i18n::text("dev-baseline-preview"))
                            .id_salt("settings-preview-above-header")
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("settings-preview-above")
                                    .max_height(preview::ABOVE_MAX_H)
                                    .show(ui, |ui| preview::panel(ui, shown, self, &app.theme));
                            });
                        ui.add_space(12.0);
                    }
                    if counts.is_empty() {
                        title(ui, view.page.title());
                        self.page_ui(
                            ui,
                            view.page,
                            &Filter::new("", view.page.title(), false).on_page(view.page),
                            app,
                        );
                    } else {
                        self.results(ui, view, counts, app);
                    }
                    ui.add_space(40.0);
                });
            });
    }

    /// What the search finds, under the title of each page it is on.
    fn results(&mut self, ui: &mut Ui, view: &mut View, counts: &[usize], app: &mut SettingsView) {
        let t = Tokens::get(ui.ctx());
        title(ui, "Search results");
        let total: usize = counts.iter().sum();
        let query = view.query.trim().to_owned();
        let what = crate::i18n::message(
            "interaction-settings-search-matches",
            &[("count", total.into()), ("query", query.as_str().into())],
        );
        ui.label(RichText::new(what).color(t.text_faint));
        for (&page, &count) in Page::ALL.iter().zip(counts) {
            if count == 0 {
                continue;
            }
            ui.add_space(26.0);
            // The page's name, opening it.
            let resp = ui
                .horizontal(|ui| {
                    crate::icons::inline(ui, page.icon(), 14.0, t.text_faint);
                    ui.label(
                        RichText::new(super::labels::display(page.title()).to_uppercase())
                            .font(theme::semibold(11.0))
                            .color(t.text_faint),
                    );
                })
                .response
                .interact(Sense::click())
                .on_hover_text(crate::i18n::text("interaction-settings-open-page"));
            crate::repo::widgets::a11y(&resp, egui::WidgetType::Link, || {
                crate::i18n::message(
                    "interaction-settings-open-page-a11y",
                    &[("page", super::labels::display(page.title()).into())],
                )
            });
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                view.page = page;
                view.query.clear();
            }
            self.page_ui(ui, page, &Filter::new(&query, page.title(), false), app);
        }
    }
}

/// The page's title at the top.
fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(super::labels::display(text)).font(theme::semibold(22.0)));
    ui.add_space(4.0);
}

/// The settings' shortcuts: Ctrl+F searches, Esc clears the search, Alt+↑/↓ go through the
/// pages.
fn keys(ui: &mut Ui, view: &mut View, dialog_open: bool) {
    // A dialog over the page has the keys; so does a field of a page being typed in (Esc or
    // leaving the page would leave it without its lost focus, which is when it is saved).
    let typing = ui
        .memory(|m| m.focused())
        .is_some_and(|focused| focused != search_id());
    if dialog_open || typing {
        return;
    }
    let popup_open = egui::Popup::is_any_open(ui.ctx());
    use crate::keymap::{self, Command, Scope};
    keymap::active(ui.ctx(), vec![Scope::Window, Scope::Settings]);
    let search = keymap::take(ui.ctx(), Command::SettingsSearch);
    let clear = !popup_open && ui.input(|i| i.key_pressed(Key::Escape));
    let up = keymap::take(ui.ctx(), Command::SettingsPrevious);
    let down = keymap::take(ui.ctx(), Command::SettingsNext);
    if search {
        ui.memory_mut(|m| m.request_focus(search_id()));
    }
    if clear && !view.query.is_empty() {
        view.query.clear();
        ui.memory_mut(|m| m.surrender_focus(search_id()));
    }
    if up || down {
        let at = Page::ALL.iter().position(|&p| p == view.page).unwrap_or(0);
        let n = Page::ALL.len();
        let to = if up { (at + n - 1) % n } else { (at + 1) % n };
        view.page = Page::ALL[to];
        view.query.clear();
    }
}

/// The list on the left: the title, the search field and the pages.
fn nav(ui: &mut Ui, view: &mut View, counts: &[usize]) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(TOP);
    title(ui, "Settings");
    ui.add_space(10.0);
    search_field(ui, &mut view.query);
    ui.add_space(12.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let searching = !counts.is_empty();
    for (i, &page) in Page::ALL.iter().enumerate() {
        let count = counts.get(i).copied();
        let selected = !searching && page == view.page;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        crate::repo::widgets::a11y_selected(
            &resp,
            || {
                let matches = count.map(|n| crate::a11y::labels::count(n, "match", "matches"));
                crate::a11y::labels::join([
                    super::labels::display(page.title()),
                    &matches.unwrap_or_default(),
                ])
            },
            selected,
        );
        let fill = if selected {
            t.hover
        } else if resp.hovered() {
            t.card
        } else {
            egui::Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, t.radius, fill);
        if selected {
            let bar = egui::Rect::from_min_size(
                rect.left_top() + vec2(0.0, 7.0),
                vec2(3.0, rect.height() - 14.0),
            );
            painter.rect_filled(bar, 1.5, t.accent);
        }
        let dim = count == Some(0);
        let (text, icon) = if dim {
            (t.text_faint, t.text_faint)
        } else if selected {
            (t.text, t.accent)
        } else {
            (t.text_dim, t.icon)
        };
        let icon_rect =
            egui::Rect::from_center_size(rect.left_center() + vec2(22.0, 0.0), vec2(16.0, 16.0));
        crate::icons::paint(ui, icon_rect, page.icon(), 16.0, icon);
        let font = if selected {
            theme::medium(13.0)
        } else {
            egui::FontId::proportional(13.0)
        };
        ui.painter().text(
            rect.left_center() + vec2(40.0, 0.0),
            Align2::LEFT_CENTER,
            super::labels::display(page.title()),
            font,
            text,
        );
        if let Some(n) = count.filter(|&n| n > 0) {
            ui.painter().text(
                rect.right_center() - vec2(12.0, 0.0),
                Align2::RIGHT_CENTER,
                crate::i18n::number(n),
                egui::FontId::proportional(11.5),
                t.text_faint,
            );
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            view.page = page;
            view.query.clear();
        }
    }
}

/// The search field: an icon, the text, and a button to clear it (or its shortcut while
/// empty).
fn search_field(ui: &mut Ui, query: &mut String) {
    let t = Tokens::get(ui.ctx());
    let focused = ui.memory(|m| m.has_focus(search_id()));
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(
            1.0,
            if focused { t.accent } else { t.field_border },
        ))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                crate::icons::inline(ui, "search", 14.0, t.text_faint);
                // The button or shortcut at the right end, the text taking what is left.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !query.is_empty() {
                        if crate::icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-settings-clear"),
                        )
                        .clicked()
                        {
                            query.clear();
                        }
                    } else if !focused {
                        ui.label(
                            RichText::new(format!("{}+F", crate::manual::command()))
                                .font(theme::mono(10.5))
                                .color(t.text_faint),
                        );
                    }
                    ui.add(
                        egui::TextEdit::singleline(query)
                            .id(search_id())
                            .frame(egui::Frame::NONE)
                            .hint_text(crate::i18n::text("interaction-settings-search"))
                            .margin(vec2(0.0, 3.0))
                            .desired_width(ui.available_width()),
                    );
                });
            });
        });
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::keymap::{Command, Stroke as KeyStroke};
    #[test]
    fn configured_page_and_search_keys_use_actual_settings_handler() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.key_bindings.insert(
            Command::SettingsNext.id().into(),
            vec![KeyStroke::new(Key::F8, egui::Modifiers::NONE)],
        );
        settings.key_bindings.insert(
            Command::SettingsSearch.id().into(),
            vec![KeyStroke::new(Key::F9, egui::Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        let mut view = View::default();
        let before = view.page;
        for key in [Key::F8, Key::F9] {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    keys(ui, &mut view, false);
                    ui.add(egui::TextEdit::singleline(&mut view.query).id(search_id()));
                },
            )
            .drop_without_applying_deltas();
        }
        assert_ne!(view.page, before);
        assert!(ctx.memory(|memory| memory.has_focus(search_id())));
    }
}

/// Paint localized captions while the selected enum and persistent salt stay stable.
fn compact_page_picker(ui: &mut Ui, view: &mut View) {
    egui::ComboBox::from_id_salt("settings-page-compact")
        .selected_text(super::labels::display(view.page.title()))
        .show_ui(ui, |ui| {
            for page in Page::ALL {
                if ui
                    .selectable_value(&mut view.page, page, super::labels::display(page.title()))
                    .clicked()
                {
                    view.query.clear();
                }
            }
        });
}

#[cfg(test)]
mod merge_regressions {
    use super::*;
    use egui_kittest::kittest::{NodeT, Queryable};

    #[test]
    fn compact_page_picker_repaints_both_languages_and_keeps_enum_selection() {
        let mut h = egui_kittest::Harness::new_ui_state(compact_page_picker, View::default());
        for language in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(language);
            h.run();
            let picker = h.get_by_role(egui::accesskit::Role::ComboBox);
            assert_eq!(
                picker.accesskit_node().data().value(),
                Some(super::super::labels::display(Page::General.title()))
            );
            picker.click();
            h.run();
            h.get_by_label(super::super::labels::display(Page::Appearance.title()))
                .click();
            h.run();
            assert_eq!(h.state().page, Page::Appearance);
            assert!(h.state().query.is_empty());
            // Reset only the selection for the next locale, using the same Context and salt.
            h.state_mut().page = Page::General;
        }
        crate::i18n::set_language("en-US");
    }
}
