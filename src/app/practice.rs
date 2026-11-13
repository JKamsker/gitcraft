//! The start-page practice picker and the background setup handoff.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::App;
use crate::practice::Scenario;

pub(super) struct Practicing {
    owner: egui::ViewportId,
    rx: Receiver<Result<std::path::PathBuf, String>>,
}

impl App {
    pub(super) fn practice_dialog(&mut self, ctx: &egui::Context) {
        if !self.practice_open {
            return;
        }
        let action = practice_picker(ctx, self.practicing.is_some());
        if action.close {
            self.practice_open = false;
        }
        if let Some(scenario) = action.start {
            self.start_practice(scenario, ctx);
        }
    }

    pub(super) fn start_practice(&mut self, scenario: Scenario, ctx: &egui::Context) {
        if self.practicing.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let queued = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                self.practice_jobs.0.child(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let _ = tx.send(crate::practice::create(scenario));
                    ctx.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        if let Err(error) = queued {
            self.report(crate::practice::display_error(&error));
            return;
        }
        self.practicing = Some(Practicing {
            owner: self.windows.current,
            rx,
        });
    }

    pub(super) fn poll_practice(&mut self, ctx: &egui::Context) {
        let Some(practicing) = &self.practicing else {
            return;
        };
        let result = match practicing.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("Making the practice repository stopped unexpectedly. Try again.".to_owned())
            }
        };
        let owner = practicing.owner;
        self.practicing = None;
        self.in_window(owner, |app| match result {
            Ok(path) => {
                app.practice_open = false;
                // The worker already made and validated the repository root. No git on UI.
                // A practice repository's folder (in the temporary folder) isn't looked in.
                app.open_root_as(path, ctx, false);
            }
            Err(error) => {
                app.practice_open = false;
                app.report(crate::practice::display_error(&error));
            }
        });
    }

    #[cfg(test)]
    pub(super) fn recorded_practice_completion(
        &mut self,
        owner: egui::ViewportId,
        result: Result<std::path::PathBuf, String>,
    ) {
        let (tx, rx) = channel();
        let _ = tx.send(result);
        self.practicing = Some(Practicing { owner, rx });
    }
}

/// Inert drawing adapter shared by the application and headless tests. Clicking a scenario
/// returns a selection; only the App handoff above is allowed to enqueue repository work.
#[derive(Default)]
struct PickerAction {
    close: bool,
    start: Option<Scenario>,
}

fn practice_picker(ctx: &egui::Context, making: bool) -> PickerAction {
    let mut action = PickerAction::default();
    let modal = egui::Modal::new(egui::Id::new("practice-picker")).show(ctx, |ui| {
        ui.set_width(520.0_f32.min((ctx.content_rect().width() - 48.0).max(180.0)));
        ui.heading(crate::i18n::text("interaction-practice-picker-title"));
        egui::ScrollArea::vertical()
            .max_height((ctx.content_rect().height() * 0.7).max(120.0))
            .show(ui, |ui| {
                ui.label(crate::i18n::text("interaction-practice-picker-description"));
                ui.add_space(10.0);
                for scenario in Scenario::ALL {
                    ui.group(|ui| {
                        if ui
                            .add_enabled(!making, egui::Button::new(scenario.display_title()))
                            .clicked()
                        {
                            action.start = Some(scenario);
                        }
                        ui.label(scenario.display_goal());
                    });
                }
            });
        if making {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(crate::i18n::text("interaction-practice-making"));
            });
        }
        action.close = ui
            .button(crate::i18n::text("interaction-practice-close"))
            .clicked();
    });
    action.close |= modal.should_close();
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable as _;
    use std::time::{Duration, Instant};

    #[test]
    fn fluent_picker_draws_and_selects_without_app_jobs_or_repositories() {
        crate::i18n::set_language("en-US");
        let mut h = egui_kittest::Harness::builder()
            .with_size([900.0, 1500.0])
            .build_ui_state(
                |ui, state: &mut (bool, Option<Scenario>, bool)| {
                    let action = practice_picker(ui.ctx(), state.0);
                    if action.start.is_some() {
                        state.1 = action.start;
                    }
                    state.2 |= action.close;
                },
                (false, None, false),
            );
        h.run_steps(3);
        h.get_by_label("Practice git");
        for scenario in Scenario::ALL {
            h.get_by_label(scenario.display_title());
            h.get_by_label(scenario.display_goal());
        }
        h.get_by_label("Rescue a detached HEAD").click();
        h.run_steps(3);
        assert_eq!(h.state().1, Some(Scenario::Detached));
        crate::i18n::set_language("de");
        h.run_steps(3);
        h.get_by_label("Git üben");
        h.get_by_label("Einen losgelösten HEAD retten");
        h.get_by_label(Scenario::Published.display_goal());
        h.get_by_label("Schließen").click();
        h.run_steps(3);
        assert!(h.state().2);
        h.state_mut().0 = true;
        h.state_mut().1 = None;
        h.run_steps(3);
        h.get_by_label("Übungsrepository wird erstellt…");
        // A disabled scenario can be queried but cannot enqueue or select any work.
        h.get_by_label("Einen Merge-Konflikt lösen").click();
        h.run_steps(3);
        assert_eq!(h.state().1, None);
        crate::i18n::set_language("en-US");
        h.run_steps(3);
        h.get_by_label("Making a practice repository…");
        h.get_by_label("Close");
    }

    #[test]
    fn the_start_page_picker_creates_and_opens_the_selected_scenario() -> Result<(), String> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size([1200.0, 900.0])
            .build_eframe(|cc| {
                let mut app = App::new(cc, None);
                app.settings.check_updates = false;
                app.settings.set(&cc.egui_ctx);
                app
            });
        harness.get_by_label("Practice git…").click();
        harness.run_steps(3);
        assert!(harness.state().practice_open);
        for scenario in Scenario::ALL {
            let _ = harness.get_by_label(scenario.display_title());
        }
        harness
            .get_by_label(Scenario::Detached.display_title())
            .click();
        harness.run_steps(3);
        let until = Instant::now() + Duration::from_secs(15);
        while harness.state().practicing.is_some() {
            if Instant::now() >= until {
                return Err("Practice creation did not finish".to_owned());
            }
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!harness.state().practice_open);
        let path = match harness.state().tabs.get(harness.state().active) {
            Some(super::super::Tab::Repo(tab)) => tab.path.clone(),
            _ => return Err("Practice did not open its repository tab".to_owned()),
        };
        assert_eq!(
            crate::practice::load(&path).map(|s| s.scenario),
            Some(Scenario::Detached)
        );
        let root = path.parent().ok_or("Practice root missing")?.to_owned();
        // The tab opened in the frame the practice repository arrived: its first read is still
        // running `git -C` there. Let it finish, as the user sees it, before closing the tab.
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            match harness.state().tabs.get(harness.state().active) {
                Some(super::super::Tab::Repo(tab)) if !tab.opening() && !tab.is_busy() => break,
                Some(super::super::Tab::Repo(_)) => {}
                _ => return Err("The practice tab went away".to_owned()),
            }
            if Instant::now() >= until {
                return Err("The practice tab did not finish reading".to_owned());
            }
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        let active = harness.state().active;
        let tab = std::mem::replace(
            &mut harness.state_mut().tabs[active],
            super::super::Tab::Home,
        );
        let super::super::Tab::Repo(tab) = tab else {
            return Err("Practice did not keep its repository tab".to_owned());
        };
        // Dropping the app only cancels what the tab started. Windows refuses to delete a
        // folder a `git` still running there holds (os error 32), so wait until all of it ends.
        (*tab).close_and_wait(Duration::from_secs(30))?;
        drop(harness);
        std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
