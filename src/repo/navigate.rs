//! Selecting commits and stashes, opening diffs, comparisons and file views, and the commit
//! search.

use super::{
    Compare, CompareData, DiffKey, DiffView, MAX_HISTORY, Msg, RepoTab, WIP_HASH, conflict, diff,
    file_text, file_view, spawn,
};
use crate::git;
use crate::settings::Settings;
use crate::theme::Tokens;

impl RepoTab {
    pub(super) fn is_wip(&self, row: usize) -> bool {
        self.commits.get(row).is_some_and(|c| c.hash == WIP_HASH)
    }

    /// The stash a graph row stands for, if it's one.
    pub(super) fn stash_at(&self, row: usize) -> Option<usize> {
        let hash = &self.commits.get(row)?.hash;
        self.stashes.iter().position(|s| &s.hash == hash)
    }

    pub(super) fn selected_hash(&self) -> Option<String> {
        self.selected
            .and_then(|r| self.commits.get(r))
            .map(|c| c.hash.clone())
    }

    pub(super) fn select(&mut self, row: usize, ctx: &egui::Context) {
        let Some(commit) = self.commits.get(row) else {
            return;
        };
        let stash = self.stash_at(row);
        if self.selected == Some(row) && self.stash_view == stash {
            return;
        }
        let hash = commit.hash.clone();
        // Below the graph, the diff stays and follows what is opened next.
        let keep_diff = self.diff_beside_graph(ctx);
        self.selected = Some(row);
        self.stash_view = stash;
        self.file_view = None;
        self.compare = None;
        if !keep_diff
            && (matches!(
                self.diff,
                Some(DiffView {
                    key: DiffKey::Commit { .. } | DiffKey::Range { .. } | DiffKey::Remerge { .. },
                    ..
                })
            ) || hash != WIP_HASH)
        {
            self.diff = None;
            self.conflict = None;
        }
        if hash == WIP_HASH {
            self.detail = None;
            self.detail_hash = None;
            self.drop_extras();
            return;
        }
        self.load_detail(hash, ctx);
    }

    pub(super) fn load_detail(&mut self, hash: String, ctx: &egui::Context) {
        self.detail = None;
        self.reword = None;
        self.detail_error = None;
        self.detail_hash = Some(hash.clone());
        self.load_extras(&hash, ctx);
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let detail = git::commit_detail(&path, &hash);
            Msg::Detail(hash, detail)
        });
    }

    pub(super) fn select_stash(&mut self, i: usize, ctx: &egui::Context) {
        let Some(stash) = self.stashes.get(i) else {
            return;
        };
        let hash = stash.hash.clone();
        self.stash_view = Some(i);
        self.selected = self.index.get(&hash).copied();
        self.scroll_to = self.selected;
        self.file_view = None;
        self.compare = None;
        self.diff = None;
        self.conflict = None;
        self.load_detail(hash, ctx);
    }

    pub(super) fn select_hash(&mut self, hash: &str, ctx: &egui::Context) {
        match self.index.get(hash).copied() {
            Some(row) => {
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            None => {
                self.status = Some((
                    format!(
                        "Commit {} is not in the loaded history",
                        hash.get(..8).unwrap_or(hash)
                    ),
                    true,
                ));
            }
        }
    }

    /// Selects `hash`, loading more history first if it's further down.
    pub(super) fn goto(&mut self, hash: &str, ctx: &egui::Context) {
        // Nothing read to look in yet: once it is (see `rebuild`).
        if self.sleeping() || self.resuming {
            self.pending_goto = Some(hash.to_owned());
            return;
        }
        if self.index.contains_key(hash) {
            self.pending_goto = None;
            self.select_hash(hash, ctx);
        } else if self.has_more && self.history.len() < MAX_HISTORY {
            self.pending_goto = Some(hash.to_owned());
            self.load_more(ctx);
        } else {
            self.pending_goto = None;
            self.select_hash(hash, ctx);
        }
    }

    /// Shows a diff (or the conflict editor) in the central area; asking for the open one
    /// closes it.
    pub(super) fn open_diff(&mut self, key: DiffKey, ctx: &egui::Context) {
        self.open_diff_of_lines(key, None, ctx);
    }

    /// [`Self::open_diff`]; with `lines`, only the part of the commit's diff that changed those
    /// lines (see [`DiffView::lines`]).
    pub(super) fn open_diff_of_lines(
        &mut self,
        key: DiffKey,
        lines: Option<Vec<git::LineRange>>,
        ctx: &egui::Context,
    ) {
        if self
            .diff
            .as_ref()
            .is_some_and(|d| d.key == key && d.lines == lines)
        {
            self.diff = None;
            self.conflict = None;
            return;
        }
        // A new opening: what is still on its way for an earlier one is not for this one.
        self.conflict_generation += 1;
        let generation = self.conflict_generation;
        self.conflict = match &key {
            // Elsewhere, read in the background (see `conflict_opened`).
            DiffKey::Work(file)
                if file.area == git::Area::Conflicted && crate::remote::is_remote(&self.path) =>
            {
                let (repo, path, state) =
                    (self.path.clone(), file.path.clone(), self.work.in_progress);
                let certain = self.rerere.reused.contains(&path);
                // Not rebuilt again when it couldn't be since the last operation.
                let skip = self.rerere.unopenable.contains(&path);
                spawn(&self.tx, ctx, move || {
                    let editor = conflict::ConflictEditor::open(&repo, &path, state);
                    if editor.is_some() || skip {
                        return Msg::ConflictOpened(generation, path, editor.map(Box::new));
                    }
                    // Its markers gone: what stands in their place (rerere's resolution).
                    let found = conflict::rerere::open_reused(&repo, &path, state, certain);
                    Msg::Then(Box::new(move |tab, _| {
                        tab.reused_opened(generation, path, found);
                    }))
                });
                None
            }
            DiffKey::Work(file) if file.area == git::Area::Conflicted => {
                let editor =
                    conflict::ConflictEditor::open(&self.path, &file.path, self.work.in_progress);
                if editor.is_none() {
                    self.open_reused_later(generation, file.path.clone(), ctx);
                }
                editor.map(|mut ed| {
                    ed.generation = generation;
                    ed
                })
            }
            _ => None,
        };
        let mut view = DiffView::new(key);
        view.lines = lines;
        self.diff = Some(view);
        self.refresh_diff(ctx);
    }

    /// The conflict editor of `path`, read in the background for the opening numbered
    /// `generation`, if that is still the latest and its diff the one open: one read for an
    /// opening closed since would replace (or reset) the editor opened after it.
    pub(super) fn conflict_opened(
        &mut self,
        generation: u64,
        path: String,
        editor: Option<Box<conflict::ConflictEditor>>,
    ) {
        let open = matches!(&self.diff, Some(DiffView { key: DiffKey::Work(file), .. })
            if file.path == path && file.area == git::Area::Conflicted);
        if open && generation == self.conflict_generation {
            self.conflict = editor.map(|e| {
                let mut ed = *e;
                ed.generation = generation;
                ed
            });
        }
    }

    /// The file of the conflict editor opened as `generation` changed on disk (seen in the
    /// background): opened again from what is there now, if that editor is still the one open.
    pub(super) fn conflict_changed(&mut self, generation: u64, path: String, ctx: &egui::Context) {
        if self
            .conflict
            .as_ref()
            .is_none_or(|ed| ed.path != path || ed.generation != generation || ed.saving)
        {
            return;
        }
        let Some(DiffView {
            key: DiffKey::Work(file),
            ..
        }) = &self.diff
        else {
            return;
        };
        let key = DiffKey::Work(file.clone());
        self.diff = None;
        self.conflict = None;
        self.open_diff(key, ctx);
    }

    /// Reads the open diff again, showing the old one meanwhile.
    pub(super) fn refresh_diff(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.diff else { return };
        let request = view.next_request();
        let key = view.key.clone();
        let lines = view.lines.clone();
        let path = self.path.clone();
        let settings = Settings::get(ctx);
        let opts = git::DiffOptions {
            ignore_ws: self.diff_ignore_ws,
            whole_file: settings.diff_whole_file,
            algorithm: diff::algorithm_for(&settings, key.path()),
            no_textconv: false,
        };
        view.algorithm = opts.algorithm;
        let dark = Tokens::get(ctx).dark();
        spawn(&self.tx, ctx, move || {
            let result = diff::load_lines(&path, &key, lines.as_deref(), opts, dark).map(Box::new);
            Msg::Diff(request, result)
        });
        self.load_diff_file(ctx);
        self.load_diff_rendered(ctx);
    }

    /// Renders the Markdown file of the open diff, when diffs show Markdown rendered.
    pub(super) fn load_diff_rendered(&mut self, ctx: &egui::Context) {
        let rendered = self.diff_rendered;
        let Some(view) = self.diff.as_mut().filter(|v| rendered && v.key.renders()) else {
            return;
        };
        view.rendered_load += 1;
        let ids = (view.request, view.rendered_load);
        let (key, path) = (view.key.clone(), self.path.clone());
        spawn(&self.tx, ctx, move || {
            let result = diff::rendered::load(&path, &key);
            Msg::DiffRendered(ids, Box::new(result))
        });
    }

    /// Shows Markdown files rendered in diffs, or as text.
    pub(super) fn show_diff_rendered(&mut self, on: bool, ctx: &egui::Context) {
        self.diff_rendered = on;
        if on {
            self.diff_rich = None;
        }
        // Read again each time it's turned on: the file may have changed since.
        if on {
            self.load_diff_rendered(ctx);
        }
    }

    /// Reads the whole file of the open diff, when diffs show the file.
    pub(super) fn load_diff_file(&mut self, ctx: &egui::Context) {
        let Some(view) = self.diff.as_ref().filter(|_| self.diff_as_file) else {
            return;
        };
        let (request, key, path) = (view.request, view.key.clone(), self.path.clone());
        let dark = Tokens::get(ctx).dark();
        spawn(&self.tx, ctx, move || {
            let result = file_text::load_for_diff(&path, &key, dark);
            Msg::DiffFile(request, result)
        });
    }

    /// Shows diffs as the whole file, or as the changes.
    pub(super) fn show_diff_as_file(&mut self, on: bool, ctx: &egui::Context) {
        self.diff_as_file = on;
        if self.diff.as_ref().is_some_and(|d| d.file.is_none()) {
            self.load_diff_file(ctx);
        }
    }

    pub(super) fn reopen_diff(&mut self, ctx: &egui::Context) {
        if let Some(view) = self.diff.take() {
            self.conflict = None;
            self.open_diff(view.key, ctx);
            // A folded file opened stays open.
            if let Some(reopened) = &mut self.diff {
                reopened.unfolded = view.unfolded;
            }
        }
    }

    /// Closes the diff of a file between two commits (opened from a comparison).
    pub(super) fn close_range_diff(&mut self) {
        if matches!(
            self.diff,
            Some(DiffView {
                key: DiffKey::Range { .. },
                ..
            })
        ) {
            self.diff = None;
        }
    }

    /// Starts comparing two points of history (replacing the comparison shown).
    pub(super) fn compare(&mut self, compare: Compare, ctx: &egui::Context) {
        self.pr_selection_request = None;
        self.close_pr();
        self.close_range_diff();
        self.file_view = None;
        self.compare = Some(compare);
        self.reload_compare(ctx);
    }

    /// Reads the comparison's files again (the working tree or a branch moved).
    pub(super) fn reload_compare(&mut self, ctx: &egui::Context) {
        // Background file-system refreshes must not replace the request still resolving a
        // PR's exact endpoints with a comparison against stale tracking refs.
        if self
            .compare
            .as_ref()
            .is_some_and(|c| self.pr_selection_request == Some(c.request))
        {
            return;
        }
        self.compare_requests += 1;
        let request = self.compare_requests;
        let Some(c) = &mut self.compare else { return };
        c.request = request;
        let (path, base, target, since_fork) = (
            self.path.clone(),
            c.base.clone(),
            c.target.clone(),
            c.since_fork,
        );
        spawn(&self.tx, ctx, move || {
            let result = (|| {
                let base = git::resolve(&path, &base)?;
                let to = target.map(|t| git::resolve(&path, &t)).transpose()?;
                // The working tree goes with HEAD's history.
                let other = match &to {
                    Some(to) => to.clone(),
                    None => git::resolve(&path, "HEAD")?,
                };
                let from = if since_fork {
                    git::merge_base(&path, &base, &other)?
                } else {
                    base.clone()
                };
                Ok(CompareData {
                    files: git::diff_files(&path, &from, to.as_deref())?,
                    apart: git::count_apart(&path, &base, &other)?,
                    from,
                    to,
                })
            })();
            Msg::Compare(request, result)
        });
    }

    /// Swaps the comparison's two ends.
    pub(super) fn swap_compare(&mut self, ctx: &egui::Context) {
        // The working tree stays the newer end.
        if let Some(c) = self.compare.take_if(|c| c.target.is_some())
            && let Some(target) = c.target
        {
            self.compare(
                Compare {
                    base: target,
                    base_label: c.target_label,
                    target: Some(c.base),
                    target_label: c.base_label,
                    data: None,
                    ..c
                },
                ctx,
            );
        }
    }

    /// Shows `path`'s history or blame, loading what that tab needs.
    pub(super) fn open_file_view(
        &mut self,
        path: String,
        rev: Option<String>,
        tab: file_view::FileTab,
        ctx: &egui::Context,
    ) {
        self.diff = None;
        self.conflict = None;
        let view = match self.file_view.take() {
            // Not one showing the history of some lines only.
            Some(mut v) if v.path == path && v.rev == rev && v.lines.is_none() => {
                v.tab = tab;
                v
            }
            _ => file_view::FileView::new(&path, rev.clone(), tab),
        };
        let needs = view.needs(tab);
        self.file_view = Some(view);
        if !needs {
            return;
        }
        let repo = self.path.clone();
        match tab {
            file_view::FileTab::History => spawn(&self.tx, ctx, move || {
                let result = git::file_history(&repo, rev.as_deref(), &path);
                Msg::FileHistory(path, rev, result)
            }),
            file_view::FileTab::Blame => {
                let dark = Tokens::get(ctx).dark();
                spawn(&self.tx, ctx, move || {
                    let result = file_view::load_blame(&repo, rev.as_deref(), &path, dark);
                    Msg::Blame(path, rev, result)
                })
            }
            file_view::FileTab::File => {
                let dark = Tokens::get(ctx).dark();
                spawn(&self.tx, ctx, move || {
                    let result = file_text::load(&repo, rev.as_deref(), &path, dark);
                    Msg::FileText(path, rev, result)
                })
            }
        }
    }

    /// Reads every file of commit `hash`, for the commit pane's "View all files".
    pub(super) fn load_tree_files(&mut self, hash: String, ctx: &egui::Context) {
        self.all_files.wait_for(&hash);
        let repo = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::tree_files(&repo, &hash);
            Msg::TreeFiles(hash, result)
        });
    }

    pub(super) fn run_search(&mut self, ctx: &egui::Context) {
        let query = self.search.query.trim().to_owned();
        self.search.edited = None;
        self.search.problem = None;
        let code = self.search.code;
        let plan = if query.is_empty() {
            None
        } else {
            match git::search_query::prepare(&query, code) {
                Ok(plan) => Some(plan),
                Err(e) => {
                    self.search.problem = Some(e.message);
                    None
                }
            }
        };
        let Some(plan) = plan else {
            // One still being read answers a query that is gone (or can't be searched).
            self.search.running = false;
            self.search.results.clear();
            self.search.matches.clear();
            self.search.searched.clear();
            return;
        };
        self.search.running = true;
        let path = self.path.clone();
        // In the branches the graph shows.
        let from = self.log_from.clone();
        let order = self.log_order;
        // Answered all the same if it panics: the search runs until it is.
        let failed = {
            let query = query.clone();
            move |e| Msg::Search(query, code, Err(e))
        };
        super::messages::spawn_or(&self.tx, ctx, failed, move || {
            let result = git::search_query::run_with_order(&path, from.as_deref(), order, &plan);
            Msg::Search(query, code, result)
        });
    }

    /// Takes in the commits a search found (if it's still the search asked for), and goes to
    /// the first.
    pub(super) fn search_done(
        &mut self,
        query: String,
        code: bool,
        result: Result<Vec<String>, String>,
        ctx: &egui::Context,
    ) {
        // Closed since, the search shows nothing (nor jumps to its first find).
        if !self.search.open || query != self.search.query.trim() || code != self.search.code {
            return;
        }
        self.search.running = false;
        self.search.searched = query;
        match result {
            Ok(hashes) => {
                self.search.problem = None;
                self.search.matches = hashes.iter().cloned().collect();
                self.search.results = hashes;
                self.search.current = 0;
                if let Some(first) = self.search.results.first().cloned() {
                    self.goto(&first, ctx);
                }
            }
            Err(e) => {
                self.search.results.clear();
                self.search.matches.clear();
                self.search.problem = Some(e);
            }
        }
    }

    pub(super) fn search_step(&mut self, delta: i32, ctx: &egui::Context) {
        let n = self.search.results.len();
        if n == 0 {
            return;
        }
        let cur = self.search.current as i32 + delta;
        self.search.current = cur.rem_euclid(n as i32) as usize;
        if let Some(hash) = self.search.results.get(self.search.current).cloned() {
            self.goto(&hash, ctx);
        }
    }

    pub(super) fn toggle_search(&mut self) {
        self.search.open = !self.search.open;
        self.search.focus = self.search.open;
        if !self.search.open {
            self.authorship.filter = false;
            self.search.running = false;
            // Nor does a query typed just before closing start a search once it's closed.
            self.search.edited = None;
            self.search.matches.clear();
            self.search.results.clear();
            self.search.searched.clear();
            self.search.problem = None;
            // Read again next time: history and files may have changed by then.
            self.search.suggest.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RepoTab;

    fn tab() -> RepoTab {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/search-test".into(), &ctx);
        tab.search.open = true;
        tab
    }

    /// Clearing the query while a search runs: its answer is for a query that is gone, and
    /// "searching…" does not stay up.
    #[test]
    fn clearing_the_query_ends_the_search_running() {
        let ctx = egui::Context::default();
        let mut tab = tab();
        tab.search.query = "foo".to_owned();
        tab.search.running = true;
        tab.search.query.clear();
        tab.run_search(&ctx);
        tab.search_done("foo".to_owned(), false, Ok(vec!["a".to_owned()]), &ctx);
        assert!(!tab.search.running);
        assert!(tab.search.results.is_empty());
    }

    /// A search closed before it answered shows no finds.
    #[test]
    fn a_closed_search_takes_no_answer() {
        let ctx = egui::Context::default();
        let mut tab = tab();
        tab.search.query = "foo".to_owned();
        tab.search.running = true;
        tab.toggle_search();
        tab.search_done("foo".to_owned(), false, Ok(vec!["a".to_owned()]), &ctx);
        assert!(!tab.search.running);
        assert!(tab.search.matches.is_empty());
        assert!(tab.pending_goto.is_none());
    }

    /// An operator written wrong is explained under the field, and nothing is searched (nor
    /// left from the search before).
    #[test]
    fn a_query_written_wrong_says_why_and_searches_nothing() {
        let ctx = egui::Context::default();
        let mut tab = tab();
        tab.search.results = vec!["a".to_owned()];
        tab.search.matches.insert("a".to_owned());
        tab.search.query = "fix author:".to_owned();
        tab.run_search(&ctx);
        assert!(!tab.search.running);
        assert!(tab.search.results.is_empty() && tab.search.matches.is_empty());
        let problem = tab.search.problem.clone().unwrap_or_default();
        assert!(problem.contains("author: needs a name"), "{problem}");
        // Written right, the hint goes and the search runs.
        tab.search.query = "fix author:bob".to_owned();
        tab.run_search(&ctx);
        assert!(tab.search.running);
        assert!(tab.search.problem.is_none());
    }

    /// What git says when a search fails (a regex it can't read) shows under the field.
    #[test]
    fn a_failed_search_says_why_under_the_field() {
        let ctx = egui::Context::default();
        let mut tab = tab();
        tab.search.results = vec!["a".to_owned()];
        tab.search.matches.insert("a".to_owned());
        tab.search.query = "regex:[".to_owned();
        tab.search.running = true;
        let e = "git could not search: invalid regex".to_owned();
        tab.search_done("regex:[".to_owned(), false, Err(e.clone()), &ctx);
        assert!(tab.search.results.is_empty() && tab.search.matches.is_empty());
        // Answered right afterwards, the hint goes.
        tab.search.running = true;
        tab.search_done("regex:[".to_owned(), false, Ok(Vec::new()), &ctx);
        assert!(tab.search.problem.is_none());
        tab.search_done("regex:[".to_owned(), false, Err(e.clone()), &ctx);
        assert_eq!(tab.search.problem, Some(e));
        assert!(!tab.search.running);
        assert!(tab.status.is_none());
        tab.toggle_search();
        assert!(tab.search.problem.is_none());
    }
}
