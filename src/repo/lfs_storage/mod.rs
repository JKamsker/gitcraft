//! Git LFS storage in the tab: the files of the checkout that are only pointers (marked in the
//! commit's file tree), downloading them, the LFS storage panel (disk used, what to fetch,
//! pruning), and Track in LFS from a file's menu.

mod menu;
mod panel;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use super::{Action, Refresh, RepoTab, ops};
use crate::git::{self, LfsFile, LfsUsage, PrunePlan};

/// What the tab knows of the LFS storage.
#[derive(Default)]
pub(in crate::repo) struct StorageState {
    /// git-lfs is installed where the repository is: `None` until asked.
    installed: Option<bool>,
    asking_installed: bool,
    /// The checkout's LFS files, by path, for the full read numbered so.
    files: HashMap<String, LfsFile>,
    files_for: Option<u64>,
    /// Those of them that are only pointers, by path (sorted once, not every frame).
    missing: Vec<String>,
    files_error: Option<String>,
    panel: Option<Panel>,
}

/// The LFS storage panel's readings and what was typed in it.
#[derive(Default)]
pub(in crate::repo) struct Panel {
    usage: Option<Result<LfsUsage, String>>,
    plan: Option<Result<PrunePlan, String>>,
    checking_plan: bool,
    /// `lfs.fetchinclude` and `lfs.fetchexclude`, once read.
    filters: Option<(Vec<String>, Vec<String>)>,
    new_include: String,
    new_exclude: String,
    filter_error: Option<String>,
}

/// What the views ask of the LFS storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StorageAct {
    OpenPanel,
    ClosePanel,
    /// Read the disk used and the checkout's files again.
    Refresh,
    CheckPrune,
    /// `lfs.fetchinclude` (or `lfs.fetchexclude`) becomes this list.
    SetFilter {
        exclude: bool,
        list: Vec<String>,
    },
    /// Fetch only this folder from now on (added to `lfs.fetchinclude`).
    FetchOnly(String),
}

impl StorageState {
    /// A file of the checkout kept in LFS whose object was never downloaded: only its pointer
    /// is on disk.
    pub(in crate::repo) fn only_pointer(&self, path: &str) -> bool {
        self.files.get(path).is_some_and(|f| !f.downloaded)
    }

    pub(in crate::repo) fn in_lfs(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// The files of the checkout that are only pointers, by path.
    fn missing(&self) -> impl Iterator<Item = &LfsFile> {
        self.missing.iter().filter_map(|p| self.files.get(p))
    }

    fn set_files(&mut self, files: Vec<LfsFile>) {
        let mut missing: Vec<String> = files
            .iter()
            .filter(|f| !f.downloaded)
            .map(|f| f.path.clone())
            .collect();
        missing.sort();
        self.missing = missing;
        self.files = files.into_iter().map(|f| (f.path.clone(), f)).collect();
    }
}

/// Downloads the objects of `paths` (every file of the checkout, by the fetch filters, when
/// empty) and checks them out: `git lfs pull`. A file asked for by name is downloaded even when
/// the filters leave it out.
pub(in crate::repo) fn download_action(paths: &[String]) -> Result<Action, String> {
    let label = match paths {
        [] => "LFS: Download".to_owned(),
        [one] => format!("LFS: Download {one}"),
        many => format!("LFS: Download {} files", many.len()),
    };
    let mut args = vec!["lfs".to_owned(), "pull".to_owned()];
    if !paths.is_empty() {
        if let Some(bad) = paths.iter().find(|p| p.contains(',')) {
            return Err(format!(
                "{bad} has a comma in its name, which git-lfs reads as two patterns: \
                 use LFS: Pull to download it"
            ));
        }
        args.extend(["--include".to_owned(), include_patterns(paths)]);
        args.extend(["--exclude".to_owned(), String::new()]);
    }
    Ok(Action::Ops {
        label,
        ops: vec![args],
        refresh: Refresh::All,
    })
}

/// Paths as `--include` patterns: from the root (a leading `/`), the characters of a pattern
/// escaped, so that each names that file alone.
fn include_patterns(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| {
            let escaped: String = p
                .chars()
                .flat_map(|c| match c {
                    '*' | '?' | '[' | ']' | '\\' => vec!['\\', c],
                    c => vec![c],
                })
                .collect();
            format!("/{escaped}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Keeps files like `path` in LFS from now on (`git lfs track`, writing `.gitattributes`).
pub(in crate::repo) fn track_action(pattern: &str) -> Action {
    Action::Ops {
        label: format!("Track {pattern} in LFS"),
        ops: ops(&[&["lfs", "track", "--", pattern]]),
        refresh: Refresh::Status,
    }
}

/// Removes old LFS objects that nothing recent, checked out or unpushed needs.
pub(in crate::repo) fn prune_action() -> Action {
    Action::Ops {
        label: "LFS: Prune old objects".to_owned(),
        ops: ops(&[&["lfs", "prune"]]),
        refresh: Refresh::All,
    }
}

/// `work`'s answer, or what it said if it panicked: the flag waiting for the answer is let go
/// of all the same.
fn caught<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|p| {
        let said = p
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(format!("Internal error: {said}"))
    })
}

impl RepoTab {
    /// Every frame: reads the checkout's LFS files after each full read of the repository,
    /// whether git-lfs is installed once; then draws the panel, if open.
    pub(in crate::repo) fn lfs_storage_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.lfs_storage_tick(ctx);
        self.storage_panel(ctx, acts);
    }

    /// [`Self::lfs_storage_ui`] without the drawing: starts the reads that are due.
    pub(in crate::repo) fn lfs_storage_tick(&mut self, ctx: &egui::Context) {
        if self.lfs_storage.installed.is_none()
            && !self.lfs_storage.asking_installed
            && !self.loading
        {
            self.ask_lfs_installed(ctx);
        }
        if self.lfs && !self.loading && self.lfs_storage.files_for != Some(self.reads) {
            self.read_lfs_files(ctx);
        }
    }

    fn ask_lfs_installed(&mut self, ctx: &egui::Context) {
        self.lfs_storage.asking_installed = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| Ok(git::lfs_installed(&path))),
            |tab, found, _| {
                tab.lfs_storage.asking_installed = false;
                tab.lfs_storage.installed = Some(found.unwrap_or(false));
            },
        );
    }

    fn read_lfs_files(&mut self, ctx: &egui::Context) {
        let read = self.reads;
        self.lfs_storage.files_for = Some(read);
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| git::lfs_files(&path)),
            move |tab, files, _| {
                if tab.lfs_storage.files_for != Some(read) {
                    return;
                }
                match files {
                    Ok(files) => {
                        tab.lfs_storage.set_files(files);
                        tab.lfs_storage.files_error = None;
                    }
                    Err(e) => {
                        tab.lfs_storage.files_error =
                            Some(format!("Could not list the LFS files: {e}"));
                    }
                }
            },
        );
    }

    /// Reads the disk used and the fetch filters, for the panel.
    fn read_storage(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| Ok((git::lfs_usage(&path), git::fetch_filters(&path)))),
            |tab, read, _| {
                if let Some(panel) = &mut tab.lfs_storage.panel {
                    match read {
                        Ok((usage, filters)) => {
                            panel.usage = Some(usage);
                            panel.filters = Some(filters);
                        }
                        Err(e) => {
                            panel.usage = Some(Err(e));
                            panel.filters = Some(Default::default());
                        }
                    }
                }
            },
        );
    }

    pub(in crate::repo) fn apply_storage_act(&mut self, act: StorageAct, ctx: &egui::Context) {
        match act {
            StorageAct::OpenPanel => {
                self.lfs_storage.panel = Some(Panel::default());
                self.read_storage(ctx);
                self.lfs_storage.files_for = None;
            }
            StorageAct::ClosePanel => self.lfs_storage.panel = None,
            StorageAct::Refresh => {
                if let Some(panel) = &mut self.lfs_storage.panel {
                    panel.usage = None;
                    panel.plan = None;
                }
                self.read_storage(ctx);
                self.lfs_storage.files_for = None;
            }
            StorageAct::CheckPrune => {
                let Some(panel) = &mut self.lfs_storage.panel else {
                    return;
                };
                panel.checking_plan = true;
                let path = self.path.clone();
                self.then(
                    ctx,
                    move || caught(|| git::lfs_prune_plan(&path)),
                    |tab, plan, _| {
                        if let Some(panel) = &mut tab.lfs_storage.panel {
                            panel.checking_plan = false;
                            panel.plan = Some(plan);
                        }
                    },
                );
            }
            StorageAct::SetFilter { exclude, list } => self.set_filter(exclude, list, ctx),
            StorageAct::FetchOnly(folder) => {
                let path = self.path.clone();
                self.then(
                    ctx,
                    move || {
                        caught(|| {
                            let (mut include, _) = git::fetch_filters(&path);
                            if !include.contains(&folder) {
                                include.push(folder.clone());
                            }
                            git::set_fetch_filter(&path, false, &include).map(|()| folder)
                        })
                    },
                    |tab, result, _| {
                        tab.status = Some(match result {
                            Ok(folder) => (
                                format!("LFS fetches only {folder} (and the other folders set in LFS: Storage…) from now on"),
                                false,
                            ),
                            Err(e) => (format!("Could not set what LFS fetches: {e}"), true),
                        });
                    },
                );
            }
        }
    }

    fn set_filter(&mut self, exclude: bool, list: Vec<String>, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    git::set_fetch_filter(&path, exclude, &list)?;
                    Ok(git::fetch_filters(&path))
                })
            },
            |tab, result, _| {
                let Some(panel) = &mut tab.lfs_storage.panel else {
                    return;
                };
                match result {
                    Ok(filters) => {
                        panel.filters = Some(filters);
                        panel.filter_error = None;
                    }
                    Err(e) => panel.filter_error = Some(format!("Could not save it: {e}")),
                }
            },
        );
    }
}
