//! File selection is local; Git/temporary transfers and all data work stay off the UI thread.
use super::messages::spawn_or;
use super::{Dialog, Msg, Refresh, RepoTab};
use crate::git;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static REQUESTS: AtomicU64 = AtomicU64::new(1);
fn loading(tab: &mut RepoTab) -> u64 {
    let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
    tab.dialog = Some(Dialog::BundleLoading(id));
    id
}
fn waiting(tab: &RepoTab, id: u64) -> bool {
    matches!(tab.dialog, Some(Dialog::BundleLoading(n)) if n == id)
}

impl RepoTab {
    pub(super) fn open_bundle_export(&mut self, ctx: &egui::Context) {
        let id = loading(self);
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || git::bundle_refs(&repo),
            move |tab, result, _| tab.bundle_export_loaded(id, result),
        );
    }
    pub(super) fn open_bundle_inspect(&mut self, ctx: &egui::Context) {
        let Some(file) = rfd::FileDialog::new()
            .set_title("Inspect local Git bundle (temporary copy to repository machine)")
            .add_filter("Git bundle", &["bundle"])
            .pick_file()
        else {
            return;
        };
        let label = file.display().to_string();
        let repo = self.path.clone();
        let id = loading(self);
        self.then_or(
            ctx,
            false,
            Err,
            move || git::read_bundle_file(&file).and_then(|b| git::inspect_bundle(&repo, b)),
            move |tab, result, _| tab.bundle_inspect_loaded(id, label, result),
        );
    }
    fn bundle_export_loaded(&mut self, id: u64, result: Result<Vec<git::BundleRef>, String>) {
        if !waiting(self, id) {
            return;
        }
        match result {
            Ok(refs) => {
                let selected = refs
                    .iter()
                    .filter(|r| r.name == "HEAD")
                    .map(|r| r.name.clone())
                    .collect::<BTreeSet<_>>();
                self.dialog = Some(Dialog::BundleExport {
                    refs,
                    selected,
                    base: String::new(),
                });
            }
            Err(e) => {
                self.dialog = None;
                self.status = Some((format!("Read bundle refs: {e}"), true));
            }
        }
    }
    fn bundle_inspect_loaded(
        &mut self,
        id: u64,
        label: String,
        result: Result<git::BundlePreview, String>,
    ) {
        if !waiting(self, id) {
            return;
        }
        match result {
            Ok(preview) => {
                self.dialog = Some(Dialog::BundleIncoming {
                    label,
                    preview: Box::new(preview),
                });
            }
            Err(e) => {
                self.dialog = None;
                self.status = Some((format!("Inspect bundle: {e}"), true));
            }
        }
    }
    pub(super) fn export_bundle(
        &mut self,
        refs: Vec<git::BundleRef>,
        base: String,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(file) = rfd::FileDialog::new()
            .set_title("Save bundle on this machine (choose a new file)")
            .set_file_name("repository.bundle")
            .add_filter("Git bundle", &["bundle"])
            .save_file()
        else {
            return;
        };
        let repo = self.path.clone();
        let label = format!("Export bundle to {}", file.display());
        self.bundle_job(label, Refresh::Status, ctx, move || {
            let bytes = git::export_bundle(&repo, &refs, Some(&base))?;
            git::save_bundle_file(&file, &bytes)
        });
    }
    fn bundle_job(
        &mut self,
        label: String,
        refresh: Refresh,
        ctx: &egui::Context,
        job: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) {
        self.busy_op = Some(label.clone());
        self.op_started = Some(Instant::now());
        self.status = Some((format!("{label}…"), false));
        // A previous operation's credential retry must not replay after a bundle failure.
        self.last_op = None;
        let failed_label = label.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::BundleDone(failed_label, Err(e), refresh),
            move || Msg::BundleDone(label, job(), refresh),
        );
    }

    pub(super) fn bundle_done(
        &mut self,
        label: String,
        result: Result<(), String>,
        refresh: Refresh,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        let ok = result.is_ok();
        let recorded = result
            .as_ref()
            .map(|()| String::new())
            .map_err(Clone::clone);
        self.log_activity(&label, &recorded, false, false, Vec::new());
        self.op_started = None;
        // Untrusted prerequisite descriptions are never interpreted as an authentication
        // request or used to retry a previous operation.
        self.status = Some(match result {
            Ok(()) => (format!("{label}: done"), false),
            Err(e) => (format!("{label} failed: {e}"), true),
        });
        match refresh {
            Refresh::All => self.reload(ctx),
            Refresh::Status => self.reload_status(ctx),
        }
        self.after_op(ok, false, ctx);
        self.link_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_bundle_read_releases_loading_dialog() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("unread-bundle-repository".into(), &ctx);
        tab.tx.cancel.cancel();
        tab.open_bundle_export(&ctx);
        let start = Instant::now();
        while tab.dialog.is_some() && start.elapsed() < std::time::Duration::from_secs(5) {
            tab.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(tab.dialog.is_none());
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(s, bad)| *bad && s.contains("cancelled"))
        );
    }

    #[test]
    fn cancelled_and_replaced_bundle_readings_are_not_current() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(std::path::PathBuf::from("."), &ctx);
        let first = loading(&mut tab);
        assert!(waiting(&tab, first));
        tab.dialog = None;
        assert!(!waiting(&tab, first));
        let second = loading(&mut tab);
        assert!(waiting(&tab, second));
        assert!(!waiting(&tab, first));
        for replaced in [false, true] {
            tab.dialog = if replaced {
                Some(Dialog::BundleLoading(second))
            } else {
                None
            };
            tab.bundle_export_loaded(first, Ok(Vec::new()));
            tab.bundle_inspect_loaded(
                first,
                "stale.bundle".into(),
                Ok(git::BundlePreview {
                    refs: Vec::new(),
                    prerequisites: Vec::new(),
                    verification_error: None,
                }),
            );
            tab.bundle_export_loaded(first, Err("obsolete export".into()));
            tab.bundle_inspect_loaded(first, "stale.bundle".into(), Err("obsolete inspect".into()));
            assert_eq!(waiting(&tab, second), replaced);
            assert!(tab.status.is_none());
            if !replaced {
                assert!(tab.dialog.is_none());
            }
        }
    }
    #[test]
    fn bundle_completion_records_outcomes_and_resumes_or_drops_queued_operations() {
        use crate::repo::activity::Outcome;
        use crate::repo::running::Waiter;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/bundle-queue".into(), &ctx);
        tab.gerrit = Some(crate::gerrit::Server {
            remote: "origin".into(),
            web: "https://gerrit.example.invalid".into(),
            project: "app".into(),
            web_warning: None,
        });
        tab.refs.head = "a".repeat(40);
        tab.waiting.push_back(Waiter::Push);
        tab.busy_op = Some("Export bundle".into());
        tab.op_started = Some(Instant::now());
        tab.bundle_done("Export bundle".into(), Ok(()), Refresh::Status, &ctx);
        assert!(tab.waiting.is_empty());
        assert!(matches!(tab.dialog, Some(Dialog::GerritReview { .. })));
        assert!(
            tab.activity
                .last()
                .is_some_and(|e| e.label == "Export bundle" && e.outcome == Outcome::Done)
        );
        tab.dialog = None;
        tab.waiting.push_back(Waiter::Push);
        tab.bundle_done(
            "Export bundle".into(),
            Err("missing object".into()),
            Refresh::Status,
            &ctx,
        );
        assert!(tab.waiting.is_empty());
        assert!(tab.dialog.is_none());
        assert!(tab.activity.last().is_some_and(
            |e| matches!(&e.outcome, Outcome::Failed(message) if message == "missing object")
        ));
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(message, failed)| *failed && message.contains("Push"))
        );
    }
}
