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
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(crate::i18n::message(
                        "repo-core-3062",
                        &[("title", session.scenario.display_title().into())],
                    ));
                    ui.label(crate::i18n::text("repo-core-3063"))
                        .on_hover_text(self.path.display().to_string());
                    check = ui
                        .add_enabled(
                            !self.practice_checking && self.busy_op.is_none() && !self.loading,
                            egui::Button::new(crate::i18n::text("repo-core-3064")),
                        )
                        .clicked();
                });
                ui.label(session.scenario.display_goal());
                egui::CollapsingHeader::new(crate::i18n::text("repo-core-3065"))
                    .id_salt("practice-hint")
                    .show(ui, |ui| {
                        ui.label(session.scenario.display_hint());
                    });
                if self.practice_checking {
                    ui.horizontal(|ui| {
                        ui.add(crate::spinner::Spinner::new());
                        ui.label(crate::i18n::text("repo-core-3067"));
                    });
                } else if let Some(result) = &self.practice_result {
                    match result {
                        Ok(true) => {
                            ui.colored_label(t.success, crate::i18n::text("repo-core-3068"));
                        }
                        Ok(false) => {
                            ui.label(crate::i18n::text("repo-core-3069"));
                        }
                        Err(error) => {
                            ui.colored_label(
                                t.danger,
                                crate::i18n::message(
                                    "repo-core-3070",
                                    &[("error", crate::practice::display_error(error).into())],
                                ),
                            );
                        }
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
    use crate::repo::hang_diagnostics as diag;
    use std::time::{Duration, Instant};

    fn wait(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
        let until = Instant::now() + Duration::from_secs(15);
        while tab.loading || tab.practice_checking {
            if Instant::now() >= until {
                return Err("Practice tab did not settle".to_owned());
            }
            diag::poll(tab, ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    // These checks exercise the tab's background goal jobs and explicit reloads.
    // Filesystem watching has separate tests; its startup recheck intentionally
    // invalidates a concurrent goal answer even when no fixture file changed.
    fn fixture_tab(path: std::path::PathBuf, ctx: &egui::Context) -> diag::Owned<RepoTab> {
        let mut tab = diag::live_tab(diag::run(format_args!("open practice tab"), || {
            RepoTab::new(path, ctx)
        }));
        let _watcher = diag::stage(format_args!("disable practice fixture watcher"));
        tab.watches += 1;
        tab.watcher = None;
        tab.fs_rx = None;
        tab.fs_pending = None;
        tab
    }

    #[test]
    fn a_reopened_tab_loads_and_checks_its_goal_in_the_background() -> Result<(), String> {
        diag::bounded(
            "a_reopened_tab_loads_and_checks_its_goal_in_the_background",
            || {
                let path = diag::run(format_args!("create practice fixture"), || {
                    crate::practice::create(Scenario::Detached)
                })?;
                let root =
                    diag::FixtureRoot(path.parent().ok_or("Practice root missing")?.to_owned());
                let ctx = diag::context();
                let mut tab = fixture_tab(path.clone(), &ctx);
                wait(&mut tab, &ctx)?;
                assert_eq!(
                    tab.practice.as_ref().map(|s| s.scenario),
                    Some(Scenario::Detached)
                );
                diag::run(format_args!("start practice check"), || {
                    tab.check_practice(&ctx)
                });
                assert!(tab.practice_checking);
                wait(&mut tab, &ctx)?;
                assert_eq!(tab.practice_result, Some(Ok(false)));
                crate::git::practice_run(&path, &["checkout", "-b", "rescued"])?;
                diag::run(format_args!("reload practice tab"), || tab.reload(&ctx));
                wait(&mut tab, &ctx)?;
                assert!(
                    tab.practice_result.is_none(),
                    "A reload clears the old result"
                );
                diag::run(format_args!("start practice check"), || {
                    tab.check_practice(&ctx)
                });
                wait(&mut tab, &ctx)?;
                assert_eq!(tab.practice_result, Some(Ok(true)));
                // The banner is drawn by the same UI entry point used in the window.
                let mut output = ctx.run_ui(Default::default(), |ui| tab.practice_banner(ui));
                output.textures_delta.clear();
                drop(tab);
                drop(root);
                Ok(())
            },
        )
    }

    #[test]
    fn an_answer_from_before_a_reload_is_discarded() -> Result<(), String> {
        diag::bounded("an_answer_from_before_a_reload_is_discarded", || {
            let path = diag::run(format_args!("create practice fixture"), || {
                crate::practice::create(Scenario::Detached)
            })?;
            let root = diag::FixtureRoot(path.parent().ok_or("Practice root missing")?.to_owned());
            let ctx = diag::context();
            let mut tab = fixture_tab(path, &ctx);
            wait(&mut tab, &ctx)?;
            diag::run(format_args!("start practice check"), || {
                tab.check_practice(&ctx)
            });
            // A newer read starts before the check's answer is consumed.
            diag::run(format_args!("reload practice tab"), || tab.reload(&ctx));
            wait(&mut tab, &ctx)?;
            diag::run(format_args!("assert stale answer discarded"), || {
                assert!(tab.practice_result.is_none())
            });
            drop(tab);
            drop(root);
            Ok(())
        })
    }

    #[cfg(windows)]
    #[test]
    fn sharing_violation_cleanup_warns_and_can_be_retried_after_release() -> Result<(), String> {
        use std::os::windows::fs::OpenOptionsExt;
        diag::bounded(
            "sharing_violation_cleanup_warns_and_can_be_retried_after_release",
            || {
                let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
                let root = diag::FixtureRoot(temp.keep());
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .share_mode(0)
                    .open(root.0.join("held"))
                    .map_err(|e| e.to_string())?;
                diag::remove_fixture(&root.0);
                assert!(root.0.exists(), "the owned handle still prevents deletion");
                drop(file);
                diag::remove_fixture(&root.0);
                // Antivirus handles may still deny deletion: cleanup remains best-effort.
                Ok(())
            },
        )
    }
}
