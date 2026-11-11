//! Drawing the views of a repository tab that are popped out, the buttons that pop them out
//! and bring them back, and what the tab's actions do to them.

mod commands;

use crate::repo::terminals::TerminalAct;
use egui::{Key, RichText, Ui, ViewportClass};

use super::{Panel, Request, title, viewport_id, window};
use crate::forge::Forge;
use crate::icons;
use crate::repo::{Action, RepoTab, pr_review};
use crate::theme::Tokens;

impl RepoTab {
    #[cfg(test)]
    pub(crate) fn set_test_popout(&mut self, panel: Panel) {
        self.popouts.pop_out(panel);
    }

    /// Immutable declarations retained while a native nested event loop prevents
    /// the coordinator from borrowing this session to draw its immediate panels.
    pub(crate) fn popout_window_specs(&self) -> Vec<(egui::ViewportId, egui::ViewportBuilder)> {
        self.popouts
            .popped()
            .map(|panel| {
                (
                    viewport_id(&self.path, panel),
                    window::builder(&title(&self.name, &self.popout_subject(panel))),
                )
            })
            .collect()
    }

    /// Draws the windows of the views popped out of this tab, and carries out what is done in
    /// them. Called every frame for every repository tab after the tab shown is drawn (`shown`:
    /// this is that one), so switching tabs leaves the windows open; a tab closed stops calling
    /// it, which closes its windows.
    pub fn popouts_ui(&mut self, ctx: &egui::Context, shown: bool) {
        if !self.popouts.any() {
            return;
        }
        let mut acts = Vec::new();
        if shown {
            self.popped_hint(ctx, &mut acts);
        }
        let mut draw = self.popouts.take_drawn();
        if draw.len() < self.popouts.popped().count() {
            // Popped out this frame: its window opens on the next.
            ctx.request_repaint();
        }
        // Inside the main window, another tab's would float over the page shown.
        if ctx.embed_viewports() && !shown {
            draw.clear();
        }
        for panel in draw {
            let id = viewport_id(&self.path, panel);
            let title = title(&self.name, &self.popout_subject(panel));
            let scope = egui::Id::new(("popout", &self.path, panel));
            let closed = window::show(ctx, id, &title, |ui, class| {
                ui.push_id(scope, |ui| self.popped_view(ui, panel, class, &mut acts));
            });
            if closed {
                // Drawn in the main window from the next frame, which this one already drew.
                self.popouts.dock(panel);
                ctx.request_repaint();
            }
        }
        if !acts.is_empty() {
            ctx.request_repaint();
        }
        for action in acts {
            self.apply(action, ctx);
        }
    }

    /// Whether `panel` is drawn in the main window (it isn't popped out).
    pub(in crate::repo) fn docked(&self, panel: Panel) -> bool {
        !self.popouts.is_popped(panel)
    }

    /// Whether the main window's keys (Esc, F7) act on `panel`: it is drawn there, or its window
    /// is inside the main one, whose keys are the only ones.
    pub(in crate::repo) fn main_keys(&self, ctx: &egui::Context, panel: Panel) -> bool {
        self.docked(panel) || ctx.embed_viewports()
    }

    /// Whether the view of `panel` is open, popped out or not.
    fn view_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Diff => self.diff.is_some(),
            Panel::Terminal => self.terminals.is_open(),
            Panel::Pr => self.pr_view.is_some(),
            Panel::Ci => self.ci_view.is_some(),
        }
    }

    /// What the window of `panel` shows, for its title.
    fn popout_subject(&self, panel: Panel) -> String {
        match panel {
            Panel::Diff => match &self.diff {
                Some(view) => crate::i18n::message(
                    "repo-core-4215",
                    &[("path", view.key.path().to_string().into())],
                ),
                None => crate::i18n::text("repo-core-2367").to_owned(),
            },
            Panel::Terminal => match self.terminals.active() {
                Some(term) if self.terminals.is_open() => crate::i18n::message(
                    "repo-core-4216",
                    &[("shell", term.shell.to_string().into())],
                ),
                _ => crate::i18n::text("repo-core-3669").to_owned(),
            },
            Panel::Pr => match &self.pr_view {
                Some(view) => match &view.details {
                    Some(Ok(details)) => crate::i18n::message(
                        "repo-core-4217",
                        &[
                            ("number", format!("{}", view.number).into()),
                            ("title", details.title.to_string().into()),
                        ],
                    ),
                    _ => crate::i18n::message(
                        "repo-core-4218",
                        &[("number", format!("{}", view.number).into())],
                    ),
                },
                None => crate::i18n::text("repo-core-4219").to_owned(),
            },
            Panel::Ci => {
                // As the CI view's header says it.
                let noun = match self.forge() {
                    Some(Forge::GitLab) => crate::i18n::text("repo-core-4220"),
                    _ => crate::i18n::text("repo-core-4221"),
                };
                match &self.ci_view {
                    Some(view) => crate::i18n::message(
                        "repo-core-4222",
                        &[
                            ("noun", noun.to_string().into()),
                            ("sha", crate::repo::short(&view.sha).to_string().into()),
                        ],
                    ),
                    None => noun.to_owned(),
                }
            }
        }
    }

    /// The view of `panel` in its window, under the tab's latest message, and the keys it
    /// takes there.
    fn popped_view(
        &mut self,
        ui: &mut Ui,
        panel: Panel,
        class: ViewportClass,
        acts: &mut Vec<Action>,
    ) {
        if class != ViewportClass::EmbeddedWindow {
            self.popout_keys(ui.ctx(), panel, acts);
        }
        self.popout_status(ui, panel);
        if !self.view_open(panel) {
            placeholder(ui, panel, absent(panel), acts);
        } else if panel == Panel::Diff && self.conflict.is_some() {
            let why = crate::i18n::text("repo-core-4223");
            placeholder(ui, panel, why, acts);
        } else {
            match panel {
                Panel::Diff => self.diff_view(ui, acts),
                Panel::Terminal => self.terminal_panel(ui, acts),
                Panel::Pr => self.pr_view_ui(ui, acts),
                Panel::Ci => self.ci_view_ui(ui, acts),
            }
        }
    }

    /// The tab's latest message (an operation that failed, done from this window or not) at
    /// the foot of the window, where the toolbar showing it may be out of sight.
    fn popout_status(&self, ui: &mut Ui, panel: Panel) {
        let Some(notice) = &self.status else {
            return;
        };
        let (text, failed) = notice.raw();
        let text = notice.projected().unwrap_or_else(|| text.clone());
        let t = Tokens::get(ui.ctx());
        egui::Panel::bottom(egui::Id::new(("popout-status", &self.path, panel)))
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| {
                let color = if *failed { t.danger } else { t.text_dim };
                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).truncate());
            });
    }
}

/// What a pop-out window says while its view isn't open.
fn absent(panel: Panel) -> &'static str {
    match panel {
        Panel::Diff => crate::i18n::text("repo-core-4231"),
        Panel::Terminal => crate::i18n::text("repo-core-4232"),
        Panel::Pr => crate::i18n::text("repo-core-4233"),
        Panel::Ci => crate::i18n::text("repo-core-4234"),
    }
}

/// A window whose view isn't there: why, and the way out.
fn placeholder(ui: &mut Ui, panel: Panel, why: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 50.0).max(12.0));
        icons::inline(ui, "picture-in-picture-2", 32.0, t.text_faint);
        ui.add_space(10.0);
        ui.label(RichText::new(why).color(t.text_dim));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if panel == Panel::Terminal && ui.button(crate::i18n::text("repo-core-4235")).clicked()
            {
                acts.push(Action::Terminal(TerminalAct::Toggle));
            }
            if ui.button(crate::i18n::text("repo-core-4236")).clicked() {
                acts.push(Action::Popout(Request::Dock(panel)));
            }
        });
    });
}

/// The header button of `panel`'s view: Pop out in the main window, Bring back in its own.
pub(in crate::repo) fn button(ui: &mut Ui, panel: Panel, popped: bool, acts: &mut Vec<Action>) {
    let (icon, tip, request) = if popped {
        (
            "picture-in-picture",
            crate::i18n::message(
                "repo-core-4238",
                &[("noun", panel.noun().to_string().into())],
            ),
            Request::Dock(panel),
        )
    } else {
        (
            "picture-in-picture-2",
            crate::i18n::message(
                "repo-core-4239",
                &[("noun", panel.noun().to_string().into())],
            ),
            Request::PopOut(panel),
        )
    };
    let resp = icons::small_button(ui, icon, &tip);
    // Named for screen readers (and tests), as the icon says nothing to them.
    crate::a11y::a11y(&resp, egui::WidgetType::Button, &tip);
    if resp.clicked() {
        acts.push(Action::Popout(request));
    }
}

/// The close button's tooltip in a view's header: a popped-out view closes with its window.
pub(in crate::repo) fn close_tip(popped: bool, docked_tip: &'static str) -> &'static str {
    if popped {
        crate::i18n::text("repo-core-0711")
    } else {
        docked_tip
    }
}

#[cfg(test)]
#[path = "views/registry_tests.rs"]
mod registry_tests;
