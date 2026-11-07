//! The start-page practice picker and the background setup handoff.

use std::sync::mpsc::{TryRecvError, channel};

use super::App;
use crate::practice::Scenario;

impl App {
    pub(super) fn practice_dialog(&mut self, ctx: &egui::Context) {
        if !self.practice_open {
            return;
        }
        let mut close = false;
        let mut start = None;
        let modal = egui::Modal::new(egui::Id::new("practice-picker"))
            .show(ctx, |ui| {
                ui.set_width(520.0_f32.min((ctx.content_rect().width() - 48.0).max(180.0)));
                ui.heading("Practice git");
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() * 0.7).max(120.0)).show(ui, |ui| {
                    ui.label("Try real git in a new local temporary folder. Your repositories are untouched. The practice folder may be removed by your system; copy it elsewhere if you want to keep it.");
                    ui.add_space(10.0);
                    for scenario in Scenario::ALL {
                        ui.group(|ui| {
                            if ui.add_enabled(self.practicing.is_none(), egui::Button::new(scenario.title())).clicked() {
                                start = Some(scenario);
                            }
                            ui.label(scenario.goal());
                        });
                    }
                });
                if self.practicing.is_some() {
                    ui.horizontal(|ui| {
                        ui.add(crate::spinner::Spinner::new());
                        ui.label("Making a practice repository…");
                    });
                }
                close = ui.button("Close").clicked();
            });
        if close || modal.should_close() {
            self.practice_open = false;
        }
        if let Some(scenario) = start {
            self.start_practice(scenario, ctx);
        }
    }

    pub(super) fn start_practice(&mut self, scenario: Scenario, ctx: &egui::Context) {
        if self.practicing.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::practice::create(scenario));
            ctx.request_repaint();
        });
        self.practicing = Some(rx);
    }

    pub(super) fn poll_practice(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.practicing else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("Making the practice repository stopped unexpectedly. Try again.".to_owned())
            }
        };
        self.practicing = None;
        match result {
            Ok(path) => {
                self.practice_open = false;
                // The worker already made and validated the repository root. No git on UI.
                self.open_root(path, ctx);
            }
            Err(error) => {
                self.practice_open = false;
                self.report(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable as _;
    use std::time::{Duration, Instant};

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
            let _ = harness.get_by_label(scenario.title());
        }
        harness.get_by_label(Scenario::Detached.title()).click();
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
        drop(harness);
        std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
