//! Walking real loaded topology; never fetches or changes the Saved Views projection.
use super::RepoTab;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Parent,
    Child,
    Merge,
    Tip,
}

#[derive(Default)]
pub(super) struct Narration {
    hash: String,
    result: Option<String>,
    reader: Option<std::sync::mpsc::Receiver<String>>,
}

#[derive(Default)]
pub(super) struct Topology {
    pub(super) parents: Vec<Vec<usize>>,
    pub(super) children: Vec<Vec<usize>>,
}

impl Topology {
    pub(super) fn build(
        commits: &[std::sync::Arc<crate::git::Commit>],
        index: &HashMap<String, usize>,
    ) -> Self {
        let mut out = Self {
            parents: vec![Vec::new(); commits.len()],
            children: vec![Vec::new(); commits.len()],
        };
        for (i, commit) in commits.iter().enumerate() {
            for parent in &commit.parents {
                if let Some(&p) = index.get(parent) {
                    out.parents[i].push(p);
                    out.children[p].push(i);
                }
            }
        }
        out
    }

    fn ancestors(&self, start: usize) -> HashSet<usize> {
        let mut seen = HashSet::new();
        let mut todo = vec![start];
        while let Some(row) = todo.pop() {
            if !seen.insert(row) {
                continue;
            }
            if let Some(parents) = self.parents.get(row) {
                todo.extend(parents);
            }
        }
        seen
    }

    fn walk(&self, start: usize, direction: Direction, tips: &HashSet<usize>) -> Option<usize> {
        match direction {
            Direction::Parent => self.parents.get(start)?.first().copied(),
            Direction::Child => self.children.get(start)?.first().copied(),
            Direction::Merge | Direction::Tip => {
                let mut seen = HashSet::from([start]);
                let mut todo = VecDeque::from([start]);
                while let Some(row) = todo.pop_front() {
                    if row != start
                        && match direction {
                            Direction::Merge => self.parents.get(row).is_some_and(|p| p.len() > 1),
                            _ => tips.contains(&row),
                        }
                    {
                        return Some(row);
                    }
                    for child in self.children.get(row).into_iter().flatten() {
                        if seen.insert(*child) {
                            todo.push_back(*child);
                        }
                    }
                }
                None
            }
        }
    }
}

impl RepoTab {
    pub(super) fn graph_walk(
        &mut self,
        direction: Direction,
        ctx: &egui::Context,
    ) -> Option<usize> {
        let start = self.selected.unwrap_or(0);
        let tips = self
            .refs
            .local
            .iter()
            .chain(self.refs.remotes.iter().flat_map(|(_, refs)| refs))
            .filter_map(|branch| self.index.get(&branch.target).copied())
            .collect();
        let next = self
            .topology
            .walk(start, direction, &tips)
            .filter(|row| !self.is_wip(*row) && self.stash_at(*row).is_none());
        if let Some(next) = next {
            // A collapsed span is expanded locally; raw row identity is never treated as a
            // projected offset. Filtering remains intact, with the selected commit as context.
            self.saved_views.dirty = true;
            if crate::settings::Settings::get(ctx).graph_sounds {
                let merge = self.commits.get(next).is_some_and(|c| c.parents.len() > 1);
                let crossing = self
                    .graph
                    .rows
                    .get(start)
                    .zip(self.graph.rows.get(next))
                    .is_some_and(|(a, b)| a.col != b.col);
                if merge {
                    crate::audio::play(crate::audio::Cue::Merge);
                } else if crossing {
                    crate::audio::play(crate::audio::Cue::Lane);
                }
            }
            Some(next)
        } else {
            self.status = Some(("No loaded commit in that direction; older or hidden history may continue beyond this view".into(), false));
            None
        }
    }

    pub(super) fn graph_description(&self, row: usize) -> String {
        let Some(commit) = self.commits.get(row) else {
            return String::new();
        };
        let parents = &commit.parents;
        let children = self.topology.children.get(row).map_or(0, Vec::len);
        let shape = if parents.len() > 1 {
            "merge"
        } else if children > 1 {
            "divergence"
        } else if parents.is_empty() {
            "root"
        } else {
            "commit"
        };
        let mut description = format!(
            "{shape}; {} parents, {children} loaded children",
            parents.len()
        );
        if parents.iter().any(|hash| !self.index.contains_key(hash)) {
            description.push_str("; parents continue beyond loaded history");
        }
        for branch in self.refs.local.iter().filter(|b| b.target == commit.hash) {
            description.push_str(&format!("; branch {}", branch.name));
            if !branch.upstream.is_empty() {
                if branch.upstream_gone() {
                    description.push_str("; upstream removed");
                } else {
                    let (ahead, behind) = branch.ahead_behind();
                    description.push_str(&format!(
                        ", {ahead} ahead and {behind} behind {}",
                        branch.upstream
                    ));
                }
            }
        }
        description
    }

    fn update_graph_narration(&mut self, ctx: &egui::Context) {
        if !crate::settings::Settings::get(ctx).graph_lane_walk {
            return;
        }
        if let Some(reader) = &self.graph_narration.reader {
            match reader.try_recv() {
                Ok(result) => {
                    self.graph_narration.result = Some(result);
                    self.graph_narration.reader = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.graph_narration.reader = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let Some(selected) = self.selected else {
            return;
        };
        let Some(commit) = self.commits.get(selected) else {
            return;
        };
        if commit.hash == self.graph_narration.hash {
            return;
        }
        self.graph_narration = Narration {
            hash: commit.hash.clone(),
            ..Default::default()
        };
        if self.is_wip(selected) || self.stash_at(selected).is_some() {
            return;
        }
        let Some(baseline) = self
            .refs
            .local
            .iter()
            .find(|b| b.name == "main")
            .or_else(|| self.refs.local.iter().find(|b| b.name == "master"))
            .or_else(|| self.refs.local.iter().find(|b| b.is_head))
        else {
            return;
        };
        let Some(&base) = self.index.get(&baseline.target) else {
            return;
        };
        if base == selected {
            return;
        }
        let topology = self.topology.clone();
        let commits = self.commits.clone();
        let name = baseline.name.clone();
        let ctx = ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.graph_narration.reader = Some(rx);
        let _ = std::thread::Builder::new().name("graph-narration".into()).spawn(move || {
            let selected_set = topology.ancestors(selected);
            let base_set = topology.ancestors(base);
            let ahead = selected_set.difference(&base_set).count();
            let behind = base_set.difference(&selected_set).count();
            let common = selected_set.intersection(&base_set).copied().min();
            let merges = base_set.iter().filter(|row| topology.parents.get(**row).is_some_and(|p| p.len() > 1)).count();
            let complete = selected_set.union(&base_set).all(|row| commits.get(*row).is_some_and(|c| c.parents.len() == topology.parents.get(*row).map_or(0, Vec::len)));
            let mut result = format!("Selected history: {ahead} ahead and {behind} behind {name} within {} loaded commits; {merges} merge commits in {name}'s loaded ancestry", commits.len());
            if let Some(common) = common.and_then(|row| commits.get(row)) {
                result.push_str(&format!("; shared ancestor {} (author date {})", common.hash.get(..8).unwrap_or(&common.hash), common.date));
            } else { result.push_str("; no shared ancestor in loaded history"); }
            if !complete { result.push_str("; comparison is partial: unloaded ancestors may change these counts"); }
            let _ = tx.send(result); ctx.request_repaint();
        });
    }

    /// Selected-commit comparison is computed on a worker with pinned immutable topology.
    /// The header reports only local observations; it never implies unloaded ancestry is absent.
    pub(super) fn graph_walk_header(&mut self, ui: &mut egui::Ui) {
        let mut settings = crate::settings::Settings::get(ui.ctx());
        self.update_graph_narration(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut settings.graph_lane_walk, "Walk graph topology")
                .changed()
            {
                settings.set(ui.ctx());
            }
            if settings.graph_lane_walk {
                ui.small("↓ parent · ↑ child · → next descendant merge · ← branch tip");
                if let Some(row) = self.selected {
                    ui.small(self.graph_description(row));
                }
                if let Some(description) = &self.graph_narration.result {
                    ui.small(description);
                }
                if ui
                    .checkbox(&mut settings.graph_sounds, "Graph sounds")
                    .changed()
                {
                    settings.set(ui.ctx());
                }
                if crate::audio::unavailable() {
                    ui.small("Audio unavailable; navigation and descriptions remain available");
                    if ui.button("Retry audio").clicked() {
                        crate::audio::retry();
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_edges_instead_of_adjacent_rows() {
        let commits: Vec<_> = [
            ("merge", vec!["main", "topic"]),
            ("unrelated", vec![]),
            ("topic", vec!["root"]),
            ("main", vec!["root"]),
            ("root", vec![]),
        ]
        .into_iter()
        .map(|(hash, parents)| {
            std::sync::Arc::new(crate::git::Commit {
                hash: hash.into(),
                parents: parents.into_iter().map(String::from).collect(),
                ..Default::default()
            })
        })
        .collect();
        let index = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let topology = Topology::build(&commits, &index);
        let tips = HashSet::from([0, 1]);
        assert_eq!(topology.walk(0, Direction::Parent, &tips), Some(3));
        assert_eq!(topology.walk(2, Direction::Child, &tips), Some(0));
        assert_eq!(topology.walk(4, Direction::Merge, &tips), Some(0));
        assert_eq!(topology.walk(4, Direction::Tip, &tips), Some(0));
        assert_eq!(topology.walk(1, Direction::Parent, &tips), None);
        assert_eq!(topology.ancestors(2), HashSet::from([2, 4]));
    }
}
