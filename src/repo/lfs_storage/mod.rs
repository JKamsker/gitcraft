//! Git LFS storage in the tab: the files of the checkout that are only pointers (marked in the
//! commit's file tree), downloading them, the LFS storage panel (disk used, what to fetch,
//! pruning), and Track in LFS from a file's menu.

mod menu;
mod panel;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use super::{Action, Refresh, RepoTab, ops};
use crate::git::{self, FetchFilters, LfsFile, LfsUsage, PrunePlan};

/// What the tab knows of the LFS storage.
#[derive(Default)]
pub(in crate::repo) struct StorageState {
    /// The checkout's LFS files, by path.
    files: HashMap<String, LfsFile>,
    /// They have been read, for the checkout as it is now.
    files_read: bool,
    files_error: Option<String>,
    /// Those that are only pointers, sorted by path, and their size: worked out once per read.
    missing: Vec<String>,
    missing_bytes: u64,
    /// A listing runs; another one is wanted once it is in.
    listing: bool,
    list_again: bool,
    /// What the last listing was for: the commit checked out, and the operations run.
    listed_for: Option<(String, u64)>,
    /// Operations that ended (they may have downloaded or checked out files).
    ops_ended: u64,
    /// The operation running when last looked, if any.
    was_busy: Option<String>,
    /// The patterns the root `.gitattributes` sends through LFS.
    patterns: Vec<String>,
    panel: Option<Panel>,
}

/// The LFS storage panel's readings and what was typed in it.
#[derive(Default)]
pub(in crate::repo) struct Panel {
    usage: Option<Result<LfsUsage, String>>,
    plan: Option<Result<PrunePlan, String>>,
    checking_plan: bool,
    /// Prune was clicked once: the second click deletes.
    confirm_prune: bool,
    filters: Option<Result<FetchFilters, String>>,
    new_include: String,
    new_exclude: String,
    filter_error: Option<String>,
}

/// What the views ask of the LFS storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StorageAct {
    OpenPanel,
    ClosePanel,
    /// Measure the disk used and list the checkout's files again.
    Refresh,
    CheckPrune,
    /// This repository's `lfs.fetchinclude` (or `lfs.fetchexclude`) becomes this list.
    SetFilter {
        exclude: bool,
        list: Vec<String>,
    },
    /// Fetch only this folder (a pattern) from now on: added to `lfs.fetchinclude`.
    FetchOnly(String),
}

impl StorageState {
    /// A file of the checkout kept in LFS that is only its pointer on disk.
    pub(in crate::repo) fn only_pointer(&self, path: &str) -> bool {
        self.missing
            .binary_search_by(|p| p.as_str().cmp(path))
            .is_ok()
    }

    pub(in crate::repo) fn in_lfs(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// The files of the checkout that are only pointers, by path.
    fn missing(&self) -> impl Iterator<Item = &LfsFile> {
        self.missing.iter().filter_map(|p| self.files.get(p))
    }

    /// Takes in a listing; `changed`: the files with changes (git-lfs says those aren't checked
    /// out either).
    fn set_files(&mut self, files: Vec<LfsFile>, changed: &HashSet<&str>) {
        let mut missing: Vec<&LfsFile> = files
            .iter()
            .filter(|f| f.only_pointer(changed.contains(f.path.as_str())))
            .collect();
        missing.sort_by(|a, b| a.path.cmp(&b.path));
        self.missing_bytes = missing.iter().map(|f| f.size).sum();
        self.missing = missing.into_iter().map(|f| f.path.clone()).collect();
        self.files = files.into_iter().map(|f| (f.path.clone(), f)).collect();
        self.files_read = true;
    }

    fn clear(&mut self) {
        self.files.clear();
        self.missing.clear();
        self.missing_bytes = 0;
        self.files_read = false;
    }
}

/// The most bytes of paths one download step names: well within Windows' command line.
const ARGS_BUDGET: usize = 16 * 1024;

/// Downloads the objects of `paths` and checks them out, even when the fetch filters leave
/// them out (`git lfs pull --include /<path>… --exclude ""`); `paths` so many that they don't
/// fit one command line are downloaded in several steps. Every file, by the fetch filters, when
/// empty.
pub(in crate::repo) fn download_action(paths: &[String]) -> Result<Action, String> {
    let label = match paths {
        [] => "LFS: Download".to_owned(),
        [one] => format!("LFS: Download {one}"),
        many => format!("LFS: Download {} files", many.len()),
    };
    if let Some(bad) = paths.iter().find(|p| p.contains(',')) {
        return Err(format!(
            "{bad} has a comma in its name, which git-lfs reads as two patterns: use LFS: Pull to \
             download it"
        ));
    }
    if let Some(bad) = paths.iter().find(|p| p.trim() != p.as_str()) {
        return Err(format!(
            "“{bad}” starts or ends with a space, which git-lfs drops: use LFS: Pull to download it"
        ));
    }
    let mut steps = Vec::new();
    if paths.is_empty() {
        steps.push(vec!["lfs".to_owned(), "pull".to_owned()]);
    }
    let mut batch: Vec<String> = Vec::new();
    let mut used = 0;
    for p in paths {
        let pattern = format!("/{}", git::escape_glob(p));
        if used + pattern.len() > ARGS_BUDGET && !batch.is_empty() {
            steps.push(pull_step(&batch));
            (batch, used) = (Vec::new(), 0);
        }
        used += pattern.len() + 1;
        batch.push(pattern);
    }
    if !batch.is_empty() {
        steps.push(pull_step(&batch));
    }
    Ok(Action::Ops {
        label,
        ops: steps,
        refresh: Refresh::All,
    })
}

fn pull_step(patterns: &[String]) -> Vec<String> {
    vec![
        "lfs".to_owned(),
        "pull".to_owned(),
        "--include".to_owned(),
        patterns.join(","),
        "--exclude".to_owned(),
        String::new(),
    ]
}

/// Keeps files like `path` in LFS from now on (`git lfs track`, writing `.gitattributes`).
pub(in crate::repo) fn track_action(pattern: &str) -> Action {
    Action::Ops {
        label: format!("Track {pattern} in LFS"),
        ops: ops(&[&["lfs", "track", "--", pattern]]),
        refresh: Refresh::All,
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

/// Whether an operation so labelled may have downloaded, checked out or replaced LFS files
/// without moving HEAD (which lists them again anyway); staging and committing don't.
fn changes_lfs_files(label: &str) -> bool {
    ["LFS", "Pull", "Track", "Stash", "Discard", "Pop", "Apply"]
        .iter()
        .any(|w| label.contains(w))
}

impl RepoTab {
    /// Every frame: the reads that are due, then the panel, if open.
    pub(in crate::repo) fn lfs_storage_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.lfs_storage_tick(ctx);
        self.storage_panel(ctx, acts);
    }

    /// Lists the checkout's LFS files when another commit is checked out or an operation ended
    /// (it may have downloaded some); measures the storage again for the panel then.
    pub(in crate::repo) fn lfs_storage_tick(&mut self, ctx: &egui::Context) {
        let busy = self.busy_op.is_some();
        let state = &mut self.lfs_storage;
        if !busy && let Some(ended) = state.was_busy.take() {
            if changes_lfs_files(&ended) {
                state.ops_ended += 1;
            }
            if let Some(panel) = &mut state.panel {
                panel.plan = None;
                panel.confirm_prune = false;
                panel.usage = None;
            }
        }
        if busy {
            state.was_busy.clone_from(&self.busy_op);
        }
        if !self.lfs {
            if state.files_read {
                state.clear();
            }
            return;
        }
        if self.loading || self.refs.head.is_empty() {
            return;
        }
        let wanted = (self.refs.head.clone(), self.lfs_storage.ops_ended);
        if self.lfs_storage.listed_for.as_ref() != Some(&wanted) {
            self.lfs_storage.listed_for = Some(wanted);
            self.list_lfs_files(ctx);
        }
        let measure = self
            .lfs_storage
            .panel
            .as_ref()
            .is_some_and(|p| p.usage.is_none());
        if measure && !busy {
            self.read_storage(ctx);
        }
    }

    /// Lists the checkout's LFS files: one listing at a time, another one after it if asked for
    /// meanwhile (a listing of a big checkout takes a while).
    fn list_lfs_files(&mut self, ctx: &egui::Context) {
        if self.lfs_storage.listing {
            self.lfs_storage.list_again = true;
            return;
        }
        self.lfs_storage.listing = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| Ok((git::lfs_files(&path), git::lfs_patterns(&path)))),
            |tab, answer, ctx| {
                let changed: HashSet<&str> = tab
                    .work
                    .unstaged
                    .iter()
                    .chain(&tab.work.staged)
                    .map(|f| f.path.as_str())
                    .collect();
                let state = &mut tab.lfs_storage;
                state.listing = false;
                match answer.and_then(|(files, patterns)| files.map(|f| (f, patterns))) {
                    Ok((files, patterns)) => {
                        state.set_files(files, &changed);
                        state.patterns = patterns;
                        state.files_error = None;
                    }
                    Err(e) => {
                        state.files_error = Some(format!("Could not list the LFS files: {e}"));
                    }
                }
                if std::mem::take(&mut tab.lfs_storage.list_again) {
                    tab.list_lfs_files(ctx);
                }
            },
        );
    }

    /// Measures the disk used and reads the fetch filters, for the panel.
    fn read_storage(&mut self, ctx: &egui::Context) {
        let Some(panel) = &mut self.lfs_storage.panel else {
            return;
        };
        // Asked for: not asked again until it is in (an empty error reads as "measuring").
        panel.usage = Some(Err(String::new()));
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| Ok((git::lfs_usage(&path), git::fetch_filters(&path)))),
            |tab, read, _| {
                if let Some(panel) = &mut tab.lfs_storage.panel {
                    let (usage, filters) = read.unwrap_or_else(|e| (Err(e.clone()), Err(e)));
                    panel.usage = Some(usage);
                    panel.filters = Some(filters);
                }
            },
        );
    }

    pub(in crate::repo) fn apply_storage_act(&mut self, act: StorageAct, ctx: &egui::Context) {
        match act {
            StorageAct::OpenPanel => self.lfs_storage.panel = Some(Panel::default()),
            StorageAct::ClosePanel => self.lfs_storage.panel = None,
            StorageAct::Refresh => {
                if let Some(panel) = &mut self.lfs_storage.panel {
                    panel.usage = None;
                    panel.plan = None;
                    panel.confirm_prune = false;
                }
                self.lfs_storage.listed_for = None;
            }
            StorageAct::CheckPrune => self.check_prune(ctx),
            StorageAct::SetFilter { exclude, list } => self.set_filter(exclude, list, ctx),
            StorageAct::FetchOnly(folder) => self.fetch_only(folder, ctx),
        }
    }

    fn check_prune(&mut self, ctx: &egui::Context) {
        let Some(panel) = &mut self.lfs_storage.panel else {
            return;
        };
        panel.checking_plan = true;
        panel.confirm_prune = false;
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

    fn fetch_only(&mut self, folder: String, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    let mut include = git::fetch_filters(&path)?.include;
                    if !include.contains(&folder) {
                        include.push(folder.clone());
                    }
                    git::set_fetch_filter(&path, false, &include)?;
                    Ok(include)
                })
            },
            |tab, result, _| {
                tab.status = Some(match result {
                    Ok(include) => {
                        let names: Vec<String> =
                            include.iter().map(|p| git::pattern_label(p)).collect();
                        (format!("LFS now fetches only {}", names.join(", ")), false)
                    }
                    Err(e) => (format!("Could not set what LFS fetches: {e}"), true),
                });
                if let Some(panel) = &mut tab.lfs_storage.panel {
                    panel.usage = None;
                }
            },
        );
    }

    fn set_filter(&mut self, exclude: bool, list: Vec<String>, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    git::set_fetch_filter(&path, exclude, &list)?;
                    git::fetch_filters(&path)
                })
            },
            move |tab, result, _| {
                let Some(panel) = &mut tab.lfs_storage.panel else {
                    return;
                };
                match result {
                    Ok(filters) => {
                        panel.filters = Some(Ok(filters));
                        panel.filter_error = None;
                        if exclude {
                            panel.new_exclude.clear();
                        } else {
                            panel.new_include.clear();
                        }
                    }
                    Err(e) => panel.filter_error = Some(format!("Could not save it: {e}")),
                }
            },
        );
    }
}
