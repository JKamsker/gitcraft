//! Sparse-checkout folder picker. All repository inspection and mutations run off the UI thread.

use std::collections::{BTreeMap, BTreeSet};

use super::dialogs::Outcome;
use super::{Action, Dialog, RepoTab};
use crate::git::{self, SparseState};
use crate::theme::Tokens;

impl RepoTab {
    pub(super) fn open_sparse(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some((
                "Wait for the operation to finish before editing sparse checkout".into(),
                true,
            ));
            return;
        }
        self.sparse_requests += 1;
        let id = self.sparse_requests;
        self.dialog = Some(Dialog::SparseLoading(id));
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::sparse_state(&path),
            move |tab, result, _| {
                if !matches!(tab.dialog, Some(Dialog::SparseLoading(request)) if request == id) {
                    return;
                }
                match result {
                    Ok(state) => {
                        let selected = state.selected.clone();
                        let enabled = state.enabled;
                        tab.dialog = Some(Dialog::Sparse {
                            state: Box::new(state),
                            selected,
                            enabled,
                            convert: false,
                        });
                    }
                    Err(e) => {
                        tab.dialog = None;
                        tab.status = Some((format!("Sparse checkout: {e}"), true));
                    }
                }
            },
        );
    }

    pub(super) fn set_sparse(
        &mut self,
        before: SparseState,
        folders: Option<Vec<String>>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        self.busy_op = Some("Sparse checkout".into());
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::set_sparse(&path, &before, folders.as_deref()),
            |tab, result, ctx| {
                tab.busy_op = None;
                tab.status = Some(match result {
                    Ok(()) => ("Sparse checkout updated".into(), false),
                    Err(e) => (format!("Sparse checkout: {e}"), true),
                });
                tab.reload(ctx);
            },
        );
    }
}

#[derive(Default)]
struct Folder {
    children: BTreeMap<String, Folder>,
}

impl Folder {
    fn insert(&mut self, path: &str) {
        let mut node = self;
        for component in path.split('/') {
            node = node.children.entry(component.to_owned()).or_default();
        }
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        selected: &mut BTreeSet<String>,
        inherited: bool,
    ) {
        for (name, node) in &self.children {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let mut checked = inherited || selected.contains(&path);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!inherited, egui::Checkbox::without_text(&mut checked))
                    .changed()
                {
                    if checked {
                        selected.insert(path.clone());
                    } else {
                        selected.remove(&path);
                    }
                }
                if node.children.is_empty() {
                    ui.label(name);
                } else {
                    egui::CollapsingHeader::new(name)
                        .id_salt(&path)
                        .show(ui, |ui| {
                            node.show(ui, &path, selected, checked);
                        });
                }
            });
        }
    }
}

pub(super) fn dialog(
    ui: &mut egui::Ui,
    state: &SparseState,
    selected: &mut BTreeSet<String>,
    enabled: &mut bool,
    convert: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.set_width(480.0);
    ui.heading("Sparse checkout");
    ui.checkbox(enabled, "Check out selected folders only");
    ui.label(egui::RichText::new("Root files and files immediately inside parent folders stay included. An empty selection includes root files only. Commit, stash or move changes first. Move ignored files outside the working tree too: Git can delete them during sparse changes.").small().color(t.text_dim));
    if state.partial {
        ui.colored_label(t.warning, "Adding folders or turning this off downloads their missing file contents from the remote.");
    }
    let non_cone = state.enabled && !state.cone;
    if non_cone && *enabled {
        ui.colored_label(t.warning, "This repository uses custom non-cone patterns. Editing folders replaces those patterns.");
        ui.collapsing("Current patterns", |ui| {
            ui.monospace(&state.patterns);
        });
        ui.checkbox(
            convert,
            "Replace custom patterns with the folders selected below",
        );
    }
    let mut root = Folder::default();
    for folder in state.folders.iter().chain(state.selected.iter()) {
        root.insert(folder);
    }
    ui.add_enabled_ui(*enabled && (!non_cone || *convert), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("sparse_folders")
            .max_height(340.0)
            .show(ui, |ui| {
                root.show(ui, "", selected, false);
            });
    });
    let folders: Vec<String> = selected.iter().cloned().collect();
    let validation = git::validate_folders(&folders);
    if let Err(e) = &validation {
        ui.colored_label(t.danger, e);
    }
    let valid = (!*enabled || !non_cone || *convert) && (!*enabled || validation.is_ok());
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(valid, egui::Button::new("Apply folders"))
            .clicked()
        {
            outcome = Outcome::Done(Box::new(Action::SetSparse {
                before: Box::new(state.clone()),
                folders: enabled.then_some(folders),
            }));
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

impl RepoTab {
    /// Ask only when an actual required blob is absent, before starting the view's reader.
    pub(super) fn check_history_download(&mut self, action: &Action, ctx: &egui::Context) -> bool {
        if !self.partial_clone || self.history_download_allowed {
            return false;
        }
        let (path, revisions, next) = match action {
            Action::OpenDiff(key @ super::DiffKey::Commit { hash, path }) => (
                path.clone(),
                vec![hash.clone(), format!("{hash}^")],
                Action::OpenDiff(key.clone()),
            ),
            Action::OpenDiff(key @ super::DiffKey::Range { from, to, path }) => (
                path.clone(),
                std::iter::once(from.clone())
                    .chain(to.iter().cloned())
                    .collect(),
                Action::OpenDiff(key.clone()),
            ),
            Action::FileView { path, rev, tab } => (
                path.clone(),
                vec![rev.clone().unwrap_or_else(|| "HEAD".into())],
                Action::FileView {
                    path: path.clone(),
                    rev: rev.clone(),
                    tab: *tab,
                },
            ),
            _ => return false,
        };
        self.history_requests += 1;
        let id = self.history_requests;
        self.dialog = Some(Dialog::HistoryLoading(id));
        let repo = self.path.clone();
        let file = path.clone();
        self.then(
            ctx,
            move || {
                for revision in revisions {
                    // A root commit has no parent. Resolve first, so only existing revisions
                    // get passed to the no-lazy-fetch probe (and ref names can't change mid-read).
                    let revision = match git::resolve(&repo, &revision) {
                        Ok(rev) => rev,
                        Err(_) if revision.ends_with('^') => continue,
                        Err(e) => return Err(e),
                    };
                    if git::file_needs_download(&repo, &revision, &file)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            },
            move |tab, result, ctx| {
                if !matches!(tab.dialog, Some(Dialog::HistoryLoading(request)) if request == id) {
                    return;
                }
                match result {
                    Ok(true) => {
                        tab.dialog = Some(Dialog::DownloadHistory {
                            path,
                            action: Some(Box::new(next)),
                        })
                    }
                    Ok(false) => {
                        tab.dialog = None;
                        tab.apply(Action::AllowHistoryDownload(Box::new(next)), ctx);
                    }
                    Err(e) => {
                        tab.dialog = None;
                        tab.status =
                            Some((format!("Checking historical file contents: {e}"), true));
                    }
                }
            },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::file_view::FileTab;
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn missing_historical_file_waits_for_download_and_cancel_keeps_it_missing() -> Result<(), String>
    {
        let root =
            std::env::temp_dir().join(format!("gitcraft-download-dialog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source");
        std::fs::create_dir_all(source.join("hidden")).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "uploadpack.allowFilter", "true"],
        ] {
            git::run(Some(&source), &args)?;
        }
        std::fs::write(source.join("hidden/file"), "contents").map_err(|e| e.to_string())?;
        git::run(Some(&source), &["add", "-A"])?;
        git::run(
            Some(&source),
            &["-c", "commit.gpgsign=false", "commit", "-qm", "one"],
        )?;
        let dest = root.join("clone");
        git::clone_with_options(
            &source.to_string_lossy(),
            &dest,
            None,
            &git::CloneOptions {
                blobless: true,
                folders: Some(vec![]),
                ..Default::default()
            },
        )?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dest.clone(), &ctx);
        tab.partial_clone = true;
        let action = || Action::FileView {
            path: "hidden/file".into(),
            rev: Some("HEAD".into()),
            tab: FileTab::File,
        };
        tab.apply(action(), &ctx);
        assert!(matches!(tab.dialog, Some(Dialog::HistoryLoading(_))));
        let until = Instant::now() + Duration::from_secs(10);
        while matches!(tab.dialog, Some(Dialog::HistoryLoading(_))) && Instant::now() < until {
            tab.poll(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(tab.dialog, Some(Dialog::DownloadHistory { .. })));
        assert!(tab.file_view.is_none());
        assert!(git::file_needs_download(&dest, "HEAD", "hidden/file")?);
        // Closing the question doesn't open anything or fetch its contents.
        tab.dialog = None;
        assert!(git::file_needs_download(&dest, "HEAD", "hidden/file")?);
        // A cancelled in-flight probe cannot replace a later dialog.
        tab.apply(action(), &ctx);
        tab.dialog = Some(Dialog::stash());
        let until = Instant::now() + Duration::from_millis(100);
        while Instant::now() < until {
            tab.poll(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(tab.dialog, Some(Dialog::Stash { .. })));
        assert!(tab.file_view.is_none());
        drop(tab);
        let _ = std::fs::remove_dir_all(root);
        Ok(())
    }
}
