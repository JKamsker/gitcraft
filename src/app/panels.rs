//! The settings tab, the manual window, and toasts for long operations that ended.

use std::time::{Duration, Instant};

use egui::{RichText, Stroke, Ui, vec2};

use super::{App, Tab};
use crate::icons;
use crate::theme::{self, Tokens};
use crate::update;

pub(super) struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

impl Toast {
    /// A toast shown for six seconds from now.
    pub(super) fn new(text: String, error: bool) -> Self {
        Self::for_secs(text, error, 6)
    }

    /// A toast shown for `secs` seconds from now.
    pub(super) fn for_secs(text: String, error: bool, secs: u64) -> Self {
        Self {
            text,
            error,
            until: Instant::now() + Duration::from_secs(secs),
        }
    }
}

impl App {
    /// Focuses the settings tab, opening it after the others when there is none.
    pub(super) fn open_settings(&mut self) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Settings)) {
            self.active = i;
        } else {
            self.tabs.push(Tab::Settings);
            self.active = self.tabs.len() - 1;
        }
    }

    /// The settings tab: its pages beside the chosen one (see [`crate::settings`]).
    pub(super) fn settings_page(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let version = self.git_version.clone().unwrap_or_default();
        let mut view = crate::settings::SettingsView {
            theme: self.theme.clone(),
            git_version: &version,
            dialog_open: self.clone_form.is_some()
                || self.remote_form.is_some()
                || self.teleport.is_some()
                || self.askpass_open()
                || self.palette.is_some()
                || self.practice_open
                || self.setup.open
                || self.manual_open
                || self.my_work.open,
            update: self.update_check(),
            check_updates_now: false,
            previous: self.previous_version.as_ref().map(|p| p.label().to_owned()),
            previous_is_newer: self
                .previous_version
                .as_ref()
                .is_some_and(update::Previous::is_newer),
            pinned: self.pinned_version.clone(),
            go_back: false,
            unpin: false,
            request: None,
        };
        let mut repositories = self.recent.clone();
        for tab in &self.tabs {
            if let Tab::Repo(repo) = tab
                && !repositories.contains(&repo.path)
            {
                repositories.push(repo.path.clone());
            }
        }
        crate::settings::network_page::repositories(&ctx, repositories);
        let tweaks = self.settings.theme_tweaks;
        if self.settings.page(ui, &mut view) {
            self.settings.set(&ctx);
            if self.settings.theme_tweaks != tweaks {
                self.apply_theme(&ctx);
            }
        }
        let check_now = view.check_updates_now;
        if view.go_back {
            self.go_back(&ctx);
        }
        if view.unpin {
            self.unpin();
        }
        if let Some(request) = view.request {
            self.profile_request(request, &ctx);
        }
        if check_now {
            self.check_update_now(&ctx);
        }
    }

    pub(super) fn manual_ui(&mut self, ctx: &egui::Context) {
        if !self.manual_open {
            return;
        }
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut close = false;
        let mut copy = false;
        let collecting = self.diagnostics.is_some();
        let resp = egui::Modal::new(egui::Id::new("manual"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(crate::i18n::text("interaction-palette-manual-caption"))
                            .font(theme::semibold(16.0)),
                    );
                    ui.label(
                        RichText::new(format!("gitcraft {}", update::CURRENT)).color(t.text_faint),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-palette-close"),
                        )
                        .clicked()
                        {
                            close = true;
                        }
                        let tip = crate::i18n::text("interaction-palette-diagnostic-hint");
                        if ui
                            .add_enabled(
                                !collecting,
                                egui::Button::new(crate::i18n::text("interaction-palette-copy")),
                            )
                            .on_hover_text(tip)
                            .on_disabled_hover_text(crate::i18n::text(
                                "interaction-palette-collecting",
                            ))
                            .clicked()
                        {
                            copy = true;
                        }
                    });
                });
                let max = (ctx.content_rect().height() - 140.0).max(240.0);
                egui::ScrollArea::vertical()
                    .max_height(max)
                    .min_scrolled_height(max)
                    .show(ui, crate::manual::ui);
            });
        if close || resp.should_close() {
            self.manual_open = false;
        }
        if copy {
            self.copy_diagnostics(ctx);
        }
    }

    /// Long operations that ended: a toast, and a desktop notification when away.
    pub(super) fn toast_finished(&mut self, focused: bool) {
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
        {
            let Tab::Repo(r) = tab else { continue };
            for (text, error) in r.finished.drain(..) {
                if !focused && self.settings.notifications {
                    notify(&text);
                }
                self.toasts.push(Toast::new(text, error));
            }
        }
    }

    pub(super) fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        let t = Tokens::get(ctx);
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    egui::Frame::new()
                        .fill(t.card)
                        .stroke(Stroke::new(
                            1.0,
                            if toast.error { t.danger } else { t.success },
                        ))
                        .corner_radius(t.radius_lg)
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .shadow(ctx.global_style().visuals.popup_shadow)
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.horizontal(|ui| {
                                let icon = if toast.error { "circle-alert" } else { "check" };
                                icons::inline(
                                    ui,
                                    icon,
                                    16.0,
                                    if toast.error { t.danger } else { t.success },
                                );
                                ui.add(egui::Label::new(display_app_message(&toast.text)).wrap());
                            });
                        });
                    ui.add_space(6.0);
                }
            });
    }
}

/// A desktop notification, on its own thread (it can take a moment to show).
fn notify(text: &str) {
    let text = notification_body(text);
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("gitcraft")
            .summary("gitcraft")
            .body(&text)
            .show();
    });
}

/// `text` as a notification's body. Linux notification servers read the body as markup, and
/// what is shown comes partly from a remote's messages: its `<` and `&` are text, not tags or
/// links.
pub(super) fn notification_body(text: &str) -> String {
    if cfg!(all(unix, not(target_os = "macos"))) {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_shows_tags_as_text() {
        let body = notification_body("push failed: remote: <a href=\"https://x\">see</a> & more");
        if cfg!(all(unix, not(target_os = "macos"))) {
            assert_eq!(
                body,
                "push failed: remote: &lt;a href=\"https://x\"&gt;see&lt;/a&gt; &amp; more"
            );
        } else {
            assert!(body.contains("<a href"));
        }
    }
}

/// App-owned status/error paint; all classification and stored values remain raw.
pub(super) fn display_app_message(raw: &str) -> String {
    super::clone::display_message(raw)
        .or_else(|| super::remote_open::display_message(raw))
        .or_else(|| super::repo_scan::display_error(raw))
        .or_else(|| super::draft_saving::display_notice(raw))
        .or_else(|| crate::launch::display_message(raw))
        .or_else(|| super::startup_display::message(raw))
        .or_else(|| crate::portable::display_message(raw))
        .unwrap_or_else(|| {
            let remote = crate::remote::display::diagnostic(raw);
            if remote == raw {
                crate::git::display_error(raw)
            } else {
                remote
            }
        })
}

#[cfg(test)]
mod remote_display_tests {
    #[test]
    fn actual_app_message_paint_uses_canonical_remote_adapter_after_own_classifiers() {
        crate::i18n::set_language("de");
        let raw = "it is a symbolic link";
        assert_eq!(
            super::display_app_message(raw),
            "Dies ist ein symbolischer Link."
        );
        assert_eq!(raw, "it is a symbolic link");
        assert_eq!(
            super::display_app_message("gitcraft isn't allowed to look in /opaque/日本語."),
            "gitcraft darf /opaque/日本語 nicht durchsuchen."
        );
        let unknown = "opaque external 日本語\n: unchanged";
        assert_eq!(super::display_app_message(unknown), unknown);
        // No command/Host provenance is available at this generic paint boundary.
        let command = "opaque command on SSH · opaque: invalid native request";
        assert_eq!(super::display_app_message(command), command);
        crate::i18n::set_language("en-US");
        assert_eq!(super::display_app_message(raw), raw);
    }
}
