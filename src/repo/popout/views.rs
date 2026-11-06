//! Drawing the views of a repository tab that are popped out, and the buttons that pop them
//! out and bring them back.

use egui::{Key, Ui, ViewportClass};

use super::{Panel, Request, title, viewport_id, window};
use crate::icons;
use crate::repo::{Action, RepoTab, pr_review};

impl RepoTab {
    /// Draws the windows of the views popped out of this tab, and carries out what is done in
    /// them. Called every frame for every repository tab, the one shown or not, so switching
    /// tabs leaves the windows open; a tab closed stops calling it, which closes its windows.
    pub fn popouts_ui(&mut self, ctx: &egui::Context) {
        let open = self.open_views();
        self.popouts.forget_closed(|p| open.contains(&p));
        let popped: Vec<Panel> = self.popouts.popped().collect();
        let mut acts = Vec::new();
        for panel in popped {
            let id = viewport_id(&self.path, panel);
            let title = title(&self.name, &self.popout_subject(panel));
            let scope = egui::Id::new(("popout", &self.path, panel));
            let closed = window::show(ctx, id, &title, |ui, class| {
                ui.push_id(scope, |ui| self.popped_view(ui, panel, class, &mut acts));
            });
            if closed {
                self.popouts.dock(panel);
            }
        }
        for action in acts {
            self.apply(action, ctx);
        }
    }

    /// Whether `panel` is drawn in the main window (it isn't popped out).
    pub(in crate::repo) fn docked(&self, panel: Panel) -> bool {
        !self.popouts.is_popped(panel)
    }

    /// The views open now, popped out or not.
    fn open_views(&self) -> Vec<Panel> {
        Panel::ALL
            .into_iter()
            .filter(|p| match p {
                Panel::Diff => self.diff.is_some(),
                Panel::Terminal => self.terminal_open,
                Panel::Pr => self.pr_view.is_some(),
                Panel::Ci => self.ci_view.is_some(),
            })
            .collect()
    }

    /// What the window of `panel` shows, for its title.
    fn popout_subject(&self, panel: Panel) -> String {
        match panel {
            Panel::Diff => match &self.diff {
                Some(view) => format!("diff of {}", view.key.path()),
                None => "diff".to_owned(),
            },
            Panel::Terminal => match self.terminals.get(self.active_terminal) {
                Some(term) => format!("terminal ({})", term.shell),
                None => "terminal".to_owned(),
            },
            Panel::Pr => match &self.pr_view {
                Some(view) => match &view.details {
                    Some(Ok(details)) => {
                        format!("pull request #{} {}", view.number, details.title)
                    }
                    _ => format!("pull request #{}", view.number),
                },
                None => "pull request".to_owned(),
            },
            Panel::Ci => match &self.ci_view {
                Some(view) => format!("CI of {}", crate::repo::short(&view.sha)),
                None => "CI".to_owned(),
            },
        }
    }

    /// The view of `panel` in its window, and the keys it takes there.
    fn popped_view(
        &mut self,
        ui: &mut Ui,
        panel: Panel,
        class: ViewportClass,
        acts: &mut Vec<Action>,
    ) {
        match panel {
            Panel::Diff => self.diff_view(ui, acts),
            Panel::Terminal => self.terminal_panel(ui, acts),
            Panel::Pr => self.pr_view_ui(ui, acts),
            Panel::Ci => self.ci_view_ui(ui, acts),
        }
        // Inside the main window, its keys are the main window's (see `RepoTab::shortcuts`).
        if class != ViewportClass::EmbeddedWindow {
            self.popout_keys(ui.ctx(), panel, acts);
        }
    }

    /// Esc closes the view in its window as in the main one; in a diff, F7 / Shift+F7 (or
    /// Alt+↓ / Alt+↑) go to the next and previous change. The terminal keeps every key.
    fn popout_keys(&self, ctx: &egui::Context, panel: Panel, acts: &mut Vec<Action>) {
        if panel == Panel::Terminal || self.dialog.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        match panel {
            Panel::Diff => {
                if esc && self.line_comment_open() {
                    acts.push(Action::Pr(pr_review::PrAct::CloseLine));
                } else if esc {
                    acts.push(Action::CloseDiff);
                }
                if let Some(delta) = ctx.input(crate::repo::layout::change_step) {
                    acts.push(Action::DiffStep(delta));
                }
            }
            Panel::Pr if esc => acts.push(Action::ClosePr),
            Panel::Ci if esc => acts.push(Action::CloseCi),
            _ => {}
        }
    }

    /// Carries out a pop-out button or palette entry.
    pub(in crate::repo) fn popout_request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::PopOut(Panel::Terminal) => {
                if !self.terminal_open {
                    self.toggle_terminal(ctx);
                }
                self.popouts.pop_out(Panel::Terminal);
            }
            Request::PopOut(panel) => {
                if self.open_views().contains(&panel) {
                    self.popouts.pop_out(panel);
                } else {
                    self.status = Some((format!("There's no {} open.", panel.noun()), true));
                }
            }
            Request::Dock(panel) => {
                self.popouts.dock(panel);
            }
            Request::PopShown => match self.shown_poppable() {
                Some(panel) => {
                    self.popouts.pop_out(panel);
                }
                None => {
                    self.status = Some((
                        "Nothing to pop out: open a diff, a pull request or the CI view first."
                            .to_owned(),
                        true,
                    ));
                }
            },
        }
        ctx.request_repaint();
    }

    /// The view the big area shows that could pop out, as the big area picks it.
    fn shown_poppable(&self) -> Option<Panel> {
        let open = self.open_views();
        [Panel::Diff, Panel::Ci, Panel::Pr]
            .into_iter()
            .find(|p| open.contains(p) && self.docked(*p))
    }
}

/// The header button of `panel`'s view: Pop out in the main window, Bring back in its own.
pub(in crate::repo) fn button(ui: &mut Ui, panel: Panel, popped: bool, acts: &mut Vec<Action>) {
    let (icon, tip, request) = if popped {
        (
            "minimize-2",
            format!("Bring the {} back into the main window", panel.noun()),
            Request::Dock(panel),
        )
    } else {
        (
            "picture-in-picture-2",
            format!("Pop the {} out into its own window", panel.noun()),
            Request::PopOut(panel),
        )
    };
    let resp = icons::small_button(ui, icon, &tip);
    // Named for screen readers (and tests), as the icon says nothing to them.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
    if resp.clicked() {
        acts.push(Action::Popout(request));
    }
}
