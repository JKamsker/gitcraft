//! Presentation filters. Original commits/parents remain available for Git operations.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use super::RepoTab;
use crate::git::Commit;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Exact name AND email pair, verified in real checkpoint metadata.
    pub hide_t3: bool,
    /// One exact, case-sensitive name per line.
    pub names: String,
    /// One exact email per line, compared case-insensitively.
    pub emails: String,
    /// Full ref patterns, passed to Git as --exclude before --all.
    pub namespaces: String,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            hide_t3: true,
            names: String::new(),
            emails: String::new(),
            namespaces: "refs/t3/*".into(),
        }
    }
}
impl Rules {
    pub fn empty() -> Self {
        Self {
            hide_t3: false,
            namespaces: String::new(),
            ..Self::default()
        }
    }
    pub fn excludes(&self) -> Vec<String> {
        crate::git::history_scope::namespace_patterns(&self.namespaces)
    }
    fn matches(&self, c: &Commit) -> bool {
        (self.hide_t3
            && c.author == "T3 Code"
            && c.email
                .eq_ignore_ascii_case("t3code@users.noreply.github.com"))
            || self
                .names
                .lines()
                .any(|n| !n.trim().is_empty() && n.trim() == c.author)
            || self
                .emails
                .lines()
                .any(|e| !e.trim().is_empty() && e.trim().eq_ignore_ascii_case(&c.email))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    override_global: bool,
    rules: Rules,
    reveal: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            override_global: false,
            rules: Rules::empty(),
            reveal: false,
        }
    }
}
fn id(repo: &Path) -> egui::Id {
    egui::Id::new(("history-filters", repo))
}
fn load(ctx: &egui::Context, repo: &Path) -> Preferences {
    ctx.data_mut(|d| d.get_persisted(id(repo)))
        .unwrap_or_default()
}
fn effective(global: &Rules, p: &Preferences) -> Rules {
    if p.reveal {
        return Rules::empty();
    }
    if p.override_global {
        return p.rules.clone();
    }
    Rules {
        hide_t3: global.hide_t3 || p.rules.hide_t3,
        names: format!("{}\n{}", global.names, p.rules.names),
        emails: format!("{}\n{}", global.emails, p.rules.emails),
        namespaces: format!("{}\n{}", global.namespaces, p.rules.namespaces),
    }
}
pub(super) fn rules(ctx: &egui::Context, repo: &Path) -> Rules {
    effective(
        &crate::settings::Settings::get(ctx).history_filter,
        &load(ctx, repo),
    )
}

#[derive(Default)]
pub(super) struct State {
    pub applied: Option<Rules>,
    pub read_namespaces: Option<Vec<String>>,
    pub hidden_loaded: usize,
    pub kept_loaded: usize,
    pub excluded_total: Option<usize>,
    /// Temporary targets while navigation reinserts a compacted row; selection then protects it.
    pub revealed: HashSet<String>,
    pub pinned: Option<Arc<Commit>>,
    pub pinned_layout: Option<Arc<Commit>>,
}

/// Keep ref tips, navigation/search targets and unresolved page boundaries visible. For each
/// omitted commit, reconnect children to its nearest visible ancestors on ALL parent paths.
/// Stable de-duplication preserves first-parent order. Only the layout copy gets new parents.
pub(super) fn compact(
    history: &[Arc<Commit>],
    rules: &Rules,
    protected: &HashSet<String>,
    first_parent: bool,
    has_more: bool,
) -> (Vec<Arc<Commit>>, Vec<Arc<Commit>>, usize, usize) {
    let by_hash: HashMap<&str, &Arc<Commit>> =
        history.iter().map(|c| (c.hash.as_str(), c)).collect();
    let hidden: HashSet<&str> = history
        .iter()
        .filter(|c| {
            rules.matches(c)
                && c.refs.is_empty()
                && !protected.contains(&c.hash)
                // --first-parent intentionally omits side parents; only an unresolved
                // parent on a traversed path can be a genuine next-page frontier.
                && (!has_more || c.parents.iter().take(if first_parent { 1 } else { usize::MAX })
                    .all(|p| by_hash.contains_key(p.as_str())))
        })
        .map(|c| c.hash.as_str())
        .collect();
    let kept = history
        .iter()
        .filter(|c| rules.matches(c) && !hidden.contains(c.hash.as_str()))
        .count();
    let mut frontiers: HashMap<&str, Vec<String>> = HashMap::new();
    // Iterative post-order traversal avoids stack overflow on long generated histories.
    for &hash in &hidden {
        let mut stack = vec![(hash, false)];
        let mut visiting = HashSet::new();
        while let Some((hash, finish)) = stack.pop() {
            if frontiers.contains_key(hash) {
                continue;
            }
            let Some(c) = by_hash.get(hash) else {
                continue;
            };
            if !finish {
                if !visiting.insert(hash) {
                    continue;
                }
                stack.push((hash, true));
                for parent in c
                    .parents
                    .iter()
                    .take(if first_parent { 1 } else { usize::MAX })
                    .rev()
                {
                    if hidden.contains(parent.as_str()) && !visiting.contains(parent.as_str()) {
                        stack.push((parent.as_str(), false));
                    }
                }
            } else {
                let mut nearest = Vec::new();
                let mut seen = HashSet::new();
                for p in c
                    .parents
                    .iter()
                    .take(if first_parent { 1 } else { usize::MAX })
                {
                    let ancestors = frontiers
                        .get(p.as_str())
                        .cloned()
                        .unwrap_or_else(|| vec![p.clone()]);
                    nearest.extend(ancestors.into_iter().filter(|p| seen.insert(p.clone())));
                }
                frontiers.insert(hash, nearest);
            }
        }
    }
    let visible: Vec<Arc<Commit>> = history
        .iter()
        .filter(|c| !hidden.contains(c.hash.as_str()))
        .cloned()
        .collect();
    let graph = visible
        .iter()
        .map(|c| {
            let mut parents = Vec::new();
            let mut seen = HashSet::new();
            for p in c
                .parents
                .iter()
                .take(if first_parent { 1 } else { usize::MAX })
            {
                let nearest = frontiers
                    .get(p.as_str())
                    .cloned()
                    .unwrap_or_else(|| vec![p.clone()]);
                parents.extend(nearest.into_iter().filter(|p| seen.insert(p.clone())));
            }
            if parents == c.parents {
                Arc::clone(c)
            } else {
                Arc::new(Commit {
                    parents,
                    ..(**c).clone()
                })
            }
        })
        .collect();
    (visible, graph, hidden.len(), kept)
}

/// A selected namespace-only row bridges omitted old ancestors to the newly visible scope.
/// Keep its operational parents intact; only this layout copy follows the old loaded DAG.
pub(super) fn pinned_projection(
    commit: &Arc<Commit>,
    previous: &[Arc<Commit>],
    current: &[Arc<Commit>],
) -> Arc<Commit> {
    let visible: HashSet<&str> = current.iter().map(|c| c.hash.as_str()).collect();
    let old: HashMap<&str, &Arc<Commit>> = previous.iter().map(|c| (c.hash.as_str(), c)).collect();
    let mut stack: Vec<&str> = commit.parents.iter().rev().map(String::as_str).collect();
    let mut visited = HashSet::new();
    let mut parents = Vec::new();
    while let Some(hash) = stack.pop() {
        if !visited.insert(hash) {
            continue;
        }
        if visible.contains(hash) {
            parents.push(hash.to_owned());
        } else if let Some(parent) = old.get(hash) {
            stack.extend(parent.parents.iter().rev().map(String::as_str));
        } else {
            // An unloaded boundary is retained, just as in ordinary partial history.
            parents.push(hash.to_owned());
        }
    }
    Arc::new(Commit {
        parents,
        ..(**commit).clone()
    })
}

/// Shared controls in global settings and the per-repository popover.
pub(crate) fn edit(ui: &mut egui::Ui, rules: &mut Rules) {
    ui.checkbox(&mut rules.hide_t3, "Hide T3 Code checkpoint identity")
        .on_hover_text("Exact author name T3 Code and email t3code@users.noreply.github.com. Co-writer badges never hide commits.");
    for (label, value, hint) in [
        (
            "Excluded author names",
            &mut rules.names,
            "Exact names, one per line; case-sensitive",
        ),
        (
            "Excluded author emails",
            &mut rules.emails,
            "Exact emails, one per line; case-insensitive",
        ),
        (
            "Hidden ref namespaces",
            &mut rules.namespaces,
            "Full refs/ patterns, one per line (e.g. refs/t3/*)",
        ),
    ] {
        ui.label(label);
        ui.add(
            egui::TextEdit::multiline(value)
                .desired_rows(2)
                .desired_width(ui.available_width().min(360.0))
                .hint_text(hint),
        )
        .on_hover_text(hint);
    }
    let invalid: Vec<&str> = rules
        .namespaces
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && crate::git::history_scope::namespace_patterns(line).is_empty()
        })
        .collect();
    if !invalid.is_empty() {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).warning,
            format!("Ignored invalid ref patterns: {}", invalid.join(", ")),
        );
    }
    ui.label(
        egui::RichText::new(
            "Display only. Tips, selection, search targets and page boundaries remain accessible.",
        )
        .small(),
    );
}

impl RepoTab {
    pub(super) fn history_filters_frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut prefs = load(&ctx, self.repo_key());
        let previous = prefs.clone();
        let namespaces = self
            .history_filter
            .excluded_total
            .map_or_else(|| "…".to_owned(), |n| n.to_string());
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("History filters", |ui| {
                let viewport = ui.ctx().content_rect();
                ui.set_max_width((viewport.width() - 24.0).clamp(120.0, 380.0));
                egui::ScrollArea::vertical().id_salt("history-filter-options").max_height((viewport.height() - 60.0).max(80.0)).show(ui, |ui| {
                ui.label(format!("{} author rows hidden in loaded history", self.history_filter.hidden_loaded));
                ui.label(format!("{namespaces} commits reachable only from excluded namespaces"));
                ui.separator();
                ui.checkbox(&mut prefs.override_global, "Override global history filters")
                    .on_hover_text("Off: add these rules to Settings → Commit graph. On: use only these rules.");
                edit(ui, &mut prefs.rules);
                ui.label(format!("{} matching commits kept: tips, selection, search or page boundaries", self.history_filter.kept_loaded));
                if ui.button("Reset repository filters").clicked() { prefs = Preferences::default(); }
                });
            }).response.on_hover_text(format!("{} author rows hidden in loaded history; {namespaces} namespace-only commits excluded. Open to edit global additions or repository overrides.", self.history_filter.hidden_loaded));
            ui.checkbox(&mut prefs.reveal, "Show hidden").on_hover_text("Temporarily reveal excluded authors and ref namespaces in this repository. No Git data is changed.");
        });
        if prefs != previous {
            ctx.data_mut(|d| d.insert_persisted(id(self.repo_key()), prefs));
        }
        let next = rules(&ctx, self.repo_key());
        if self.history_filter.applied.as_ref() != Some(&next) {
            let exclusions_changed = self
                .history_filter
                .read_namespaces
                .as_ref()
                .is_some_and(|r| *r != next.excludes());
            self.history_filter.applied = Some(next);
            if !self.history.is_empty() {
                self.rebuild(&ctx);
            }
            if exclusions_changed {
                self.backlinks_scope_changed(&ctx);
                if self.search.open && !self.search.query.trim().is_empty() {
                    self.search.matches.clear();
                    self.search.results.clear();
                    self.run_search(&ctx);
                }
                self.reload(&ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests;
