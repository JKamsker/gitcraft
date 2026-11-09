//! The insights page: its title bar, the views listed on the left by group (a menu instead when
//! the area is narrow), and the view chosen beside them.

use egui::{Align2, RichText, Sense, Ui, vec2};

use super::{InsightsAct, Kind};
use crate::icons;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// Width of the list of views.
const NAV_WIDTH: f32 = 200.0;
/// Narrower than this, the list is a menu above the view.
const NARROW: f32 = 760.0;

impl RepoTab {
    /// The big area, while the insights page is open.
    pub(in crate::repo) fn insights_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(view) = self.insights.as_deref() else {
            return;
        };
        let (kind, table) = (view.kind, view.table);
        widgets::panel_header(ui, "Insights", |ui| {
            if icons::small_button(ui, "x", "Close insights (Esc)").clicked() {
                acts.push(Action::Insights(InsightsAct::Close));
            }
            if icons::small_button(ui, "refresh-cw", "Read again").clicked() {
                acts.push(Action::Insights(InsightsAct::Reload));
            }
            let tip = if table {
                "Show the picture"
            } else {
                "Show as a table"
            };
            if icons::toggle_button(ui, "list", tip, table)
                .on_hover_text(tip)
                .clicked()
            {
                acts.push(Action::Insights(InsightsAct::ToggleTable));
            }
        });
        let narrow = ui.available_width() < NARROW;
        if narrow {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                menu(ui, kind, acts);
            });
            content(self, ui, kind, acts);
            return;
        }
        ui.horizontal_top(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.set_width(NAV_WIDTH);
                egui::ScrollArea::vertical()
                    .id_salt("insights-nav")
                    .auto_shrink([false, false])
                    .show(ui, |ui| nav(ui, kind, acts));
            });
            ui.add_space(12.0);
            ui.vertical(|ui| content(self, ui, kind, acts));
        });
    }
}

/// The view's title, its line, and the view.
fn content(tab: &mut RepoTab, ui: &mut Ui, kind: Kind, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(
            RichText::new(kind.title())
                .font(theme::semibold(16.0))
                .color(t.text),
        );
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(kind.blurb()).color(t.text_faint)).truncate());
    });
    ui.add_space(4.0);
    let width = ui.available_width() - 8.0;
    ui.vertical(|ui| {
        ui.set_max_width(width);
        tab.insight_ui(kind, ui, acts);
    });
}

/// The views by group, the one shown marked.
fn nav(ui: &mut Ui, shown: Kind, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let mut group = "";
    for kind in sorted() {
        if kind.group() != group {
            group = kind.group();
            ui.add_space(8.0);
            ui.label(
                RichText::new(group.to_uppercase())
                    .font(theme::medium(10.5))
                    .color(t.text_faint),
            );
            ui.add_space(2.0);
        }
        let selected = kind == shown;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        crate::a11y::a11y_selected(&resp, kind.title(), selected);
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
                rect.left_top() + vec2(0.0, 6.0),
                vec2(3.0, rect.height() - 12.0),
            );
            painter.rect_filled(bar, 1.5, t.accent);
        }
        let (text, icon) = if selected {
            (t.text, t.accent)
        } else {
            (t.text_dim, t.icon)
        };
        let icon_rect =
            egui::Rect::from_center_size(rect.left_center() + vec2(18.0, 0.0), vec2(14.0, 14.0));
        icons::paint(ui, icon_rect, kind.icon(), 14.0, icon);
        let font = if selected {
            theme::medium(12.5)
        } else {
            egui::FontId::proportional(12.5)
        };
        ui.painter().text(
            rect.left_center() + vec2(34.0, 0.0),
            Align2::LEFT_CENTER,
            kind.title(),
            font,
            text,
        );
        if resp
            .on_hover_text(kind.blurb())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !selected
        {
            acts.push(Action::Insights(InsightsAct::Open(kind)));
        }
    }
}

/// The views as a menu, for a narrow area.
fn menu(ui: &mut Ui, shown: Kind, acts: &mut Vec<Action>) {
    egui::ComboBox::from_id_salt("insights-menu")
        .selected_text(format!("{} · {}", shown.group(), shown.title()))
        .width(280.0)
        .show_ui(ui, |ui| {
            for kind in sorted() {
                let label = format!("{} · {}", kind.group(), kind.title());
                if ui.selectable_label(kind == shown, label).clicked() && kind != shown {
                    acts.push(Action::Insights(InsightsAct::Open(kind)));
                }
            }
        });
}

/// The views grouped, each group in the order its first view is listed.
fn sorted() -> Vec<Kind> {
    let mut groups: Vec<&str> = Vec::new();
    for kind in Kind::ALL {
        if !groups.contains(&kind.group()) {
            groups.push(kind.group());
        }
    }
    groups
        .iter()
        .flat_map(|g| Kind::ALL.into_iter().filter(move |k| k.group() == *g))
        .collect()
}
