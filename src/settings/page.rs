//! The settings tab: its pages listed on the left under a search field, the chosen page on the
//! right; while searching, what the search matches on every page instead.

use egui::{Align2, Id, Key, Modifiers, RichText, Sense, Stroke, Ui, vec2};

use super::layout::Filter;
use super::{Page, Settings};
use crate::theme::{self, Tokens};

/// Width of the list of pages.
const NAV_WIDTH: f32 = 210.0;
/// Room between the list and the page.
const GAP: f32 = 36.0;
/// Widest a page gets.
const CONTENT_WIDTH: f32 = 720.0;
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

fn search_id() -> Id {
    Id::new("settings-search")
}

impl Settings {
    /// The settings tab. Returns true when a setting changed.
    pub fn page(&mut self, ui: &mut Ui) -> bool {
        let before = self.clone();
        let mut view: View = ui.data(|d| d.get_temp(view_id())).unwrap_or_default();
        keys(ui, &mut view);
        // How many settings the search finds on each page.
        let counts: Vec<usize> = if view.query.trim().is_empty() {
            Vec::new()
        } else {
            Page::ALL
                .iter()
                .map(|&page| {
                    let filter = Filter::new(&view.query, page.title(), true);
                    self.page_ui(ui, page, &filter);
                    filter.found()
                })
                .collect()
        };

        let side = ((ui.available_width() - (NAV_WIDTH + GAP + CONTENT_WIDTH)) / 2.0).max(16.0);
        ui.horizontal_top(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(NAV_WIDTH);
                nav(ui, &mut view, &counts);
            });
            ui.add_space(GAP);
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
                        if counts.is_empty() {
                            title(ui, view.page.title());
                            self.page_ui(ui, view.page, &Filter::new("", view.page.title(), false));
                        } else {
                            self.results(ui, &mut view, &counts);
                        }
                        ui.add_space(40.0);
                    });
                });
        });
        ui.data_mut(|d| d.insert_temp(view_id(), view));
        *self != before
    }

    /// What the search finds, under the title of each page it is on.
    fn results(&mut self, ui: &mut Ui, view: &mut View, counts: &[usize]) {
        let t = Tokens::get(ui.ctx());
        title(ui, "Search results");
        let total: usize = counts.iter().sum();
        let query = view.query.trim().to_owned();
        let what = match total {
            0 => format!("Nothing matches “{query}”."),
            1 => format!("1 setting matches “{query}”."),
            n => format!("{n} settings match “{query}”."),
        };
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
                        RichText::new(page.title().to_uppercase())
                            .font(theme::semibold(11.0))
                            .color(t.text_faint),
                    );
                })
                .response
                .interact(Sense::click())
                .on_hover_text("Open this page")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                view.page = page;
                view.query.clear();
            }
            self.page_ui(ui, page, &Filter::new(&query, page.title(), false));
        }
    }
}

/// The page's title at the top.
fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::semibold(22.0)));
    ui.add_space(4.0);
}

/// The settings' shortcuts: Ctrl+F searches, Esc clears the search, Alt+↑/↓ go through the
/// pages.
fn keys(ui: &mut Ui, view: &mut View) {
    let popup_open = egui::Popup::is_any_open(ui.ctx());
    let (search, clear, up, down) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::COMMAND, Key::F),
            !popup_open && i.key_pressed(Key::Escape),
            i.consume_key(Modifiers::ALT, Key::ArrowUp),
            i.consume_key(Modifiers::ALT, Key::ArrowDown),
        )
    });
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
        let (rect, resp) = ui.allocate_exact_size(vec2(NAV_WIDTH, 32.0), Sense::click());
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
            page.title(),
            font,
            text,
        );
        if let Some(n) = count.filter(|&n| n > 0) {
            ui.painter().text(
                rect.right_center() - vec2(12.0, 0.0),
                Align2::RIGHT_CENTER,
                n.to_string(),
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
                        if crate::icons::small_button(ui, "x", "Clear (Esc)").clicked() {
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
                            .hint_text("Search settings")
                            .margin(vec2(0.0, 3.0))
                            .desired_width(ui.available_width()),
                    );
                });
            });
        });
}
