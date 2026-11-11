//! Walking real loaded topology; never fetches or changes the Saved Views projection.
#[path = "core_seams/graph_narration.rs"]
mod graph_narration;

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
    reader: super::history_reader::Reader<String>,
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
        ignored: &HashSet<String>,
    ) -> Self {
        let mut out = Self {
            parents: vec![Vec::new(); commits.len()],
            children: vec![Vec::new(); commits.len()],
        };
        for (i, commit) in commits.iter().enumerate() {
            if commit.hash == super::WIP_HASH || ignored.contains(&commit.hash) {
                continue;
            }
            for parent in &commit.parents {
                if ignored.contains(parent) || parent == super::WIP_HASH {
                    continue;
                }
                if let Some(&p) = index.get(parent) {
                    out.parents[i].push(p);
                    out.children[p].push(i);
                }
            }
        }
        out
    }

    fn ancestors(&self, start: usize, active: &dyn Fn() -> bool) -> HashSet<usize> {
        let mut seen = HashSet::new();
        let mut todo = vec![start];
        while let Some(row) = todo.pop() {
            if !active() {
                break;
            }
            if !seen.insert(row) {
                continue;
            }
            if let Some(parents) = self.parents.get(row) {
                todo.extend(parents);
            }
        }
        seen
    }

    /// Count introductions on this release's first-parent line only. Merges
    /// nested in a merged branch are not integrations performed on the release.
    pub(super) fn release_integrations(
        &self,
        tip: usize,
        descendants: &HashSet<usize>,
        commits: &[std::sync::Arc<crate::git::Commit>],
        active: &dyn Fn() -> bool,
    ) -> Result<(usize, bool), String> {
        let mut at = tip;
        let mut seen = HashSet::new();
        let mut count = 0;
        let mut complete = true;
        while seen.insert(at) {
            if !active() {
                return Err(crate::i18n::text("repo-core-2747").into());
            }
            let Some(commit) = commits.get(at) else {
                return Ok((count, false));
            };
            let Some(first) = commit.parents.first() else {
                break;
            };
            let loaded = self.parents.get(at).map(Vec::as_slice).unwrap_or_default();
            complete &= loaded.len() == commit.parents.len();
            let Some(parent) = loaded.iter().copied().find(|p| commits[*p].hash == *first) else {
                return Ok((count, false));
            };
            if commit.parents.len() > 1
                && !descendants.contains(&parent)
                && loaded
                    .iter()
                    .any(|p| *p != parent && descendants.contains(p))
            {
                count += 1;
            }
            at = parent;
        }
        Ok((count, complete))
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
        focused: Option<usize>,
        ctx: &egui::Context,
    ) -> Option<usize> {
        let current = self.selected.or(focused);
        let start = current.unwrap_or(0);
        let tips = self
            .refs
            .local
            .iter()
            .chain(self.refs.remotes.iter().flat_map(|(_, refs)| refs))
            .filter_map(|branch| self.index.get(&branch.target).copied())
            .collect();
        let real = |row: usize| {
            row < self.commits.len() && !self.is_wip(row) && self.stash_at(row).is_none()
        };
        // Worktree and stash rows deliberately have no topology edges. Enter the
        // actual history on the first arrow instead of trapping their keyboard focus.
        let next = if current.is_none() || !real(start) {
            self.index
                .get(&self.refs.head)
                .copied()
                .filter(|row| real(*row))
                .or_else(|| (0..self.commits.len()).find(|row| real(*row)))
        } else {
            self.topology
                .walk(start, direction, &tips)
                .filter(|row| real(*row))
        };
        if let Some(next) = next {
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
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2748").into(),
                false,
            )));
            None
        }
    }

    pub(super) fn graph_description(&self, row: usize) -> String {
        let Some(commit) = self.commits.get(row) else {
            return String::new();
        };
        if self.topology.parents.len() != self.commits.len() {
            return crate::i18n::text("repo-core-2749").into();
        }
        let parents = &commit.parents;
        let children = self.topology.children.get(row).map_or(0, Vec::len);
        let shape = if parents.len() > 1 {
            "merge"
        } else if children > 1 {
            crate::i18n::text("repo-core-2750")
        } else if parents.is_empty() {
            crate::i18n::text("repo-core-2751")
        } else {
            "commit"
        };
        let mut description = crate::i18n::message(
            "repo-core-2752",
            &[
                ("shape", shape.to_string().into()),
                ("count", (parents.len()).into()),
                ("children", (children).into()),
            ],
        );
        if parents.iter().any(|hash| !self.index.contains_key(hash)) {
            description.push_str(crate::i18n::text("repo-core-2753"));
        }
        for branch in self.refs.local.iter().filter(|b| b.target == commit.hash) {
            description.push_str(&crate::i18n::message(
                "repo-core-2754",
                &[("name", branch.name.to_string().into())],
            ));
            if !branch.upstream.is_empty() {
                if branch.upstream_gone() {
                    description.push_str(crate::i18n::text("repo-core-2755"));
                } else {
                    let (ahead, behind) = branch.ahead_behind();
                    description.push_str(&crate::i18n::message(
                        "repo-core-2756",
                        &[
                            ("ahead", (ahead).into()),
                            ("behind", (behind).into()),
                            ("upstream", branch.upstream.to_string().into()),
                        ],
                    ));
                }
            }
        }
        if self.selected == Some(row)
            && self.graph_narration.hash == commit.hash
            && let Some(narration) = &self.graph_narration.result
        {
            description.push_str("; ");
            description.push_str(narration);
        }
        description
    }

    pub(super) fn queue_graph_topology(&mut self) {
        self.topology = Default::default();
        self.graph_narration.hash.clear();
        self.graph_narration.result = None;
        self.graph_narration.reader.cancel();
        let commits = self.commits.clone();
        let index = self.index.clone();
        let ignored = self.stashes.iter().map(|s| s.hash.clone()).collect();
        self.topology_reader.queue(move |active| {
            if !active() {
                return Err(crate::i18n::text("repo-core-2757").into());
            }
            let topology = Topology::build(&commits, &index, &ignored);
            if !active() {
                return Err(crate::i18n::text("repo-core-2757").into());
            }
            Ok(std::sync::Arc::new(topology))
        });
    }
    pub(super) fn poll_graph_topology(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.topology_reader.poll(&self.tx, ctx) {
            match result {
                Ok(topology) => {
                    self.topology = topology;
                    self.graph_narration.hash.clear();
                }
                Err(error) => self.status = Some(crate::repo::Notice::plain((error, true))),
            }
        }
    }
}

#[cfg(test)]
#[path = "direct_tests/graph_walk_tests.rs"]
mod tests;
