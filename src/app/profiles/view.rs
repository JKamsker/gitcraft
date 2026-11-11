//! Profile menu, selection rows, and author presentation.

use super::*;

impl App {
    /// The profile in use on the tab strip, a menu to switch to another.
    /// The profile in use, and its menu: the profiles, and `extras`, the title bar's buttons
    /// when it has no room for them.
    pub(in crate::app) fn profile_button(&mut self, ui: &mut Ui, t: &Tokens, extras: &[AppButton]) {
        let profile = self.settings.profile();
        let font = theme::medium(12.0);
        let name =
            ui.painter()
                .layout_no_wrap(profile.display_title().to_owned(), font, t.text_dim);
        let width = (name.size().x + 34.0).min(180.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
        crate::repo::widgets::a11y(&resp, egui::WidgetType::ComboBox, || {
            crate::i18n::message(
                "interaction-tabs-profile-a11y",
                &[("profile", profile.display_title().into())],
            )
        });
        if resp.hovered() {
            ui.painter().rect_filled(rect, t.radius, t.pressed);
        }
        let badge = egui::pos2(rect.left() + 15.0, rect.center().y);
        settings::paint_badge(ui, badge, 9.0, &profile);
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 28.0, rect.top()),
            egui::pos2(rect.right() - 6.0, rect.bottom()),
        );
        ui.painter().with_clip_rect(text_rect).galley(
            egui::pos2(text_rect.left(), rect.center().y - name.size().y / 2.0),
            name,
            t.text_dim,
        );
        let resp = resp.on_hover_text(crate::i18n::message(
            "interaction-tabs-profile-tip",
            &[
                ("profile", profile.display_title().into()),
                ("author", author_line(&profile).into()),
            ],
        ));
        let mut request = None;
        egui::Popup::menu(&resp)
            .style(theme::menu_style)
            .show(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-tabs-profiles"))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                for p in &self.settings.user_profiles {
                    let active = p.id == profile.id;
                    let row = profile_row(ui, p, active, t);
                    let author = author_line(p);
                    let row = if author.is_empty() {
                        row
                    } else {
                        row.on_hover_text(author.trim_start_matches(" · "))
                    };
                    if row.clicked() && !active {
                        request = Some(ProfileRequest::Switch(p.id));
                    }
                }
                ui.separator();
                if ui
                    .button(crate::i18n::text("interaction-tabs-new"))
                    .clicked()
                {
                    request = Some(ProfileRequest::New);
                }
                if ui
                    .button(crate::i18n::text("interaction-tabs-manage"))
                    .clicked()
                {
                    settings::show_page(ui.ctx(), settings::Page::Profile);
                    self.open_settings();
                }
                if !extras.is_empty() {
                    ui.separator();
                    app_buttons::menu_entries(ui, extras);
                }
            });
        if let Some(request) = request {
            self.profile_request(request, ui.ctx());
        }
    }
}

/// A profile's row in the title bar's menu: its badge and name across the menu's width, a
/// check on the one in use.
pub(super) fn profile_row(
    ui: &mut Ui,
    p: &settings::UserProfile,
    active: bool,
    t: &Tokens,
) -> egui::Response {
    let width = ui.available_width().clamp(180.0, 260.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    crate::repo::widgets::a11y_selected(
        &resp,
        || {
            crate::i18n::message(
                "interaction-tabs-profile-a11y",
                &[("profile", p.display_title().into())],
            )
        },
        active,
    );
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.pressed);
    }
    settings::paint_badge(ui, egui::pos2(rect.left() + 13.0, rect.center().y), 8.0, p);
    let color = if active { t.text } else { t.text_dim };
    let font = if active {
        theme::medium(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    let name = ui
        .painter()
        .layout_no_wrap(p.display_title().to_owned(), font, color);
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 28.0, rect.top()),
        egui::pos2(rect.right() - 26.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(text_rect).galley(
        egui::pos2(text_rect.left(), rect.center().y - name.size().y / 2.0),
        name,
        color,
    );
    if active {
        let check = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 13.0, rect.center().y),
            vec2(14.0, 14.0),
        );
        crate::icons::paint(ui, check, "check", 14.0, t.accent);
    }
    resp
}

/// " · Ann <ann@example.com>" for a profile that sets an author, "" otherwise.
pub(super) fn author_line(p: &settings::UserProfile) -> String {
    let (name, email) = (p.author_name.trim(), p.author_email.trim());
    match (name.is_empty(), email.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!(" · {name}"),
        (true, false) => format!(" · <{email}>"),
        (false, false) => format!(" · {name} <{email}>"),
    }
}
