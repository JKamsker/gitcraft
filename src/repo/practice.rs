//! A persistent goal banner: goal checks run on the same worker handoff as other tab reads.

use super::RepoTab;
use crate::theme::Tokens;

impl RepoTab {
    pub(super) fn practice_banner(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.practice.clone() else {
            return;
        };
        let mut check = false;
        let t = Tokens::get(ui.ctx());
        egui::Panel::top("practice-goal")
            .frame(egui::Frame::new().fill(t.card).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("PRACTICE · {}", session.scenario.title()));
                    ui.label("Local throwaway repository").on_hover_text(self.path.display().to_string());
                    check = ui.add_enabled(!self.practice_checking && self.busy_op.is_none() && !self.loading, egui::Button::new("Check goal")).clicked();
                });
                ui.label(session.scenario.goal());
                egui::CollapsingHeader::new("Hint").id_salt("practice-hint").show(ui, |ui| { ui.label(session.scenario.hint()); });
                if self.practice_checking {
                    ui.horizontal(|ui| { ui.add(crate::spinner::Spinner::new()); ui.label("Checking the repository…"); });
                } else if let Some(result) = &self.practice_result {
                    match result {
                        Ok(true) => { ui.colored_label(t.success, "Goal complete. You can keep experimenting or start another scenario from Practice."); }
                        Ok(false) => { ui.label("Not yet. Follow the goal above, then check again."); }
                        Err(error) => { ui.colored_label(t.danger, format!("Could not check the goal: {error}")); }
                    }
                }
            });
        if check {
            self.check_practice(ui.ctx());
        }
    }

    fn check_practice(&mut self, ctx: &egui::Context) {
        if self.practice_checking || self.busy_op.is_some() || self.loading {
            return;
        }
        let Some(session) = self.practice.clone() else {
            return;
        };
        self.practice_checking = true;
        let path = self.path.clone();
        let read = self.reads;
        let failed = |error: String| {
            super::Msg::Then(Box::new(move |tab, _| {
                tab.practice_checking = false;
                tab.practice_result = Some(Err(error));
            }))
        };
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            let result = session.check(&path);
            super::Msg::Then(Box::new(move |tab, _| {
                tab.practice_checking = false;
                if tab.practice.as_ref() == Some(&session)
                    && tab.reads == read
                    && tab.busy_op.is_none()
                {
                    tab.practice_result = Some(result);
                }
            }))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::Scenario;
    use std::time::{Duration, Instant};

    fn wait(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
        let until = Instant::now() + Duration::from_secs(15);
        while tab.loading || tab.practice_checking {
            if Instant::now() >= until {
                return Err("Practice tab did not settle".to_owned());
            }
            tab.poll(ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    // These checks exercise the tab's background goal jobs and explicit reloads.
    // Filesystem watching has separate tests; its startup recheck intentionally
    // invalidates a concurrent goal answer even when no fixture file changed.
    fn fixture_tab(path: std::path::PathBuf, ctx: &egui::Context) -> RepoTab {
        let mut tab = RepoTab::new(path, ctx);
        tab.watches += 1;
        tab.watcher = None;
        tab.fs_rx = None;
        tab.fs_pending = None;
        tab
    }

    #[test]
    fn a_reopened_tab_loads_and_checks_its_goal_in_the_background() -> Result<(), String> {
        let path = crate::practice::create(Scenario::Detached)?;
        let root = path.parent().ok_or("Practice root missing")?.to_owned();
        let ctx = egui::Context::default();
        let mut tab = fixture_tab(path.clone(), &ctx);
        wait(&mut tab, &ctx)?;
        assert_eq!(
            tab.practice.as_ref().map(|s| s.scenario),
            Some(Scenario::Detached)
        );
        tab.check_practice(&ctx);
        assert!(tab.practice_checking);
        wait(&mut tab, &ctx)?;
        assert_eq!(tab.practice_result, Some(Ok(false)));
        crate::git::practice_run(&path, &["checkout", "-b", "rescued"])?;
        tab.reload(&ctx);
        wait(&mut tab, &ctx)?;
        assert!(
            tab.practice_result.is_none(),
            "A reload clears the old result"
        );
        tab.check_practice(&ctx);
        wait(&mut tab, &ctx)?;
        assert_eq!(tab.practice_result, Some(Ok(true)));
        // The banner is drawn by the same UI entry point used in the window.
        let mut output = ctx.run_ui(Default::default(), |ui| tab.practice_banner(ui));
        output.textures_delta.clear();
        drop(tab);
        std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn an_answer_from_before_a_reload_is_discarded() -> Result<(), String> {
        let path = crate::practice::create(Scenario::Detached)?;
        let root = path.parent().ok_or("Practice root missing")?.to_owned();
        let ctx = egui::Context::default();
        let mut tab = fixture_tab(path, &ctx);
        wait(&mut tab, &ctx)?;
        tab.check_practice(&ctx);
        // A newer read starts before the check's answer is consumed.
        tab.reload(&ctx);
        wait(&mut tab, &ctx)?;
        assert!(tab.practice_result.is_none());
        drop(tab);
        std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
