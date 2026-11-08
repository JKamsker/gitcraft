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
    fn release_integrations(
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
            self.status = Some((crate::i18n::text("repo-core-2748").into(), false));
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
                Err(error) => self.status = Some((error, true)),
            }
        }
    }
    fn update_graph_narration(&mut self, ctx: &egui::Context) {
        if !crate::settings::Settings::get(ctx).graph_lane_walk {
            self.graph_narration.reader.cancel();
            self.graph_narration.hash.clear();
            self.graph_narration.result = None;
            return;
        }
        if let Some(result) = self.graph_narration.reader.poll(&self.tx, ctx) {
            self.graph_narration.result = Some(result.unwrap_or_else(|e| e));
        }
        if self.topology.parents.len() != self.commits.len() {
            return;
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
        self.graph_narration.hash = commit.hash.clone();
        self.graph_narration.result = None;
        self.graph_narration.reader.cancel();
        if self.is_wip(selected) || self.stash_at(selected).is_some() {
            return;
        }
        let Some(baseline) = self
            .refs
            .local
            .iter()
            .find(|b| matches!(b.name.as_str(), "main" | "master" | "trunk"))
            .or_else(|| self.refs.local.iter().find(|b| b.is_head))
        else {
            return;
        };
        let Some(&base) = self.index.get(&baseline.target) else {
            return;
        };
        let name = baseline.name.clone();
        let topology = self.topology.clone();
        let commits = self.commits.clone();
        let releases: Vec<_> = self
            .refs
            .local
            .iter()
            .filter(|b| b.name.starts_with("release/"))
            .filter_map(|b| self.index.get(&b.target).map(|row| (b.name.clone(), *row)))
            .take(20)
            .collect();
        self.graph_narration.reader.queue(move |active| {
            let selected_set = topology.ancestors(selected, active);
            let base_set = topology.ancestors(base, active);
            if !active() {
                return Err(crate::i18n::text("repo-core-2747").into());
            }
            let ahead = selected_set.difference(&base_set).count();
            let behind = base_set.difference(&selected_set).count();
            let common = selected_set.intersection(&base_set).copied().min();
            let complete = selected_set.union(&base_set).all(|row| {
                commits.get(*row).is_some_and(|c| {
                    c.parents.len() == topology.parents.get(*row).map_or(0, Vec::len)
                })
            });
            let mut result = crate::i18n::message(
                "repo-core-2758",
                &[
                    ("ahead", (ahead).into()),
                    ("behind", (behind).into()),
                    ("name", name.to_string().into()),
                    ("count", (commits.len()).into()),
                ],
            );
            if let Some(common) = common.and_then(|row| commits.get(row)) {
                result.push_str(&crate::i18n::message(
                    "repo-core-2759",
                    &[
                        (
                            "hash",
                            common
                                .hash
                                .get(..8)
                                .unwrap_or(&common.hash)
                                .to_string()
                                .into(),
                        ),
                        ("date", common.date.to_string().into()),
                    ],
                ));
            } else {
                result.push_str(crate::i18n::text("repo-core-2760"));
            }
            let mut descendants = HashSet::new();
            let mut todo = vec![selected];
            while let Some(row) = todo.pop() {
                if !active() {
                    return Err(crate::i18n::text("repo-core-2747").into());
                }
                if descendants.insert(row) {
                    todo.extend(topology.children.get(row).into_iter().flatten());
                }
            }
            for (release, tip) in releases {
                let (merges, line_complete) =
                    topology.release_integrations(tip, &descendants, &commits, active)?;
                result.push_str(&crate::i18n::message(
                    "repo-core-2761",
                    &[
                        ("merges", format!("{}", merges).into()),
                        ("release", release.to_string().into()),
                    ],
                ));
                if !line_complete {
                    result.push_str(crate::i18n::text("repo-core-2762"));
                }
            }
            if !complete {
                result.push_str(crate::i18n::text("repo-core-2763"));
            }
            if !active() {
                return Err(crate::i18n::text("repo-core-2747").into());
            }
            Ok(result)
        });
    }

    /// Selected-commit comparison is computed on a worker with pinned immutable topology.
    /// The header reports only local observations; it never implies unloaded ancestry is absent.
    pub(super) fn graph_walk_header(&mut self, ui: &mut egui::Ui) {
        let mut settings = crate::settings::Settings::get(ui.ctx());
        self.update_graph_narration(ui.ctx());
        if !settings.graph_lane_walk {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(
                    &mut settings.graph_lane_walk,
                    crate::i18n::text("repo-core-0222"),
                )
                .changed()
            {
                settings.set(ui.ctx());
            }
            if settings.graph_lane_walk {
                ui.small(crate::i18n::message(
                    "repo-core-2764",
                    &[
                        (
                            "GraphNext",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphNext)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphPrevious",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphPrevious)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphRight",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphRight)
                                .to_string()
                                .into(),
                        ),
                        (
                            "GraphLeft",
                            crate::keymap::label(ui.ctx(), crate::keymap::Command::GraphLeft)
                                .to_string()
                                .into(),
                        ),
                    ],
                ));
                if let Some(row) = self.selected {
                    let relations: Vec<_> = self
                        .topology
                        .parents
                        .get(row)
                        .into_iter()
                        .flatten()
                        .map(|r| (crate::i18n::text("repo-core-0882"), *r))
                        .chain(
                            self.topology
                                .children
                                .get(row)
                                .into_iter()
                                .flatten()
                                .map(|r| (crate::i18n::text("repo-core-2765"), *r)),
                        )
                        .collect();
                    let mut chosen = None;
                    ui.menu_button(crate::i18n::text("repo-core-2766"), |ui| {
                        for (kind, row) in relations {
                            if let Some(c) = self.commits.get(row)
                                && ui
                                    .button(crate::i18n::message(
                                        "repo-core-2767",
                                        &[
                                            ("kind", kind.to_string().into()),
                                            (
                                                "hash",
                                                c.hash
                                                    .get(..8)
                                                    .unwrap_or(&c.hash)
                                                    .to_string()
                                                    .into(),
                                            ),
                                            ("summary", c.summary.to_string().into()),
                                        ],
                                    ))
                                    .clicked()
                            {
                                chosen = Some(row);
                                ui.close();
                            }
                        }
                    });
                    if let Some(row) = chosen {
                        self.select(row, ui.ctx());
                        self.scroll_to = Some(row);
                    }
                }
                if let Some(row) = self.selected {
                    ui.small(self.graph_description(row));
                }
                if self
                    .selected
                    .and_then(|row| self.commits.get(row))
                    .is_some_and(|c| c.hash == self.graph_narration.hash)
                    && let Some(description) = &self.graph_narration.result
                {
                    ui.small(description);
                }
                if ui
                    .checkbox(
                        &mut settings.graph_sounds,
                        crate::i18n::text("repo-core-2768"),
                    )
                    .changed()
                {
                    settings.set(ui.ctx());
                }
                if crate::audio::unavailable() {
                    ui.small(crate::i18n::text("repo-core-2769"));
                    if ui.button(crate::i18n::text("repo-core-2770")).clicked() {
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
    fn release_narration_counts_only_first_parent_integrations() -> Result<(), String> {
        // feature -> main -> release; main's merge must not count twice.
        let commits: Vec<_> = [
            ("repeat", vec!["release", "feature"]),
            ("release", vec!["root", "main"]),
            ("direct", vec!["root", "feature"]),
            ("main", vec!["root", "feature"]),
            ("feature", vec!["root"]),
            ("root", vec![]),
            ("partial", vec!["unloaded", "feature"]),
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
        let topology = Topology::build(&commits, &index, &HashSet::new());
        let descendants = HashSet::from([0, 1, 2, 3, 4, 6]);
        for tip in [0, 1, 2] {
            assert_eq!(
                topology.release_integrations(tip, &descendants, &commits, &|| true)?,
                (1, true)
            );
        }
        assert_eq!(
            topology.release_integrations(6, &descendants, &commits, &|| true)?,
            (0, false)
        );
        assert!(
            topology
                .release_integrations(0, &descendants, &commits, &|| false)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn topology_toggle_requeues_cancelled_and_completed_narration() -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/narration-toggle".into(), &ctx);
        tab.commits = vec![std::sync::Arc::new(crate::git::Commit {
            hash: "root".into(),
            ..Default::default()
        })];
        tab.index.insert("root".into(), 0);
        tab.selected = Some(0);
        tab.refs.local.push(crate::git::Branch {
            name: "main".into(),
            target: "root".into(),
            is_head: true,
            upstream: String::new(),
            track: String::new(),
        });
        tab.topology =
            std::sync::Arc::new(Topology::build(&tab.commits, &tab.index, &HashSet::new()));
        tab.graph_narration.hash = "root".into();
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        tab.graph_narration.reader.queue(move |active| {
            entered.send(()).map_err(|e| e.to_string())?;
            released
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            assert!(!active());
            finished.send(()).map_err(|e| e.to_string())?;
            Ok("stale result".into())
        });
        tab.graph_narration.reader.poll(&tab.tx, &ctx);
        entry
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let mut initialized = false;
        let mut harness = Harness::builder()
            .with_size([1000.0, 600.0])
            .build_ui_state(
                move |ui, tab: &mut RepoTab| {
                    if !initialized {
                        crate::settings::Settings {
                            graph_lane_walk: true,
                            ..Default::default()
                        }
                        .set(ui.ctx());
                        initialized = true;
                    }
                    // The settings switch drives the same header lifecycle as Graph tools.
                    let mut settings = crate::settings::Settings::get(ui.ctx());
                    if ui
                        .checkbox(&mut settings.graph_lane_walk, "Topology setting")
                        .changed()
                    {
                        settings.set(ui.ctx());
                    }
                    tab.graph_walk_header(ui);
                },
                tab,
            );
        harness.run_steps(2);
        harness.get_by_label("Topology setting").click();
        harness.run_steps(2);
        assert!(harness.state().graph_narration.hash.is_empty());
        release.send(()).map_err(|e| e.to_string())?;
        finish
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        for _ in 0..2 {
            harness.get_by_label("Topology setting").click();
            harness.run_steps(2);
            let start = Instant::now();
            while harness.state().graph_narration.result.is_none() {
                if start.elapsed() > Duration::from_secs(5) {
                    return Err("Narration did not resume".into());
                }
                std::thread::yield_now();
                harness.run_steps(1);
            }
            assert!(
                harness
                    .state()
                    .graph_narration
                    .result
                    .as_ref()
                    .is_some_and(|s| s.contains("0 ahead and 0 behind main"))
            );
            harness.get_by_label("Topology setting").click();
            harness.run_steps(2);
            assert!(harness.state().graph_narration.result.is_none());
        }
        Ok(())
    }

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
        let topology = Topology::build(&commits, &index, &HashSet::new());
        let tips = HashSet::from([0, 1]);
        assert_eq!(topology.walk(0, Direction::Parent, &tips), Some(3));
        assert_eq!(topology.walk(2, Direction::Child, &tips), Some(0));
        assert_eq!(topology.walk(4, Direction::Merge, &tips), Some(0));
        assert_eq!(topology.walk(4, Direction::Tip, &tips), Some(0));
        assert_eq!(topology.walk(1, Direction::Parent, &tips), None);
        assert_eq!(topology.ancestors(2, &|| true), HashSet::from([2, 4]));
    }
    #[test]
    fn lane_walking_announces_shape_and_records_only_opted_in_merge_cues() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/graph-walk-fixture".into(), &ctx);
        tab.commits = [
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
        tab.index = tab
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        tab.topology =
            std::sync::Arc::new(Topology::build(&tab.commits, &tab.index, &HashSet::new()));
        tab.graph = crate::graph::layout(&tab.commits);
        tab.selected = Some(2);
        let _ = crate::audio::take_recorded();
        assert_eq!(tab.graph_walk(Direction::Child, None, &ctx), Some(0));
        assert!(crate::audio::take_recorded().is_empty());
        let mut settings = crate::settings::Settings::get(&ctx);
        settings.graph_sounds = true;
        settings.set(&ctx);
        assert_eq!(tab.graph_walk(Direction::Child, None, &ctx), Some(0));
        assert_eq!(
            crate::audio::take_recorded(),
            vec![crate::audio::Cue::Merge]
        );
        assert!(tab.graph_description(0).contains("merge; 2 parents"));
        assert!(tab.graph_description(4).contains("divergence"));
        tab.selected = Some(0);
        assert_eq!(tab.graph_walk(Direction::Parent, None, &ctx), Some(3));

        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        tab.rebuild_graph_labels();
        let mut initialized = false;
        let mut harness = Harness::builder().with_size([900.0, 600.0]).build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    return;
                }
                crate::settings::Settings {
                    graph_lane_walk: true,
                    ..Default::default()
                }
                .set(ui.ctx());
                tab.graph_view(ui, &mut Vec::new());
            },
            tab,
        );
        harness.step();
        let label = harness
            .get_by_label_contains("commit merge")
            .accesskit_node()
            .label()
            .unwrap_or_default();
        assert!(label.starts_with("commit merge"), "{label}");
        assert!(label.contains("merge; 2 parents"), "{label}");
    }
}
