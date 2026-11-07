//! Repository-scoped saved graph relations and their display-only projection.
mod projection;
mod reader;
#[cfg(test)]
mod tests;
mod view;
use super::RepoTab;
use crate::git::{
    self,
    investigations::views::{self, Kind, Preset, Scope, Selection},
};
pub(super) use projection::Row;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    version: u32,
    saved: Vec<Preset>,
    active: Option<Preset>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            saved: Vec::new(),
            active: None,
        }
    }
}
impl Preferences {
    fn normalize(mut self) -> Self {
        if self.version != 1 {
            return Self::default();
        }
        self.saved.retain(Preset::valid);
        self.saved.truncate(32);
        if self.active.as_ref().is_some_and(|p| !p.valid()) {
            self.active = None;
        }
        self
    }
}
enum Answer {
    Selection(Selection),
    Page(u64, Vec<git::Commit>),
}
pub(super) struct State {
    loaded: bool,
    prefs: Preferences,
    name: String,
    reader: reader::Reader<Answer>,
    scope: Option<Scope>,
    selection: Option<Selection>,
    pub error: Option<String>,
    pub dirty: bool,
    expanded: BTreeSet<String>,
    projection: projection::Projection,
    selected: Option<usize>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            loaded: false,
            prefs: Preferences::default(),
            name: String::new(),
            reader: Default::default(),
            scope: None,
            selection: None,
            error: None,
            dirty: true,
            expanded: BTreeSet::new(),
            projection: Default::default(),
            selected: None,
        }
    }
}
impl State {
    pub fn active(&self) -> bool {
        self.prefs.active.is_some()
    }
    pub fn cancel(&mut self) {
        self.reader.cancel();
        self.scope = None;
        self.selection = None;
        self.error = None;
        self.dirty = true;
    }
    pub fn len(&self) -> usize {
        self.projection.rows.len()
    }
    pub fn row(&self, i: usize) -> Option<&Row> {
        self.projection.rows.get(i)
    }
    pub fn position(&self, raw: usize) -> Option<usize> {
        if self.active() {
            self.projection.position(raw)
        } else {
            Some(raw)
        }
    }
    pub fn next(&self, raw: usize, down: bool) -> Option<usize> {
        self.projection.next(raw, down)
    }
}
fn preferences_id(path: &std::path::Path) -> egui::Id {
    egui::Id::new(("saved-graph-relations-v1", path))
}
impl RepoTab {
    fn save_graph_preferences(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_persisted(
                preferences_id(self.repo_key()),
                self.saved_views.prefs.clone(),
            )
        });
    }
    fn set_graph_view(&mut self, preset: Option<Preset>, ctx: &egui::Context) {
        self.saved_views.cancel();
        self.saved_views.prefs.active = preset;
        self.history_minimap.dirty = true;
        self.saved_views.expanded.clear();
        self.saved_views.error = None;
        self.save_graph_preferences(ctx);
        self.poll_saved_views(ctx);
    }
    fn graph_view_scope(&self) -> Scope {
        // All inputs to Git are immutable IDs from the graph as loaded, never ref names.
        let parents: BTreeSet<_> = self
            .history
            .iter()
            .flat_map(|c| c.parents.iter().map(String::as_str))
            .collect();
        let mut tips: Vec<_> = self
            .history
            .iter()
            .filter(|c| git::investigations::oid(&c.hash) && !parents.contains(c.hash.as_str()))
            .map(|c| c.hash.clone())
            .collect();
        tips.sort();
        tips.dedup();
        let mut remotes: Vec<_> = self
            .refs
            .remotes
            .iter()
            .flat_map(|(_, branches)| branches.iter())
            .filter(|b| git::investigations::oid(&b.target))
            .map(|b| b.target.clone())
            .collect();
        remotes.sort();
        remotes.dedup();
        let wanted = match self.saved_views.prefs.active.as_ref().map(|p| &p.kind) {
            Some(Kind::MergesInto { branch }) => Some(branch.as_str()),
            _ => None,
        };
        let default_branch = wanted
            .and_then(|name| self.refs.local.iter().find(|b| b.name == name))
            .map(|b| (b.name.clone(), b.target.clone()));
        Scope {
            tips,
            remotes,
            email: self.identity.email.clone(),
            default_branch,
            now: self
                .saved_views
                .scope
                .as_ref()
                .map_or_else(git::now, |s| s.now),
        }
    }
    pub(super) fn poll_saved_views(&mut self, ctx: &egui::Context) {
        if !self.shown {
            if self.saved_views.scope.is_some() {
                self.saved_views.cancel();
            }
            return;
        }
        if let Some(result) = self.saved_views.reader.poll(ctx) {
            match result {
                Ok(Answer::Selection(selection)) => {
                    self.saved_views.selection = Some(selection);
                    self.saved_views.error = None;
                    self.saved_views.dirty = true;
                }
                Ok(Answer::Page(generation, page)) => {
                    if generation == self.history_gen {
                        self.more_loaded(Ok(page), ctx);
                    }
                }
                Err(e) => {
                    self.saved_views.error = Some(e);
                }
            }
        }
        if self.loading || self.deferred || !self.saved_views.loaded || !self.saved_views.active() {
            return;
        }
        if self.saved_views.dirty {
            let scope = self.graph_view_scope();
            if self.saved_views.scope.as_ref() != Some(&scope) {
                let Some(preset) = self.saved_views.prefs.active.clone() else {
                    return;
                };
                let repo = self.path.clone();
                let request = scope.clone();
                self.saved_views.reader.queue(move |active| {
                    views::query(&repo, &preset, &request, active).map(Answer::Selection)
                });
                self.saved_views.scope = Some(scope);
                self.saved_views.selection = None;
                self.saved_views.error = None;
            }
        }
        if self.saved_views.reader.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
    }
    pub(super) fn prepare_graph_projection(&mut self) {
        if !self.saved_views.active() {
            return;
        }
        if self.saved_views.dirty || self.saved_views.selected != self.selected {
            let all;
            let matches = if let Some(selection) = &self.saved_views.selection {
                &selection.commits
            } else {
                all = self.commits.iter().map(|c| c.hash.clone()).collect();
                &all
            };
            let stashes = self.stashes.iter().map(|s| s.hash.clone()).collect();
            self.saved_views.projection = projection::Projection::build(
                &self.commits,
                &self.graph,
                matches,
                &self.saved_views.expanded,
                self.selected,
                &stashes,
            );
            self.saved_views.selected = self.selected;
            self.saved_views.dirty = false;
        self.history_minimap.dirty = true;
        }
    }
    pub(super) fn expand_graph_span(&mut self, range: std::ops::Range<usize>) {
        if let Some(commits) = self.commits.get(range) {
            self.saved_views
                .expanded
                .extend(commits.iter().map(|c| c.hash.clone()));
            self.saved_views.dirty = true;
        }
    }
    pub(super) fn load_graph_view_page(&mut self) {
        if self.saved_views.reader.busy()
            || self.loading_more
            || !self.has_more
            || self.history.len() >= super::MAX_HISTORY
        {
            return;
        }
        let Some(scope) = self.saved_views.scope.as_ref() else {
            return;
        };
        let tips = scope.tips.clone();
        let repo = self.path.clone();
        let skip = self.history.len();
        let generation = self.history_gen;
        self.saved_views.reader.queue(move |active| {
            git::investigations::page(&repo, &tips, skip, git::LOG_PAGE, active)
                .map(|page| Answer::Page(generation, page))
        });
    }
}
