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
    /// Show the lost commits, scanning for them unless that was done already.
    FindLost,
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
    /// A dropped stash, applied back rather than made a branch of.
    pub(crate) stash: bool,
}

impl Pick {
    fn of_entry(e: &ReflogEntry) -> Self {
        Self {
            hash: e.new.clone(),
            summary: e.summary.clone(),
            merge: e.parents.len() > 1,
            stash: false,
        }
    }

    fn of_lost(c: &LostCommit) -> Self {
        Self {
            hash: c.hash.clone(),
            summary: c.summary.clone(),
            merge: c.parents.len() > 1,
            stash: c.stash,
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
    /// The scan that was cancelled or let go of, while its thread still runs: over SSH or in
    /// WSL, cancelling ends the connection, but git there may go on until it is done, so no
    /// other scan is started beside it.
    ending: std::sync::Weak<()>,
}

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
            ending: std::sync::Weak::new(),
        }
    }

    pub(in crate::repo) fn lost_commits(&self) -> Option<&[LostCommit]> {
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
                crate::i18n::text("repo-workflows-reflog-open-reflog-d509b2ee")
            } else {
                crate::i18n::text("repo-workflows-reflog-open-reflog-680d5c5f")
            };
            self.status = Some((format!("Finish or close the {editor} first"), true));
            return;
        }
        let switched = self.reflog.as_ref().is_some_and(|v| v.source != source);
        if switched {
            self.leave_off_graph_commit(ctx);
        }
        // It sits on the graph: what covers that goes.
        self.close_pr();
        self.ci_view = None;
        self.file_view = None;
        self.diff = None;
        let view = self
            .reflog
            .get_or_insert_with(|| ReflogView::new(source.clone()));
        view.source = source;
        if switched {
            view.selected = None;
            // Another reflog's entries are not shown under this one's name meanwhile.
            view.entries.clear();
            view.more = false;
        }
        // The lost commits wait for Find lost commits: the scan reads every object.
        if matches!(view.source, Source::Ref(_)) && (switched || view.entries.is_empty()) {
            self.read_reflog(ctx);
        }
    }

    /// Closes the view; a commit it showed that the graph doesn't have leaves the detail pane.
    pub(super) fn close_reflog(&mut self, ctx: &egui::Context) {
        self.leave_off_graph_commit(ctx);
        self.reflog = None;
    }

    /// When the detail pane shows a commit of the view that no graph row is, the graph's own
    /// selection again: the changes, or HEAD.
    fn leave_off_graph_commit(&mut self, ctx: &egui::Context) {
        if self.reflog_commit_off_graph().is_none() {
            return;
        }
        if let Some(view) = &mut self.reflog {
            view.selected = None;
        }
        let row = self
            .index
            .get(super::WIP_HASH)
            .or_else(|| self.index.get(&self.refs.head))
            .copied();
        match row {
            Some(row) => {
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            None => {
                self.detail = None;
                self.detail_hash = None;
            }
        }
    }

    pub(super) fn reflog_act(&mut self, act: ReflogAct, ctx: &egui::Context) {
        match act {
            ReflogAct::Show(source) => self.open_reflog(source, ctx),
            ReflogAct::FindLost => {
                self.open_reflog(Source::Lost, ctx);
                if self.reflog.as_ref().is_some_and(|v| v.lost.is_none()) {
                    self.start_scan(ctx);
                }
            }
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

    /// Reads the reflog shown again from its newest entry.
    fn read_reflog(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.reflog else { return };
        let Source::Ref(reference) = view.source.clone() else {
            return;
        };
        view.loading = true;
        view.error = None;
        view.request = next_request();
        let request = view.request;
        // As far as it was read: a reload doesn't send a deep-scrolled list back to its top.
        let count = view.entries.len().max(git::REFLOG_PAGE);
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::reflog(&path, &reference, 0, count),
            move |tab, page, ctx| tab.reflog_read(request, 0, page, ctx),
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
            move |tab, page, ctx| tab.reflog_read(request, skip, page, ctx),
        );
    }

    /// Takes in the page read from entry `skip` on, for the read numbered `request`.
    fn reflog_read(
        &mut self,
        request: u64,
        skip: usize,
        page: Result<git::ReflogPage, String>,
        ctx: &egui::Context,
    ) {
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
                } else if !follows(&view.entries[..skip.min(view.entries.len())], &page.entries) {
                    // Entries were added or expired since the pages before were read: those
                    // after them are numbered otherwise now. Read it all again.
                    view.entries.truncate(skip);
                    self.read_reflog(ctx);
                    return;
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
        let remote = crate::remote::is_remote(&self.path);
        let Some(view) = &mut self.reflog else { return };
        if view.scan.is_some() {
            return;
        }
        if remote && view.ending.upgrade().is_some() {
            self.status = Some((
                "The last scan is still ending on that machine; try again in a moment".to_owned(),
                true,
            ));
            return;
        }
        let running = std::sync::Arc::new(());
        view.ending = std::sync::Arc::downgrade(&running);
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
            move || {
                let found = git::lost_commits(&path, &cancel);
                drop(running);
                found
            },
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

    /// After a reload: the reflog shown is read again (most commands add to it, some without
    /// moving any ref), and lost commits the graph now has (made a branch of…) are no longer
    /// listed.
    pub(super) fn reflog_after_reload(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.reflog else { return };
        if let Some(Ok(lost)) = &mut view.lost {
            let kept = (view.source == Source::Lost)
                .then(|| {
                    view.selected
                        .and_then(|i| lost.get(i))
                        .map(|c| c.hash.clone())
                })
                .flatten();
            lost.retain(|c| !self.index.contains_key(&c.hash));
            if view.source == Source::Lost {
                view.selected = kept.and_then(|h| lost.iter().position(|c| c.hash == h));
            }
        }
        if let Source::Ref(r) = &view.source {
            // A branch deleted meanwhile has no reflog left: back to HEAD's.
            if let Some(name) = r.strip_prefix("refs/heads/")
                && !self.refs.local.iter().any(|b| b.name == name)
            {
                view.source = Source::head();
                view.selected = None;
                view.entries.clear();
            }
            self.read_reflog(ctx);
        }
    }
}

/// Whether `page` is the page after `shown`: its first entry numbered after their last one, and
/// it is where that one came from when that is known.
fn follows(shown: &[ReflogEntry], page: &[ReflogEntry]) -> bool {
    let (Some(last), Some(first)) = (shown.last(), page.first()) else {
        return true;
    };
    let number = |e: &ReflogEntry| git::reflog_number(&e.selector);
    let after = matches!((number(last), number(first)), (Some(a), Some(b)) if b > a);
    after && last.old.as_ref().is_none_or(|old| *old == first.new)
}

/// Whether two reads of a reflog found the same entry (its number moves as entries are added).
fn same(a: &ReflogEntry, b: &ReflogEntry) -> bool {
    a.new == b.new && a.time == b.time && a.message == b.message
}

#[cfg(test)]
mod follow_tests {
    use super::follows;
    use crate::git::{ReflogEntry, parse_message};

    fn entry(n: usize, new: &str, old: Option<&str>) -> ReflogEntry {
        ReflogEntry {
            selector: format!("HEAD@{{{n}}}"),
            time: 0,
            date: String::new(),
            new: new.to_owned(),
            old: old.map(str::to_owned),
            parents: Vec::new(),
            message: parse_message("commit: x"),
            summary: String::new(),
        }
    }

    /// A page read after entries were added on top starts with one already shown.
    #[test]
    fn a_page_follows_only_where_the_last_one_ended() {
        let shown = [entry(0, "a", Some("b")), entry(1, "b", Some("c"))];
        assert!(follows(&shown, &[entry(2, "c", None)]));
        // Renumbered: the entry after is shown again.
        assert!(!follows(&shown, &[entry(1, "b", None)]));
        // Expired in between: not where the last one came from.
        assert!(!follows(&shown, &[entry(2, "d", None)]));
        assert!(follows(&[], &[entry(5, "x", None)]));
        assert!(follows(&shown, &[]));
    }
}
