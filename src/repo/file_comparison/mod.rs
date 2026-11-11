//! Separate raw snapshot panes. Opening one drops DiffView and its reading-mode lifetimes.
mod selection;

mod code_moves;
mod model;
#[cfg(test)]
mod tests;
mod text;
mod view;

use super::{
    Action, DiffKey, Msg, OpKind, Refresh, RepoTab,
    messages::{spawn_mutation, spawn_or},
};
use crate::git::file_comparison::{self as files, Entry, MigrationSource};
use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// The same content-only renderer, without a repository tab or mutation actions.
pub(crate) struct ReadOnlyComparison(model::Pair);
impl ReadOnlyComparison {
    pub(crate) fn prepare(
        labels: [String; 2],
        bytes: [Vec<u8>; 2],
        present: [bool; 2],
        active: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        let [a, b] = bytes;
        let versions = [a, b]
            .into_iter()
            .zip(present)
            .map(|(bytes, present)| files::Version {
                mode: present.then(|| "100644".into()),
                oid: None,
                bytes,
                warning: None,
            })
            .collect::<Vec<_>>();
        let boundary = model::boundary("standalone", &versions[0], &versions[1], active)?;
        let [a, b]: [files::Version; 2] = versions
            .try_into()
            .map_err(|_| "Invalid comparison sides")?;
        Ok(Self(model::Pair {
            labels,
            versions: [a, b],
            boundary,
        }))
    }
    pub(crate) fn ui(&self, ui: &mut egui::Ui) {
        view::show_pair(ui, &self.0, &crate::theme::Tokens::get(ui.ctx()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Migration,
    CodeMoves,
    Lens,
    Matrix,
}
enum Data {
    Migration(files::Migration),
    CodeMoves(Box<code_moves::CodeMoves>),
    Lens(Box<model::LensData>),
    Matrix(files::Matrix),
}
pub(super) struct State {
    page: Page,
    generation: Arc<AtomicU64>,
    request: u64,
    /// Ownership epoch for displayed rows, independent of later read-only pair requests.
    data_request: u64,
    loading: bool,
    data: Option<Result<Arc<Data>, String>>,
    pair: Option<Result<model::Pair, String>>,
    source: u8,
    code_move: Option<usize>,
    from: String,
    to: String,
    path: String,
    previous: Option<String>,
    folder: String,
    branches: Vec<String>,
    selected: BTreeSet<String>,
    cells: Vec<(Vec<u8>, usize)>,
}
impl State {
    pub(super) fn invalidate(&mut self) {
        self.generation.store(0, Ordering::Relaxed);
        self.request = 0;
        self.data_request = 0;
        self.loading = false;
        self.data = Some(Err("Repository changed. Refresh the file view.".into()));
        self.pair = None;
    }
    fn begin(&mut self, clear: bool) -> u64 {
        static REQUESTS: AtomicU64 = AtomicU64::new(0);
        self.request = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
        self.generation.store(self.request, Ordering::Relaxed);
        self.loading = true;
        self.pair = None;
        if clear {
            self.data_request = 0;
            self.data = None;
            self.cells.clear();
            self.code_move = None;
        }
        self.request
    }
    fn accept(&mut self, request: u64, result: Result<Data, String>) {
        if request == self.request {
            self.loading = false;
            self.data_request = request;
            self.data = Some(result.map(Arc::new));
        }
    }
    fn accept_pair(&mut self, request: u64, result: Result<model::Pair, String>) {
        if request == self.request {
            self.loading = false;
            self.pair = Some(result);
        }
    }
}
impl RepoTab {
    pub(super) fn open_file_comparison(&mut self, page: Page, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Finish or close the editor first".into(),
                true,
            )));
            return;
        }
        let focused = self
            .diff
            .as_ref()
            .and_then(|d| match &d.key {
                DiffKey::Work(f) => Some((f.path.clone(), f.old_path.clone())),
                _ => None,
            })
            .or_else(|| {
                self.work
                    .unstaged
                    .first()
                    .or(self.work.staged.first())
                    .map(|f| (f.path.clone(), f.old_path.clone()))
            });
        let mut branches: Vec<String> = self
            .refs
            .local
            .iter()
            .map(|b| format!("refs/heads/{}", b.name))
            .chain(
                self.refs
                    .remotes
                    .iter()
                    .flat_map(|(_, bs)| bs.iter().map(|b| format!("refs/remotes/{}", b.name))),
            )
            .collect();
        branches.sort();
        branches.dedup();
        if !branches.iter().any(|b| b == "HEAD") {
            branches.push("HEAD".into());
        }
        let selected = branches.iter().take(2).cloned().collect();
        let (path, previous) = focused.unwrap_or_default();
        let revision = self
            .selected_hash()
            .filter(|h| h != super::WIP_HASH)
            .unwrap_or_else(|| "HEAD".into());
        let (source, from, to) = if page == Page::CodeMoves {
            match self.diff.as_ref().map(|d| &d.key) {
                Some(DiffKey::Work(f)) => (
                    if f.area == crate::git::Area::Staged {
                        0
                    } else {
                        3
                    },
                    "HEAD~1".into(),
                    revision.clone(),
                ),
                Some(DiffKey::Commit { hash, .. }) => (1, "HEAD~1".into(), hash.clone()),
                Some(DiffKey::Range { from, to, .. }) => (
                    if to.is_some() { 2 } else { 4 },
                    from.clone(),
                    to.clone().unwrap_or(revision.clone()),
                ),
                _ => (0, "HEAD~1".into(), revision.clone()),
            }
        } else {
            (0, "HEAD~1".into(), revision)
        };
        self.leave_pages();
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        self.file_comparison = Some(State {
            page,
            generation: Arc::new(AtomicU64::new(0)),
            request: 0,
            data_request: 0,
            loading: false,
            data: None,
            pair: None,
            source,
            code_move: None,
            from,
            to,
            path,
            previous,
            folder: String::new(),
            branches,
            selected,
            cells: Vec::new(),
        });
        self.load_file_comparison(ctx);
    }
    pub(super) fn load_file_comparison(&mut self, ctx: &egui::Context) {
        let Some(state) = &mut self.file_comparison else {
            return;
        };
        let request = state.begin(true);
        let alive = Arc::downgrade(&state.generation);
        let (page, source, from, to, path, previous, folder, branches) = (
            state.page,
            state.source,
            state.from.clone(),
            state.to.clone(),
            state.path.clone(),
            state.previous.clone(),
            state.folder.clone(),
            state.selected.iter().cloned().collect::<Vec<_>>(),
        );
        let repo = self.path.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |e| {
                Msg::Then(Box::new(move |tab, _| {
                    if let Some(s) = &mut tab.file_comparison {
                        s.accept(request, Err(e));
                    }
                }))
            },
            move || {
                let active = || {
                    alive
                        .upgrade()
                        .is_some_and(|g| g.load(Ordering::Relaxed) == request)
                };
                let result = match page {
                    Page::CodeMoves => code_moves::read(&repo, source, &from, &to, &active)
                        .map(|data| Data::CodeMoves(Box::new(data))),
                    Page::Migration => files::read_migration(
                        &repo,
                        &match source {
                            0 => MigrationSource::Staged,
                            1 => MigrationSource::Commit(to),
                            _ => MigrationSource::Range(from, to),
                        },
                        &active,
                    )
                    .map(Data::Migration),
                    Page::Matrix => {
                        files::matrix(&repo, &branches, &folder, &active).map(Data::Matrix)
                    }
                    Page::Lens => {
                        files::read_lens(&repo, &files::LensSpec { path, previous }, &active)
                            .and_then(|s| model::prepare_lens(s, &active))
                            .map(|s| Data::Lens(Box::new(s)))
                    }
                };
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == repo
                        && let Some(s) = &mut tab.file_comparison
                    {
                        s.accept(request, result);
                    }
                }))
            },
        );
    }
    fn load_comparison_pair(
        &mut self,
        entries: [Option<Entry>; 2],
        labels: [String; 2],
        ctx: &egui::Context,
    ) {
        let Some(state) = &mut self.file_comparison else {
            return;
        };
        let request = state.begin(false);
        let alive = Arc::downgrade(&state.generation);
        let repo = self.path.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |e| {
                Msg::Then(Box::new(move |tab, _| {
                    if let Some(s) = &mut tab.file_comparison {
                        s.accept_pair(request, Err(e));
                    }
                }))
            },
            move || {
                let active = || {
                    alive
                        .upgrade()
                        .is_some_and(|g| g.load(Ordering::Relaxed) == request)
                };
                let result = (|| {
                    let a = files::blob(&repo, entries[0].as_ref(), &active)?;
                    let b = files::blob(&repo, entries[1].as_ref(), &active)?;
                    let boundary = model::boundary("comparison", &a, &b, &active)?;
                    Ok(model::Pair {
                        labels,
                        versions: [a, b],
                        boundary,
                    })
                })();
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == repo
                        && let Some(s) = &mut tab.file_comparison
                    {
                        s.accept_pair(request, result);
                    }
                }))
            },
        );
    }
}
