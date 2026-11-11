//! Tab identity, title derivation, and action application.

use super::*;

impl App {
    /// Each tab's title and width.
    pub(super) fn titles(&self, ui: &Ui, t: &Tokens) -> Vec<Title> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let active = i == self.active;
                let (title, chip, background): (String, Option<Chip>, usize) = match tab {
                    Tab::Home => (
                        crate::i18n::text("interaction-tabs-new-tab").to_owned(),
                        None,
                        0,
                    ),
                    Tab::Repo(r) => {
                        let label = r.label(self.settings.worktrees_in_same_tab);
                        let chip = worktree_tab::chip_for(ui, &label, MAX_CHIP_W);
                        (label.repo, chip, r.background_work().len())
                    }
                    Tab::Settings => (
                        crate::i18n::text("interaction-tabs-settings").to_owned(),
                        None,
                        0,
                    ),
                };
                let font = if active {
                    theme::medium(12.5)
                } else {
                    egui::FontId::proportional(12.5)
                };
                let color = if active { t.text } else { t.text_dim };
                let galley = ui.painter().layout_no_wrap(title, font, color);
                let extras = chip.as_ref().map_or(0.0, |c| c.width + 6.0)
                    + if background > 0 { BADGE_W } else { 0.0 };
                let width = (galley.size().x + 66.0 + extras).clamp(MIN_TAB_W, MAX_TAB_W);
                Title {
                    galley,
                    color,
                    width,
                    chip,
                    background,
                }
            })
            .collect()
    }

    /// Ids that stay with the tabs as they move.
    pub(super) fn tab_ids(&self) -> Vec<egui::Id> {
        let mut homes = 0;
        self.tabs
            .iter()
            .map(|tab| match tab {
                Tab::Repo(r) => egui::Id::new(("tab", &r.path)),
                Tab::Home => {
                    homes += 1;
                    egui::Id::new(("tab-home", homes))
                }
                Tab::Settings => egui::Id::new("tab-settings"),
            })
            .collect()
    }

    /// The title bar's buttons, then the profile in use; `folded`, the buttons are in the
    /// profile's menu instead.
    pub(super) fn bar_buttons(&mut self, ui: &mut Ui, t: &Tokens, folded: bool) {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        let fixed = self.buttons_elsewhere();
        if !folded {
            app_buttons::area(ui, Place::TitleBar, Look::Bar, Flow::RightToLeft, &fixed);
        }
        let in_menu = if folded {
            self.title_bar_buttons()
        } else {
            Vec::new()
        };
        self.profile_button(ui, t, &in_menu);
    }

    /// Every button the title bar shows: its own, and the others' on a tab with no side panels.
    pub(super) fn title_bar_buttons(&self) -> Vec<AppButton> {
        let layout = self.settings.button_layout.normalized();
        let mut buttons = layout.title_bar.clone();
        buttons.extend(self.buttons_elsewhere());
        buttons
    }

    /// The buttons placed in the side panels' footers when the active tab has none (the start
    /// page, the settings, a repository still opening): the title bar shows them too.
    pub(super) fn buttons_elsewhere(&self) -> Vec<AppButton> {
        let panels = matches!(self.tabs.get(self.active), Some(Tab::Repo(r)) if !r.opening());
        if panels {
            return Vec::new();
        }
        let layout = self.settings.button_layout.normalized();
        [Place::BottomLeft, Place::BottomRight]
            .into_iter()
            .flat_map(|p| layout.at(p).to_vec())
            .collect()
    }

    pub(super) fn apply_strip_actions(&mut self, actions: StripActions, ctx: &egui::Context) {
        if actions.quit {
            self.quit(ctx);
            return;
        }
        if let Some((index, target, position)) = actions.window
            && actions.close.is_none()
        {
            self.move_to_window(index, target, position, ctx);
            return;
        }
        if let Some(i) = actions.select {
            self.active = i;
        }
        // A tab closed as another is dropped: the indices are from before either, so the drop
        // is left out rather than moving the wrong tab or closing one that moved.
        if let Some((from, to)) = actions.moved
            && actions.close.is_none()
        {
            self.move_tab(from, to);
        }
        if let Some((path, look)) = actions.restyle {
            if look == TabLook::default() {
                self.tab_looks.remove(&path);
            } else {
                self.tab_looks.insert(path, look);
            }
        }
        if let Some(i) = actions.close {
            self.close_tab(i, ctx);
        }
        if actions.add {
            self.new_tab();
        }
        if actions.reopen {
            self.reopen_closed_tab(ctx);
        }
        if let Some(source) = actions.teleport {
            self.start_teleport(source, ctx);
        }
        if let Some((source, machine)) = actions.teleport_to {
            self.prepare_teleport_drop(source, machine, ctx);
        }
        // Opened and closed after the strip is drawn.
        if actions.switch_to.is_some() {
            self.switch_to = actions.switch_to;
        }
        if actions.close_worktree {
            self.close_worktree = true;
        }
    }
}
