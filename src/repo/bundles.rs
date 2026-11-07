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
        self.then(
            ctx,
            move || git::bundle_refs(&repo),
            move |tab, result, _| {
                if !waiting(tab, id) {
                    return;
                }
                match result {
                    Ok(refs) => {
                        let selected = refs
                            .iter()
                            .filter(|r| r.name == "HEAD")
                            .map(|r| r.name.clone())
                            .collect::<BTreeSet<_>>();
                        tab.dialog = Some(Dialog::BundleExport {
                            refs,
                            selected,
                            base: String::new(),
                        });
                    }
                    Err(e) => {
                        tab.dialog = None;
                        tab.status = Some((format!("Read bundle refs: {e}"), true));
                    }
                }
            },
        );
    }
    pub(super) fn open_bundle_import(&mut self, ctx: &egui::Context) {
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
        self.then(
            ctx,
            move || git::read_bundle_file(&file).and_then(|b| git::inspect_bundle(&repo, b)),
            move |tab, result, _| {
                if !waiting(tab, id) {
                    return;
                }
                match result {
                    Ok(preview) => {
                        let imports = preview
                            .refs
                            .iter()
                            .map(|r| {
                                (
                                    true,
                                    git::BundleImport {
                                        source: r.name.clone(),
                                        destination: git::bundle_destination(&r.name),
                                    },
                                )
                            })
                            .collect();
                        tab.dialog = Some(Dialog::BundleIncoming {
                            label,
                            preview: Box::new(preview),
                            imports,
                        });
                    }
                    Err(e) => {
                        tab.dialog = None;
                        tab.status = Some((format!("Inspect bundle: {e}"), true));
                    }
                }
            },
        );
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
    pub(super) fn import_bundle(
        &mut self,
        preview: Box<git::BundlePreview>,
        imports: Vec<git::BundleImport>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        let repo = self.path.clone();
        self.bundle_job("Import bundle refs".into(), Refresh::All, ctx, move || {
            git::import_bundle(&repo, &preview, &imports)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    }
}
