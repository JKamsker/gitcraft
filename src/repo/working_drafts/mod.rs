//! Independent index/worktree snapshots, read and written on the shared task service.
pub(in crate::repo) mod view;
use super::saved_views::reader::Reader;
use super::{Dialog, RepoTab};
use crate::git::drafts as store;
use std::time::{Duration, Instant};
const PAUSE: Duration = Duration::from_secs(15);
#[derive(Default)]
pub(super) struct State {
    enabled: bool,
    loaded: bool,
    edited: Option<Instant>,
    reader: Reader<Read>,
    pub error: Option<String>,
    undo: Option<(store::Id, String)>,
}
enum Read {
    Config(bool),
    History(bool, Vec<store::Draft>),
    Preview(Box<store::RestorePreview>),
}
enum Change {
    Setting,
    Snapshot,
    Recovery(String),
}
enum Changed {
    Setting(bool),
    Snapshot,
    Recovery(store::Id),
}
impl State {
    pub fn observe(&mut self) {
        self.edited = Some(Instant::now());
    }
}
impl RepoTab {
    pub(super) fn poll_working_drafts(&mut self, ctx: &egui::Context) {
        self.working_drafts.reader.bind(self.tx.cancel.clone());
        let reading_dialog = matches!(self.dialog, Some(Dialog::WorkingDraftLoading));
        if !reading_dialog && self.working_drafts.loaded {
            self.working_drafts.reader.cancel();
        }
        if let Some(result) = self.working_drafts.reader.poll(ctx) {
            match result {
                Ok(Read::Config(on)) => {
                    self.working_drafts.loaded = true;
                    self.working_drafts.enabled = on;
                    self.working_drafts.observe();
                }
                Ok(Read::History(on, versions)) => {
                    self.working_drafts.loaded = true;
                    self.working_drafts.enabled = on;
                    self.dialog = Some(Dialog::WorkingDraftHistory {
                        on,
                        versions,
                        error: self.working_drafts.error.clone(),
                        index: false,
                    });
                }
                Ok(Read::Preview(preview)) => {
                    let selected = (0..preview.hunks.len()).collect();
                    self.dialog = Some(Dialog::WorkingDraftRecovery { preview, selected });
                }
                Err(e) => {
                    self.working_drafts.loaded = true;
                    self.working_drafts.error = Some(e.clone());
                    if reading_dialog {
                        self.dialog = None;
                        self.status = Some((format!("Earlier versions: {e}"), true));
                    }
                }
            }
        }
        if self.sleeping() || !self.shown || self.loading || self.busy_op.is_some() {
            return;
        }
        if !self.working_drafts.loaded && !self.working_drafts.reader.busy() {
            let path = self.path.clone();
            self.working_drafts
                .reader
                .queue(move |active| store::enabled_while(&path, active).map(Read::Config));
            self.working_drafts.reader.poll(ctx);
        } else if self.working_drafts.enabled
            && !self.working_drafts.reader.busy()
            && let Some(at) = self.working_drafts.edited
        {
            if at.elapsed() >= PAUSE {
                self.save_working_draft(true, ctx);
            } else {
                ctx.request_repaint_after(PAUSE.saturating_sub(at.elapsed()));
            }
        }
    }
    pub(super) fn earlier_versions(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        self.dialog = Some(Dialog::WorkingDraftLoading);
        self.working_drafts
            .reader
            .queue_mutation(self.tx.repo.clone(), move |active| {
                let on = store::enabled_while(&path, active)?;
                store::list(&path, active).map(|versions| Read::History(on, versions))
            });
        self.poll_working_drafts(ctx);
    }
    pub(super) fn preview_working_draft(
        &mut self,
        id: store::Id,
        path: String,
        index: bool,
        ctx: &egui::Context,
    ) {
        let repo = self.path.clone();
        self.dialog = Some(Dialog::WorkingDraftLoading);
        self.working_drafts.reader.queue(move |active| {
            store::preview(&repo, &id, &path, index, active).map(|p| Read::Preview(Box::new(p)))
        });
        self.poll_working_drafts(ctx);
    }
    pub(super) fn working_draft_undo_available(&self) -> bool {
        self.working_drafts.undo.is_some()
    }
    pub(super) fn undo_working_draft(&mut self, ctx: &egui::Context) {
        if let Some((id, path)) = self.working_drafts.undo.clone() {
            self.preview_working_draft(id, path, false, ctx);
        }
    }
    pub(super) fn enable_working_drafts(&mut self, on: bool, ctx: &egui::Context) {
        let path = self.path.clone();
        self.change_working_draft(Change::Setting, false, ctx, move |_| {
            store::set_enabled(&path, on)?;
            store::enabled(&path).map(Changed::Setting)
        });
    }
    pub(super) fn save_working_draft(&mut self, automatic: bool, ctx: &egui::Context) {
        let path = self.path.clone();
        self.change_working_draft(Change::Snapshot, automatic, ctx, move |active| {
            if store::enabled_while(&path, active)? {
                store::capture(
                    &path,
                    if automatic { "Pause" } else { "Saved manually" },
                    active,
                )?;
            }
            Ok(Changed::Snapshot)
        });
    }
    pub(super) fn recover_working_draft(
        &mut self,
        preview: store::RestorePreview,
        hunks: Vec<usize>,
        ctx: &egui::Context,
    ) {
        let path = self.path.clone();
        self.change_working_draft(
            Change::Recovery(preview.path.clone()),
            false,
            ctx,
            move |active| store::restore(&path, &preview, &hunks, active).map(Changed::Recovery),
        );
    }
    fn change_working_draft(
        &mut self,
        change: Change,
        automatic: bool,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Fn() -> bool) -> Result<Changed, String> + Send + 'static,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        self.working_drafts.edited = None;
        self.working_drafts.reader.cancel();
        self.busy_op = Some("Working draft".into());
        self.op_started = Some(Instant::now());
        if !automatic {
            self.dialog = None;
        }
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let token = crate::tasks::cancellation();
                work(&|| !token.is_cancelled())
            },
            move |tab, result, ctx| {
                tab.busy_op = None;
                tab.op_started = None;
                match result {
                    Ok(Changed::Setting(on)) => {
                        tab.working_drafts.enabled = on;
                        tab.working_drafts.loaded = true;
                        if on {
                            tab.working_drafts.observe();
                        }
                    }
                    Ok(Changed::Snapshot) => {}
                    Ok(Changed::Recovery(id)) => {
                        if let Change::Recovery(path) = change {
                            tab.working_drafts.undo = Some((id, path));
                        }
                        tab.reload_status(ctx);
                    }
                    Err(e) => {
                        tab.working_drafts.error = Some(e);
                        if matches!(change, Change::Setting) {
                            tab.working_drafts.loaded = false;
                        }
                    }
                }
                if !automatic {
                    tab.earlier_versions(ctx);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{OpKind, operations::run_steps_counted};
    #[test]
    fn checkout_with_global_options_saves_layers_before_moving_and_aborts_on_backup_failure()
    -> Result<(), String> {
        let path = std::env::temp_dir().join(format!(
            "gitcraft-draft-op-{}-{}",
            std::process::id(),
            crate::git::now()
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            crate::git::run(Some(&path), &args)?;
        }
        std::fs::write(path.join("file"), b"base\n").map_err(|e| e.to_string())?;
        crate::git::run(Some(&path), &["add", "file"])?;
        crate::git::run(Some(&path), &["commit", "-qm", "base"])?;
        store::set_enabled(&path, true)?;
        std::fs::write(path.join("file"), b"staged\n").map_err(|e| e.to_string())?;
        crate::git::run(Some(&path), &["add", "file"])?;
        std::fs::write(path.join("file"), b"working\n").map_err(|e| e.to_string())?;
        let ops = vec![vec![
            "-c".into(),
            "advice.detachedHead=false".into(),
            "checkout".into(),
            "-b".into(),
            "next".into(),
        ]];
        run_steps_counted(&path, &ops, OpKind::plain(), None, true, None).0?;
        let saved = store::list(&path, &|| true)?;
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].metadata.branch.as_deref(), Some("refs/heads/main"));
        assert_ne!(saved[0].metadata.index_tree, saved[0].metadata.work_tree);
        std::fs::write(path.join("too-large"), vec![b'x'; store::FILE_LIMIT + 1])
            .map_err(|e| e.to_string())?;
        let ops = vec![vec![
            "checkout".into(),
            "-b".into(),
            "must-not-exist".into(),
        ]];
        let failure = run_steps_counted(&path, &ops, OpKind::plain(), None, true, None)
            .0
            .expect_err("unsafe checkout must stop");
        assert!(failure.contains("Working draft safety check:"), "{failure}");
        assert_eq!(
            crate::git::run(Some(&path), &["branch", "--show-current"])?.trim(),
            "next"
        );
        Ok(())
    }
}
