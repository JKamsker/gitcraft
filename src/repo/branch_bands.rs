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
            Self::Compact => crate::i18n::text("repo-core-0207"),
            Self::Trunk => crate::i18n::text("repo-core-0208"),
            Self::GitFlow => crate::i18n::text("repo-core-0209"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
        let mut prefs: Self = ctx
            .data_mut(|d| d.get_persisted(id(path)))
            .unwrap_or_default();
        prefs
            .pins
            .retain(|name, band| name.len() <= 1024 && *band < 4);
        prefs
    }
    fn store(&self, ctx: &egui::Context, path: &Path) {
        ctx.data_mut(|d| d.insert_persisted(id(path), self.clone()));
    }
    fn band(&self, full: &str, remotes: &[String]) -> usize {
        if let Some(band) = self.pins.get(full) {
            return *band;
        }
        let short = full
            .strip_prefix("refs/heads/")
            .or_else(|| {
                full.strip_prefix("refs/remotes/").and_then(|s| {
                    remotes
                        .iter()
                        .filter_map(|remote| {
                            s.strip_prefix(&format!("{remote}/"))
                                .map(|short| (remote.len(), short))
                        })
                        .max_by_key(|(len, _)| *len)
                        .map(|(_, short)| short)
                })
            })
            .unwrap_or(full);
        if matches!(short, "main" | "master" | "trunk") {
            0
        } else if self.model == Model::GitFlow && short == "develop" {
            1
        } else if short.starts_with("release/") || short.starts_with("hotfix/") {
            if self.model == Model::GitFlow { 2 } else { 1 }
        } else if self.model == Model::GitFlow {
            3
        } else {
            2
        }
    }
    pub(super) fn hints(&self, commits: &[Arc<Commit>], refs: &Refs) -> HashMap<String, LaneHint> {
        if self.model == Model::Compact && self.pins.is_empty() {
            return HashMap::new();
        }
        let index: HashMap<&str, &Commit> = commits
            .iter()
            .map(|c| (c.hash.as_str(), c.as_ref()))
            .collect();
        let mut branches: Vec<(String, &str)> = refs
            .local
            .iter()
            .map(|b| (format!("refs/heads/{}", b.name), b.target.as_str()))
            .chain(
                refs.remotes
                    .iter()
                    .flat_map(|(_, bs)| bs)
                    .map(|b| (format!("refs/remotes/{}", b.name), b.target.as_str())),
            )
            .collect();
        let remotes: Vec<_> = refs.remotes.iter().map(|(name, _)| name.clone()).collect();
        branches.sort_by_key(|(name, _)| (self.band(name, &remotes), name.clone()));
        let colour = |name: &str| {
            name.bytes().fold(2166136261u32, |h, b| {
                (h ^ u32::from(b)).wrapping_mul(16777619)
            }) as usize
        };
        // Reserve every real ref tip first: a newly added descendant branch must not
        // recolour an existing branch's unchanged tip or its first-parent ancestry.
        let mut hints = HashMap::new();
        for (name, tip) in &branches {
            hints.entry((*tip).to_owned()).or_insert(LaneHint {
                band: self.band(name, &remotes),
                colour: colour(name),
            });
        }
        for (name, mut at) in branches {
            let hint = LaneHint {
                band: self.band(&name, &remotes),
                colour: colour(&name),
            };
            let mut first = true;
            while let Some(commit) = index.get(at) {
                if let Some(existing) = hints.get(at)
                    && (!first || existing.band != hint.band || existing.colour != hint.colour)
                {
                    break;
                }
                hints.insert(at.to_owned(), hint);
                first = false;
                let Some(parent) = commit.parents.first() else {
                    break;
                };
                at = parent;
            }
        }
        // Detached/stash/unlabelled runs occupy the last band, never the trunk band.
        for c in commits {
            hints
                .entry(c.hash.clone())
                .or_insert(LaneHint { band: 3, colour: 3 });
        }
        hints
    }
}
impl RepoTab {
    // Reconcile after worktree discovery and before accepting a layout. Sibling
    // tabs observe the shared persisted value on their next awake poll.
    pub(super) fn sync_branch_bands(&mut self, ctx: &egui::Context) {
        let prefs = Preferences::load(ctx, self.repo_key());
        if prefs != self.branch_bands {
            self.branch_bands = prefs;
            self.queue_branch_bands();
        }
    }
    pub(super) fn queue_branch_bands(&mut self) {
        let prefs = self.branch_bands.clone();
        let commits = self.commits.clone();
        let refs = self.refs.clone();
        let first_parent = self.log_order.first_parent;
        self.band_reader.queue(move |active| {
            if !active() {
                return Err(crate::i18n::text("repo-core-0220").into());
            }
            let hints = prefs.hints(&commits, &refs);
            let graph = crate::graph::layout_banded(&commits, first_parent, &hints);
            if !active() {
                return Err(crate::i18n::text("repo-core-0220").into());
            }
            Ok(graph)
        });
    }
    pub(super) fn poll_branch_bands(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.band_reader.poll(&self.tx, ctx) {
            match result {
                Ok(graph) => {
                    self.graph = graph;
                    self.rebuild_graph_labels();
                    self.history_minimap.dirty = true;
                }
                Err(error) => self.status = Some((error, true)),
            }
        }
    }
    pub(super) fn branch_bands_header(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.menu_button(crate::i18n::text("repo-core-0221"), |ui| {
            let mut settings = crate::settings::Settings::get(ui.ctx());
            if ui
                .checkbox(
                    &mut settings.graph_lane_walk,
                    crate::i18n::text("repo-core-0222"),
                )
                .changed()
            {
                settings.set(ui.ctx());
            }

            if ui.button(crate::i18n::text("repo-core-0223")).clicked() {
                self.open_sparse();
                ui.close();
            }
            if ui.button(crate::i18n::text("repo-core-0224")).clicked() {
                self.open_cleanup();
                ui.close();
            }
            self.postcard_button(ui);
            if ui.button(crate::i18n::text("repo-core-0225")).clicked() {
                self.open_growth();
                ui.close();
            }
            if ui.button(crate::i18n::text("repo-core-0226")).clicked() {
                self.open_atlas();
                ui.close();
            }
            if ui.button(crate::i18n::text("repo-core-0227")).clicked() {
                self.open_activity(super::history_activity::Page::Calendar, ui.ctx());
                ui.close();
            }
            if ui.button(crate::i18n::text("repo-core-0228")).clicked() {
                self.open_activity(super::history_activity::Page::Contributors, ui.ctx());
                ui.close();
            }
            egui::ComboBox::from_id_salt("branch-model")
                .selected_text(self.branch_bands.model.label())
                .show_ui(ui, |ui| {
                    for model in [Model::Compact, Model::Trunk, Model::GitFlow] {
                        changed |= ui
                            .selectable_value(&mut self.branch_bands.model, model, model.label())
                            .changed();
                    }
                });
            ui.menu_button(crate::i18n::text("repo-core-0230"), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (full, name) in
                            self.refs
                                .local
                                .iter()
                                .map(|b| (format!("refs/heads/{}", b.name), b.name.as_str()))
                                .chain(
                                    self.refs.remotes.iter().flat_map(|(_, bs)| bs).map(|b| {
                                        (format!("refs/remotes/{}", b.name), b.name.as_str())
                                    }),
                                )
                        {
                            ui.menu_button(name, |ui| {
                                for band in 0..4 {
                                    if ui
                                        .selectable_label(
                                            self.branch_bands.pins.get(&full) == Some(&band),
                                            crate::i18n::message(
                                                "repo-core-0231",
                                                &[("band", format!("{}", band + 1).into())],
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.branch_bands.pins.insert(full.clone(), band);
                                        changed = true;
                                        ui.close();
                                    }
                                }
                                if ui.button(crate::i18n::text("repo-core-0232")).clicked() {
                                    self.branch_bands.pins.remove(&full);
                                    changed = true;
                                    ui.close();
                                }
                            });
                        }
                    });
                if ui.button(crate::i18n::text("repo-core-0233")).clicked() {
                    self.branch_bands.pins.clear();
                    changed = true;
                    ui.close();
                }
            });
            if self.branch_bands.model != Model::Compact || !self.branch_bands.pins.is_empty() {
                ui.small(crate::i18n::text("repo-core-0234"));
            }
        });
        if self.band_reader.busy() {
            ui.add(crate::spinner::Spinner::new().size(14.0));
            ui.small(crate::i18n::text("repo-core-0235"))
                .on_hover_text(crate::i18n::text("repo-core-0236"));
        }
        if changed {
            self.branch_bands.store(ui.ctx(), self.repo_key());
            self.queue_branch_bands();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Branch;
    #[test]
    fn linked_worktrees_reconcile_shared_preferences_and_reopen() {
        let ctx = egui::Context::default();
        let main = std::path::PathBuf::from("/repos/main");
        let linked = std::path::PathBuf::from("/repos/linked");
        let mut tab = RepoTab::new_deferred(linked.clone(), &ctx);
        let mut sibling = RepoTab::new_deferred(main.clone(), &ctx);
        let worktrees = vec![crate::git::Worktree {
            path: main.clone(),
            main: true,
            ..Default::default()
        }];
        tab.worktrees = worktrees.clone();
        sibling.worktrees = worktrees;
        let prefs = Preferences {
            model: Model::GitFlow,
            pins: BTreeMap::from([("refs/heads/feature/x".into(), 1)]),
        };
        prefs.store(&ctx, tab.repo_key());
        tab.sync_branch_bands(&ctx);
        sibling.sync_branch_bands(&ctx);
        assert_eq!(tab.branch_bands, prefs);
        assert_eq!(sibling.branch_bands, prefs);
        let mut reopened = RepoTab::new_deferred(linked, &ctx);
        reopened.continue_from(&tab);
        reopened.sync_branch_bands(&ctx);
        assert_eq!(reopened.branch_bands, prefs);
        Preferences::default().store(&ctx, sibling.repo_key());
        reopened.sync_branch_bands(&ctx);
        assert_eq!(reopened.branch_bands, Preferences::default());
    }
    #[test]
    fn real_tips_reserve_ancestry_colour_and_slash_remote_main_remains_trunk() {
        let c = |hash: &str, parents: &[&str]| {
            Arc::new(Commit {
                hash: hash.into(),
                parents: parents.iter().map(|p| (*p).into()).collect(),
                ..Default::default()
            })
        };
        let branch = |name: &str, target: &str| Branch {
            name: name.into(),
            target: target.into(),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        };
        let commits = vec![c("a", &["z"]), c("z", &["base"]), c("base", &[])];
        let mut refs = Refs {
            local: vec![branch("feature/z", "z")],
            ..Default::default()
        };
        let prefs = Preferences {
            model: Model::Trunk,
            ..Default::default()
        };
        let before = prefs.hints(&commits, &refs);
        refs.local.push(branch("feature/a", "a"));
        let after = prefs.hints(&commits, &refs);
        assert_eq!(before["z"].colour, after["z"].colour);
        assert_eq!(before["base"].colour, after["base"].colour);
        refs.remotes = vec![("my/fork".into(), vec![branch("my/fork/main", "base")])];
        assert_eq!(prefs.hints(&commits, &refs)["base"].band, 0);
    }
    #[test]
    fn a_newer_feature_tip_joins_the_older_trunk_in_the_trunk_band() {
        let c = |hash: &str, parents: &[&str]| {
            Arc::new(Commit {
                hash: hash.into(),
                parents: parents.iter().map(|p| (*p).into()).collect(),
                ..Default::default()
            })
        };
        let commits = vec![
            c("feature", &["main"]),
            c("main", &["base"]),
            c("base", &[]),
        ];
        let branch = |name: &str, target: &str| Branch {
            name: name.into(),
            target: target.into(),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        };
        let refs = Refs {
            local: vec![branch("feature/work", "feature"), branch("main", "main")],
            ..Default::default()
        };
        let prefs = Preferences {
            model: Model::Trunk,
            ..Default::default()
        };
        let graph = crate::graph::layout_banded(&commits, false, &prefs.hints(&commits, &refs));
        assert!(graph.rows[1].col < graph.rows[0].col);
        assert_eq!(graph.rows[1].incoming, graph.rows[0].outgoing);
        assert_ne!(graph.rows[1].origin, graph.rows[0].origin);
        assert_eq!(graph.rows[1].col, graph.rows[2].col);
    }
    #[test]
    fn branch_bands_preserve_merge_edges_and_colour_after_unrelated_tip_arrives() {
        let c = |hash: &str, parents: &[&str]| {
            Arc::new(Commit {
                hash: hash.into(),
                parents: parents.iter().map(|p| (*p).into()).collect(),
                ..Default::default()
            })
        };
        let commits = vec![
            c("feature", &["base"]),
            c("main", &["base"]),
            c("base", &[]),
        ];
        let refs = Refs {
            local: vec![
                Branch {
                    name: "main".into(),
                    target: "main".into(),
                    is_head: true,
                    upstream: String::new(),
                    track: String::new(),
                },
                Branch {
                    name: "feature/work".into(),
                    target: "feature".into(),
                    is_head: false,
                    upstream: String::new(),
                    track: String::new(),
                },
            ],
            ..Default::default()
        };
        let prefs = Preferences {
            model: Model::Trunk,
            ..Default::default()
        };
        let hints = prefs.hints(&commits, &refs);
        let graph = crate::graph::layout_banded(&commits, false, &hints);
        assert!(graph.rows[1].col < graph.rows[0].col);
        assert_eq!(graph.rows[2].incoming.len(), 2);
        let colour = graph.rows[0].colours[&graph.rows[0].col];
        let mut other = commits.clone();
        other.insert(0, c("unrelated", &[]));
        let graph = crate::graph::layout_banded(&other, false, &prefs.hints(&other, &refs));
        assert_eq!(graph.rows[1].colours[&graph.rows[1].col], colour);
        let mut pinned = prefs;
        pinned.pins.insert("refs/heads/main".into(), 3);
        pinned.pins.insert("refs/heads/feature/work".into(), 0);
        let graph = crate::graph::layout_banded(&commits, false, &pinned.hints(&commits, &refs));
        assert!(graph.rows[0].col < graph.rows[1].col);
    }
}
