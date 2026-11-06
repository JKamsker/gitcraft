//! The reflog view, above the graph: where HEAD and each local branch have been (what moved
//! them, from which commit to which), and the scan for lost commits that no reflog has any
//! more. An entry shows its commit in the detail pane, and in the graph when it is there; it can
//! come back as a branch, a cherry-pick or a checkout. This reaches what Ctrl+Z can't: what was
//! done in the terminal, in another client or before gitcraft was opened.

mod menu;
#[cfg(test)]
mod tests;
mod view;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::RepoTab;
use crate::git::{self, LostCommit, ReflogEntry};

/// Numbers the reads of every tab's view, so an answer for an older one is told apart.
static REQUESTS: AtomicU64 = AtomicU64::new(0);

fn next_request() -> u64 {
    REQUESTS.fetch_add(1, Ordering::Relaxed) + 1
}

/// What the view lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The reflog of `HEAD` or of a local branch (`refs/heads/x`).
    Ref(String),
    /// The commits the scan finds that nothing reaches.
    Lost,
}

impl Source {
    pub(crate) fn head() -> Self {
        Source::Ref("HEAD".to_owned())
    }

    /// The reflog of the local branch `name`.
    pub(crate) fn branch(name: &str) -> Self {
        Source::Ref(format!("refs/heads/{name}"))
    }
}

/// What the view asks for.
pub(crate) enum ReflogAct {
    Show(Source),
    /// The entry (or lost commit) at this place in the list.
    Select(usize),
    LoadMore,
    Reload,
    Scan,
    CancelScan,
}

/// A commit the view lists, whatever list it is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pick {
    pub(crate) hash: String,
    pub(crate) summary: String,
    pub(crate) merge: bool,
}

impl Pick {
    fn of_entry(e: &ReflogEntry) -> Self {
        Self {
            hash: e.new.clone(),
            summary: e.summary.clone(),
            merge: e.parents.len() > 1,
        }
    }

    fn of_lost(c: &LostCommit) -> Self {
        Self {
            hash: c.hash.clone(),
            summary: c.summary.clone(),
            merge: c.parents.len() > 1,
        }
    }
}

/// A scan for lost commits under way. Dropped (cancelled, the view closed, the tab closed),
/// it stops the git it runs.
struct Scan {
    cancel: git::Cancel,
    started: Instant,
    request: u64,
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct ReflogView {
    pub(super) source: Source,
    /// The reflog shown, newest first, as far as it is read.
    entries: Vec<ReflogEntry>,
    /// Older entries follow.
    more: bool,
    loading: bool,
    error: Option<String>,
    /// The latest read of the reflog: only its answer is taken.
    request: u64,
    /// What the last scan found.
    lost: Option<Result<Vec<LostCommit>, String>>,
    scan: Option<Scan>,
    /// The place of the entry (or lost commit) selected in the list shown.
    selected: Option<usize>,
    /// The refs the reflog was read with: once they move, it is read again.
    seen: RefsKey,
}

/// Where HEAD and the local branches are: what moves a reflog.
type RefsKey = (String, Option<String>, Vec<(String, String)>);

impl ReflogView {
    fn new(source: Source) -> Self {
        Self {
            source,
            entries: Vec::new(),
            more: false,
            loading: false,
            error: None,
            request: 0,
            lost: None,
            scan: None,
            selected: None,
            seen: Default::default(),
        }
    }

    fn lost_commits(&self) -> Option<&[LostCommit]> {
        self.lost.as_ref()?.as_ref().ok().map(Vec::as_slice)
    }

    /// The commit at place `i` of the list shown.
    fn pick(&self, i: usize) -> Option<Pick> {
        match self.source {
            Source::Ref(_) => self.entries.get(i).map(Pick::of_entry),
            Source::Lost => self.lost_commits()?.get(i).map(Pick::of_lost),
        }
    }

    pub(super) fn selected_pick(&self) -> Option<Pick> {
        self.pick(self.selected?)
    }
}

impl RepoTab {
    /// Opens the view on `source` (or switches it there), above the graph.
    pub(super) fn open_reflog(&mut self, source: Source, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            let editor = if self.conflict.is_some() {
                "conflict editor"
            } else {
                "interactive rebase"
            };
            self.status = Some((format!("Finish or close the {editor} first"), true));
            return;
        }
        // It sits on the graph: what covers that goes.
        self.close_pr();
        self.ci_view = None;
        self.file_view = None;
        self.diff = None;
        let view = self
            .reflog
            .get_or_insert_with(|| ReflogView::new(source.clone()));
        let switched = view.source != source;
        view.source = source;
        if switched {
            view.selected = None;
        }
        match &view.source {
            Source::Ref(_) if switched || view.entries.is_empty() => self.read_reflog(ctx),
            Source::Lost if view.lost.is_none() && view.scan.is_none() => self.start_scan(ctx),
            _ => {}
        }
    }

    pub(super) fn reflog_act(&mut self, act: ReflogAct, ctx: &egui::Context) {
        match act {
            ReflogAct::Show(source) => self.open_reflog(source, ctx),
            ReflogAct::Select(i) => self.select_in_reflog(i, ctx),
            ReflogAct::LoadMore => self.read_more_reflog(ctx),
            ReflogAct::Reload => match self.reflog.as_ref().map(|v| &v.source) {
                Some(Source::Lost) => {
                    if let Some(view) = &mut self.reflog {
                        view.scan = None;
                    }
                    self.start_scan(ctx);
                }
                Some(Source::Ref(_)) => self.read_reflog(ctx),
                None => {}
            },
            ReflogAct::Scan => self.start_scan(ctx),
            ReflogAct::CancelScan => {
                if let Some(view) = &mut self.reflog {
                    view.scan = None;
                }
            }
        }
    }

    fn refs_key(&self) -> RefsKey {
        (
            self.refs.head.clone(),
            self.refs.current.clone(),
            self.refs
                .local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect(),
        )
    }

    /// Reads the reflog shown again from its newest entry.
    fn read_reflog(&mut self, ctx: &egui::Context) {
        let seen = self.refs_key();
        let Some(view) = &mut self.reflog else { return };
        let Source::Ref(reference) = view.source.clone() else {
            return;
        };
        view.seen = seen;
        view.loading = true;
        view.error = None;
        view.request = next_request();
        let request = view.request;
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::reflog(&path, &reference, 0, git::REFLOG_PAGE),
            move |tab, page, _| tab.reflog_read(request, 0, page),
        );
    }

    /// Reads the next page of the reflog shown, unless one is being read or it's all here.
    fn read_more_reflog(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.reflog else { return };
        let Source::Ref(reference) = view.source.clone() else {
            return;
        };
        if view.loading || !view.more {
            return;
        }
        view.loading = true;
        view.request = next_request();
        let (request, skip) = (view.request, view.entries.len());
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::reflog(&path, &reference, skip, git::REFLOG_PAGE),
            move |tab, page, _| tab.reflog_read(request, skip, page),
        );
    }

    /// Takes in the page read from entry `skip` on, for the read numbered `request`.
    fn reflog_read(&mut self, request: u64, skip: usize, page: Result<git::ReflogPage, String>) {
        let Some(view) = &mut self.reflog else { return };
        if view.request != request {
            return;
        }
        view.loading = false;
        match page {
            Ok(page) => {
                let kept = view.selected.and_then(|i| view.entries.get(i)).cloned();
                if skip == 0 {
                    view.entries = page.entries;
                } else {
                    view.entries.truncate(skip);
                    view.entries.extend(page.entries);
                }
                view.more = page.more;
                // Entries are numbered from the newest: one added moves them all down.
                view.selected =
                    kept.and_then(|old| view.entries.iter().position(|e| same(e, &old)));
            }
            Err(e) => view.error = Some(e),
        }
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.reflog else { return };
        if view.scan.is_some() {
            return;
        }
        let cancel = git::Cancel::default();
        let request = next_request();
        view.scan = Some(Scan {
            cancel: cancel.clone(),
            started: Instant::now(),
            request,
        });
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::lost_commits(&path, &cancel),
            move |tab, found, _| tab.scan_done(request, found),
        );
    }

    fn scan_done(&mut self, request: u64, found: Result<Vec<LostCommit>, String>) {
        let Some(view) = &mut self.reflog else { return };
        // Cancelled, or another scan started since.
        if view.scan.as_ref().map(|s| s.request) != Some(request) {
            return;
        }
        view.scan = None;
        let kept = (view.source == Source::Lost)
            .then(|| view.selected_pick())
            .flatten();
        view.lost = Some(found);
        if view.source == Source::Lost {
            view.selected =
                kept.and_then(|old| view.lost_commits()?.iter().position(|c| c.hash == old.hash));
        }
    }

    /// Shows the commit at place `i` of the list: in the detail pane, and selected in the graph
    /// when it is there (a lost commit isn't).
    fn select_in_reflog(&mut self, i: usize, ctx: &egui::Context) {
        let Some(view) = &mut self.reflog else { return };
        let Some(pick) = view.pick(i) else { return };
        view.selected = Some(i);
        self.close_pr();
        self.ci_view = None;
        self.compare = None;
        self.file_view = None;
        self.stash_view = None;
        match self.index.get(&pick.hash).copied() {
            Some(row) => {
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            None => {
                self.selected = None;
                self.diff = None;
                if self.detail_hash.as_deref() != Some(pick.hash.as_str()) {
                    self.load_detail(pick.hash, ctx);
                }
            }
        }
    }

    /// The commit the detail pane shows for the view, while it shows one no graph row is (a
    /// reload then keeps it rather than going back to HEAD).
    pub(super) fn reflog_commit_off_graph(&self) -> Option<String> {
        let pick = self.reflog.as_ref()?.selected_pick()?;
        (self.selected.is_none() && self.detail_hash.as_deref() == Some(pick.hash.as_str()))
            .then_some(pick.hash)
    }

    /// After a reload: the reflog shown is read again if HEAD or a branch moved.
    pub(super) fn reflog_after_reload(&mut self, ctx: &egui::Context) {
        let key = self.refs_key();
        let Some(view) = &mut self.reflog else { return };
        if matches!(view.source, Source::Ref(_)) && view.seen != key {
            // A branch deleted meanwhile has no reflog left: back to HEAD's.
            if let Source::Ref(r) = &view.source
                && let Some(name) = r.strip_prefix("refs/heads/")
                && !key.2.iter().any(|(n, _)| n == name)
            {
                view.source = Source::head();
                view.selected = None;
            }
            self.read_reflog(ctx);
        }
    }
}

/// Whether two reads of a reflog found the same entry (its number moves as entries are added).
fn same(a: &ReflogEntry, b: &ReflogEntry) -> bool {
    a.new == b.new && a.time == b.time && a.message == b.message
}
