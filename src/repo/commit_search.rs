//! Commit search requests and their visible results share one lifetime.
use super::{Msg, RepoTab};
use crate::git;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
pub(super) struct Search {
    pub(super) open: bool,
    pub(super) focus: bool,
    /// Search what commits added or removed in files, rather than messages.
    pub(super) code: bool,
    pub(super) query: String,
    pub(super) searched: String,
    pub(super) results: Vec<String>,
    pub(super) matches: Arc<HashSet<String>>,
    pub(super) current: usize,
    pub(super) running: bool,
    pub(super) edited: Option<Instant>,
    pub(super) problem: Option<String>,
    pub(super) suggest: super::graph_view::suggest::SuggestState,
    pending: Option<Arc<Request>>,
}

/// Allocation identity survives A→B→A without counters or reusable query keys.
/// Jobs hold the request, while Search alone owns retirement of its token.
pub(super) struct Request {
    path: std::path::PathBuf,
    query: String,
    code: bool,
    from: Option<Vec<String>>,
    order: git::LogOrder,
    excluded: Vec<String>,
    connection: crate::connections::Context,
    connection_generation: u64,
    cancel: crate::tasks::Cancellation,
}

impl Search {
    pub(super) fn retire(&mut self) {
        if let Some(request) = self.pending.take() {
            request.cancel.cancel();
        }
        self.running = false;
    }

    #[cfg(test)]
    pub(super) fn request(&self) -> Option<Arc<Request>> {
        self.pending.clone()
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.retire();
    }
}

impl Request {
    fn current(&self, tab: &RepoTab, ctx: &egui::Context) -> bool {
        !self.cancel.is_cancelled()
            && tab.search.open
            && self.path == tab.path
            && self.query == tab.search.query.trim()
            && self.code == tab.search.code
            && self.from == tab.log_from
            && self.order == tab.log_order
            && self.excluded == super::history_filter::rules(ctx, tab.repo_key()).excludes()
            && self.connection == crate::connections::current()
            && self.connection_generation != u64::MAX
            && self.connection_generation == crate::connections::generation()
    }
}

impl RepoTab {
    pub(super) fn run_search(&mut self, ctx: &egui::Context) {
        self.search.retire();
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
            self.history_minimap.dirty = true;
            self.search.results.clear();
            self.search.matches = Default::default();
            self.search.searched.clear();
            return;
        };
        if !self.search.open {
            return;
        }
        let request = Arc::new(Request {
            path: self.path.clone(),
            query,
            code,
            from: self.log_from.clone(),
            order: self.log_order,
            excluded: super::history_filter::rules(ctx, self.repo_key()).excludes(),
            connection: crate::connections::current(),
            connection_generation: crate::connections::generation(),
            cancel: self.tx.cancel.child(),
        });
        self.search.pending = Some(request.clone());
        self.search.running = true;
        let mut sender = self.tx.clone();
        sender.cancel = request.cancel.clone();
        // Answered all the same if it panics: the search runs until it is.
        let failed = {
            let request = request.clone();
            move |e| Msg::Search(request, Err(e))
        };
        super::messages::spawn_or(&sender, ctx, failed, move || {
            let result = (|| {
                let roots = git::history_scope::search_roots(
                    &request.path,
                    request.from.as_deref(),
                    &request.excluded,
                )?;
                git::search_query::run_with_order(&request.path, Some(&roots), request.order, &plan)
            })();
            Msg::Search(request, result)
        });
    }

    /// Takes in the commits a search found (if it's still the search asked for), and goes to
    /// the first.
    pub(super) fn search_done(
        &mut self,
        request: Arc<Request>,
        result: Result<Vec<String>, String>,
        ctx: &egui::Context,
    ) {
        if !self
            .search
            .pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, &request))
        {
            return;
        }
        if !request.current(self, ctx) {
            self.search.retire();
            return;
        }
        self.search.retire();
        self.history_minimap.dirty = true;
        self.search.searched = request.query.clone();
        match result {
            Ok(hashes) => {
                self.search.problem = None;
                self.search.matches = hashes
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>()
                    .into();
                self.search.results = hashes;
                self.search.current = 0;
                if let Some(first) = self.search.results.first().cloned() {
                    self.goto(&first, ctx);
                }
            }
            Err(e) => {
                self.search.results.clear();
                self.search.matches = Default::default();
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
        self.history_minimap.dirty = true;
        self.search.open = !self.search.open;
        self.search.focus = self.search.open;
        if !self.search.open {
            self.authorship.filter = false;
            self.search.retire();
            self.history_minimap.dirty = true;
            // Nor does a query typed just before closing start a search once it's closed.
            self.search.edited = None;
            self.search.matches = Default::default();
            self.search.results.clear();
            self.search.searched.clear();
            self.search.problem = None;
            // Read again next time: history and files may have changed by then.
            self.search.suggest.reset();
        }
    }
}

#[cfg(test)]
mod tests;
