//! Persistent branch-model bands. No Git work is performed while painting or changing a preset.
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::RepoTab;
use crate::git::{Commit, Refs};
use crate::graph::LaneHint;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Model {
    #[default]
    Compact,
    Trunk,
    GitFlow,
}
impl Model {
    fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact lanes",
            Self::Trunk => "Trunk · release · feature",
            Self::GitFlow => "Git flow",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    model: Model,
    pins: BTreeMap<String, usize>,
}
fn id(path: &Path) -> egui::Id {
    egui::Id::new(("branch-bands-v1", path))
}
impl Preferences {
    pub(super) fn load(ctx: &egui::Context, path: &Path) -> Self {
        let mut prefs: Self = ctx.data_mut(|d| d.get_persisted(id(path))).unwrap_or_default();
        prefs.pins.retain(|name, band| name.len() <= 1024 && *band < 4);
        prefs
    }
    fn band(&self, full: &str) -> usize {
        if let Some(band) = self.pins.get(full) { return *band; }
        let short = full.strip_prefix("refs/heads/").or_else(|| full.strip_prefix("refs/remotes/").and_then(|s| s.split_once('/').map(|(_,n)| n))).unwrap_or(full);
        if matches!(short, "main" | "master" | "trunk") { 0 }
        else if self.model == Model::GitFlow && short == "develop" { 1 }
        else if short.starts_with("release/") || short.starts_with("hotfix/") { if self.model == Model::GitFlow { 2 } else { 1 } }
        else if self.model == Model::GitFlow { 3 } else { 2 }
    }
    pub(super) fn hints(&self, commits: &[Arc<Commit>], refs: &Refs) -> HashMap<String, LaneHint> {
        if self.model == Model::Compact && self.pins.is_empty() { return HashMap::new(); }
        let index: HashMap<&str, &Commit> = commits.iter().map(|c| (c.hash.as_str(), c.as_ref())).collect();
        let mut branches: Vec<(String, &str)> = refs.local.iter().map(|b| (format!("refs/heads/{}",b.name), b.target.as_str())).chain(refs.remotes.iter().flat_map(|(_,bs)| bs).map(|b| (format!("refs/remotes/{}",b.name), b.target.as_str()))).collect();
        branches.sort_by_key(|(name,_)| (self.band(name), name.clone()));
        let mut hints = HashMap::new();
        for (name, mut at) in branches {
            let band = self.band(&name);
            // FNV-1a is explicitly stable across processes and Rust versions.
            let colour = name.bytes().fold(2166136261u32, |h,b| (h ^ u32::from(b)).wrapping_mul(16777619)) as usize;
            while let Some(commit) = index.get(at) {
                if hints.contains_key(at) { break; }
                hints.insert(at.to_owned(), LaneHint { band, colour });
                let Some(parent) = commit.parents.first() else { break; };
                at = parent;
            }
        }
        // Detached/stash/unlabelled runs occupy the last band, never the trunk band.
        for c in commits {
            hints.entry(c.hash.clone()).or_insert(LaneHint { band: 3, colour: 3 });
        }
        hints
    }
}
impl RepoTab {
    pub(super) fn branch_bands_header(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("branch-model").selected_text(self.branch_bands.model.label()).show_ui(ui, |ui| {
                for model in [Model::Compact, Model::Trunk, Model::GitFlow] {
                    changed |= ui.selectable_value(&mut self.branch_bands.model, model, model.label()).changed();
                }
            });
            ui.menu_button("Pin branch to band…", |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (full, name) in self.refs.local.iter().map(|b| (format!("refs/heads/{}", b.name), b.name.as_str())).chain(self.refs.remotes.iter().flat_map(|(_,bs)|bs).map(|b| (format!("refs/remotes/{}", b.name), b.name.as_str()))) {
                        ui.menu_button(name, |ui| {
                            for band in 0..4 {
                                if ui.selectable_label(self.branch_bands.pins.get(&full) == Some(&band), format!("Band {}", band + 1)).clicked() {
                                    self.branch_bands.pins.insert(full.clone(), band);
                                    changed = true;
                                    ui.close();
                                }
                            }
                            if ui.button("Use preset").clicked() { self.branch_bands.pins.remove(&full); changed = true; ui.close(); }
                        });
                    }
                });
                if ui.button("Clear all pins").clicked() { self.branch_bands.pins.clear(); changed = true; ui.close(); }
            });
            if self.branch_bands.model != Model::Compact || !self.branch_bands.pins.is_empty() {
                ui.small("Bands ordered left to right · branch colours remain stable across reloads");
            }
        });
        if changed {
            ui.ctx().data_mut(|d| d.insert_persisted(id(self.repo_key()), self.branch_bands.clone()));
            self.graph = crate::graph::layout_banded(&self.commits, &self.branch_bands.hints(&self.commits, &self.refs));
            self.rebuild_graph_labels();
        self.history_minimap.dirty = true;
            self.saved_views.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Branch;
    #[test]
    fn branch_bands_preserve_merge_edges_and_colour_after_unrelated_tip_arrives() {
        let c = |hash: &str, parents: &[&str]| Arc::new(Commit { hash: hash.into(), parents: parents.iter().map(|p| (*p).into()).collect(), ..Default::default() });
        let commits = vec![c("feature", &["base"]),c("main", &["base"]),c("base", &[])];
        let refs = Refs { local: vec![Branch { name: "main".into(), target: "main".into(), is_head: true, upstream: String::new(), track: String::new() },Branch { name: "feature/work".into(), target: "feature".into(), is_head: false, upstream: String::new(), track: String::new() }], ..Default::default() };
        let prefs = Preferences { model: Model::Trunk, ..Default::default() };
        let hints = prefs.hints(&commits, &refs);
        let graph = crate::graph::layout_banded(&commits, &hints);
        assert!(graph.rows[1].col < graph.rows[0].col);
        assert_eq!(graph.rows[2].incoming.len(), 2);
        let colour = graph.rows[0].colours[&graph.rows[0].col];
        let mut other = commits.clone(); other.insert(0,c("unrelated", &[]));
        let graph = crate::graph::layout_banded(&other, &prefs.hints(&other,&refs));
        assert_eq!(graph.rows[1].colours[&graph.rows[1].col], colour);
        let mut pinned = prefs; pinned.pins.insert("refs/heads/main".into(),3); pinned.pins.insert("refs/heads/feature/work".into(),0);
        let graph = crate::graph::layout_banded(&commits, &pinned.hints(&commits,&refs));
        assert!(graph.rows[0].col < graph.rows[1].col);
    }
}
