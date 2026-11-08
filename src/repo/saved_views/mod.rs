//! Repository-scoped saved graph relations and their display-only projection.
mod projection;
pub(in crate::repo) mod reader;
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
    #[serde(default)]
    generated_active: Option<Preset>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            saved: Vec::new(),
            active: None,
            generated_active: None,
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
        if self.active.as_ref() != self.generated_active.as_ref() {
            self.generated_active = None;
        }
        self
    }
    fn active_caption(&self) -> String {
        let Some(active) = &self.active else {
            return crate::i18n::text("repo-workflows-saved_views-saved-graph-header-0b3db67d")
                .into();
        };
        if self.generated_active.as_ref() != Some(active) {
            return active.name.clone();
        }
        match &active.kind {
            Kind::MyUnpushed => {
                crate::i18n::text("repo-workflows-saved_views-saved-graph-header-5f0d0c04").into()
            }
            Kind::RecentAncestors => {
                crate::i18n::text("repo-workflows-saved_views-saved-graph-header-a5d2f076").into()
            }
            Kind::Descendants { anchor } => crate::i18n::message(
                "repo-workflows-saved_views-saved-graph-header-d2cb0f94",
                &[("super-super-short-anchor-1", super::short(anchor).into())],
            ),
            Kind::MergesInto { branch } => crate::i18n::message(
                "repo-workflows-saved_views-saved-graph-header-b5bc2926",
                &[("b-name-1", branch.as_str().into())],
            ),
        }
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
        self.saved_views.reader.bind(self.tx.cancel.clone());
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
        if self.loading || self.sleeping() || !self.saved_views.loaded || !self.saved_views.active()
        {
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
        let order = self.log_order;
        self.saved_views.reader.queue(move |active| {
            git::investigations::page(&repo, &tips, skip, order, git::LOG_PAGE, active)
                .map(|page| Answer::Page(generation, page))
        });
    }
}

#[cfg(test)]
mod fluent_r1_preset_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_generated_preset_origin_persists_and_legacy_user_names_stay_opaque() {
        for (name, kind) in [
            ("My unpushed work", Kind::MyUnpushed),
            ("My recent changes + dependencies", Kind::RecentAncestors),
            (
                "Descendants of aaaaaaaa",
                Kind::Descendants {
                    anchor: "a".repeat(40),
                },
            ),
            (
                "Only merges into raw/main",
                Kind::MergesInto {
                    branch: "raw/main".into(),
                },
            ),
        ] {
            let preset = Preset {
                name: name.into(),
                kind,
                days: 30,
            };
            let original = Preferences {
                active: Some(preset.clone()),
                generated_active: Some(preset.clone()),
                ..Default::default()
            };
            let encoded = serde_json::to_string(&original).unwrap();
            let restored: Preferences = serde_json::from_str(&encoded).unwrap();
            let mut first = None;
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                let caption = restored.active_caption();
                if lang == "de" {
                    assert_ne!(caption, name);
                    if let Some(first) = &first {
                        assert_eq!(&caption, first);
                    } else {
                        first = Some(caption);
                    }
                } else {
                    assert_eq!(caption, name);
                }
                assert_eq!(restored.active.as_ref(), Some(&preset));
                assert_eq!(serde_json::to_string(&restored).unwrap(), encoded);
            }
            for custom in [name, "Meine ungepushten Änderungen", "My unpushed work"] {
                let mut user = restored.clone();
                user.active.as_mut().unwrap().name = custom.into();
                user.generated_active = None;
                for lang in ["de", "en-US", "de"] {
                    crate::i18n::set_language(lang);
                    assert_eq!(user.active_caption(), custom);
                }
                let mut legacy = serde_json::to_value(&user).unwrap();
                legacy.as_object_mut().unwrap().remove("generated_active");
                let legacy: Preferences = serde_json::from_value(legacy).unwrap();
                assert_eq!(legacy.active_caption(), custom);
            }
            let mut mismatch = restored;
            mismatch.active.as_mut().unwrap().days += 1;
            assert_eq!(mismatch.active_caption(), name);
        }
        crate::i18n::set_language("en-US");
    }
}
