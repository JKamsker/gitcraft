//! Shared inspection and button click for the existing rebase confirmation fixtures.

use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;

use super::preview::PreviewState;
use super::{Dialog, Outcome, confirm};
use crate::git;
use crate::repo::{Action, RepoTab, Undo, operations, ops};

impl RepoTab {
    /// Wait for the actual preview and check that it describes the requested, unchanged branch.
    pub(crate) fn review_rebase_for_test(
        &mut self,
        ctx: &egui::Context,
        branch: &str,
        head: &str,
        onto: &str,
    ) -> Result<(), String> {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            self.poll(ctx);
            match &self.dialog {
                Some(Dialog::Confirm { preview, .. }) => match &preview.state {
                    PreviewState::Ready(_) => break,
                    PreviewState::Failed(error) => {
                        return Err(format!("rebase preview failed: {error}"));
                    }
                    _ => {}
                },
                None => {}
                _ => return Err("expected a rebase confirmation".into()),
            }
            assert!(
                Instant::now() < until,
                "no rebase preview: {:?}",
                self.status
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let Some(Dialog::Confirm {
            ops: steps,
            preview,
            undo,
            ..
        }) = &self.dialog
        else {
            unreachable!();
        };
        assert_eq!(
            preview.ask,
            git::PreviewAsk::rebase(branch, head, onto, self.upstream_ref(branch))
        );
        assert!(matches!(undo, Undo::HeadMove { mode: "--keep" }));
        if preview.instead.is_none() {
            assert_eq!(
                steps,
                &vec![
                    operations::expect_symref("HEAD", &format!("refs/heads/{branch}")),
                    operations::expect_ref_at("HEAD", head),
                    ops(&[&["rebase", "--end-of-options", onto]]).remove(0),
                ]
            );
        }
        assert!(self.busy_op.is_none());
        assert_eq!(git::resolve(&self.path, "HEAD").as_deref(), Ok(head));
        assert_eq!(
            git::run(Some(&self.path), &["symbolic-ref", "--short", "HEAD"])?.trim(),
            branch
        );
        assert!(git::status(&self.path)?.in_progress.is_none());
        Ok(())
    }

    /// Click the production confirmation button and return exactly the action it emits.
    pub(crate) fn accept_rebase_for_test(
        &mut self,
        ctx: &egui::Context,
        branch: &str,
        head: &str,
        onto: &str,
    ) -> Result<Action, String> {
        self.review_rebase_for_test(ctx, branch, head, onto)?;
        let Some(Dialog::Confirm {
            title,
            body,
            button,
            ops,
            alternative,
            mut use_alternative,
            refresh,
            undo,
            mut preview,
            safe,
        }) = self.dialog.take()
        else {
            unreachable!();
        };
        let mut accepted = None;
        let mut harness = egui_kittest::Harness::builder()
            .with_size([800.0, 1000.0])
            .build_ui(|ui| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let outcome = confirm::confirm(
                    ui,
                    confirm::Asked {
                        title: &title,
                        body: &body,
                        button: &button,
                        ops: &ops,
                        alternative: &alternative,
                        refresh,
                        undo: &undo,
                    },
                    &mut use_alternative,
                    &mut preview,
                    safe,
                );
                if let Outcome::Done(action) = outcome {
                    assert!(accepted.is_none(), "confirmation emitted twice");
                    accepted = Some(*action);
                }
            });
        harness.run();
        harness.get_by_label(&button).click();
        harness.run();
        drop(harness);
        accepted.ok_or_else(|| "the reviewed rebase button emitted no action".into())
    }
}
