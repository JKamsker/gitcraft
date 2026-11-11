//! Application command routing from buttons and keyboard.

use super::*;

impl App {
    /// Carries out what was done with the app's buttons this frame, wherever they are.
    pub(super) fn app_button_events(&mut self, ctx: &egui::Context) {
        use crate::app_buttons::{AppButton, Event};
        use crate::settings::ThemePick;
        for event in crate::app_buttons::take_events(ctx) {
            match event {
                Event::Clicked(AppButton::Settings) => self.open_settings(),
                Event::Clicked(AppButton::Palette) => self.open_palette(),
                Event::Clicked(AppButton::Manual) => self.manual_open = true,
                // The theme button opens its menu rather than being clicked.
                Event::Clicked(AppButton::Theme) => {}
                Event::Theme(ThemePick::Theme(id)) => self.set_theme(ctx, id),
                Event::Theme(ThemePick::Browse) => {
                    crate::settings::show_page(ctx, crate::settings::Page::Appearance);
                    self.open_settings();
                }
                Event::Move(button, place) => {
                    let layout = self.settings.button_layout.normalized();
                    self.settings.button_layout = layout.moved(button, place, usize::MAX);
                    self.settings.set(ctx);
                }
            }
        }
    }

    /// Keyboard shortcuts, and the reload on coming back to the window. Returns whether the
    /// window has the focus.
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) -> bool {
        keymap::owner(
            ctx,
            egui::Id::new((
                self.active,
                self.tabs.get(self.active).and_then(|t| {
                    if let Tab::Repo(r) = t {
                        Some(&r.path)
                    } else {
                        None
                    }
                }),
            )),
        );
        keymap::active(
            ctx,
            match self.tabs.get(self.active) {
                Some(Tab::Repo(r)) => r.key_scopes(ctx),
                Some(Tab::Settings) => vec![keymap::Scope::Window, keymap::Scope::Settings],
                _ => vec![keymap::Scope::Window],
            },
        );
        // A focused text field or the terminal keeps these keys for itself.
        let modal_open = self.window_has_dialog()
            || self.askpass_open()
            || self.palette.is_some()
            || self.practice_open
            || self.setup.open
            || self.manual_open
            || self.repo_scan.is_some()
            || (crate::a11y::keyboard_taken(ctx) && !home_filter::search_has_focus(ctx));
        let focused =
            ctx.input(|i| i.focused || i.raw.viewports.values().any(|v| v.focused == Some(true)));
        let capturing = keymap::editor::capturing(ctx);
        let allowed = if capturing {
            Some(Vec::new())
        } else if modal_open {
            let mut allowed = vec![keymap::Command::Manual, keymap::Command::Palette];
            if self.palette.is_none()
                && !self.manual_open
                && self.clone_form.is_none()
                && self.remote_form.is_none()
                && !self.askpass_open()
                && let Some(Tab::Repo(r)) = self.tabs.get(self.active)
                && r.composer_has_focus(ctx)
            {
                allowed.push(keymap::Command::Commit);
            }
            if self.palette.is_none()
                && !self.manual_open
                && !self.window_has_dialog()
                && !self.askpass_open()
                && let Some(Tab::Repo(repo)) = self.tabs.get(self.active)
                && repo.search_shortcut_exception(ctx)
            {
                allowed.push(keymap::Command::Search);
                if repo.find_shortcut_exception(ctx) {
                    allowed.extend([keymap::Command::FindNext, keymap::Command::FindPrevious]);
                }
            }
            if self.palette.is_none()
                && !self.manual_open
                && !self.window_has_dialog()
                && !self.askpass_open()
                && matches!(self.tabs.get(self.active), Some(Tab::Settings))
                && Settings::search_has_focus(ctx)
            {
                allowed.extend([
                    keymap::Command::SettingsSearch,
                    keymap::Command::SettingsPrevious,
                    keymap::Command::SettingsNext,
                ]);
            }
            Some(allowed)
        } else {
            None
        };
        keymap::restrict(ctx, allowed);
        let commands = if capturing {
            keymap::cancel(ctx, keymap::Scope::Window);
            Vec::new()
        } else if modal_open {
            keymap::pressed(ctx, keymap::Scope::Window)
                .into_iter()
                .filter(|c| matches!(c, keymap::Command::Manual | keymap::Command::Palette))
                .collect()
        } else {
            keymap::pressed(ctx, keymap::Scope::Window)
        };
        let mut refresh = false;
        for command in commands {
            refresh |= self.window_command(command, modal_open, ctx);
        }
        // Coming back to the window (e.g. after using git elsewhere) refreshes the open repo.
        let regained = self.focus.regained(focused);
        if let Some(Tab::Repo(r)) = self.tabs.get_mut(self.active)
            && (refresh || regained.is_some_and(|initial| !initial || !r.just_read()))
        {
            r.refresh(ctx);
        }
        focused
    }

    /// Does what the key of `command` does; `modal_open`: a dialog, the palette, the manual or a
    /// text field has the keyboard. Returns whether the open repository is to be read again.
    pub(super) fn window_command(
        &mut self,
        command: keymap::Command,
        modal_open: bool,
        ctx: &egui::Context,
    ) -> bool {
        use keymap::Command;
        // Not over a dialog that asks something (the clone, the remote one, ssh's question).
        let asking = self.window_has_dialog() || self.askpass_open();
        match command {
            Command::Manual if !asking && self.palette.is_none() => {
                self.manual_open = !self.manual_open;
            }
            Command::Palette if !asking => {
                if self.palette.is_some() {
                    self.palette = None;
                } else {
                    self.open_palette();
                }
            }
            Command::Manual | Command::Palette => {}
            _ if modal_open => {}
            Command::Settings => self.open_settings(),
            Command::NewTab => self.new_tab(),
            Command::ReopenTab => self.reopen_closed_tab(ctx),
            Command::CloseTab => self.close_tab(self.active, ctx),
            Command::NextTab => self.step_tab(1),
            Command::PreviousTab => self.step_tab(-1),
            Command::GoToTab(place) => self.go_to_tab(usize::from(place)),
            Command::LastTab => self.go_to_last_tab(),
            Command::Reload => return true,
            // Contextual actions are delivered by their owning view.
            _ => {}
        }
        false
    }
}
