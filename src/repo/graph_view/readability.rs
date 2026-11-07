//! Viewport-only sizing and labels. Lane names are prepared once with each history rebuild.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use egui::Ui;

use super::badge::{BadgeKind, badges};
use crate::repo::columns::MIN_REFS_W;
use crate::repo::{RepoTab, WIP_HASH};

pub(super) struct Viewport {
    pub refs_width: f32,
    pub now: i64,
    /// At most one faint label per continuous lane run, on its first visible commit.
    pub labels: HashMap<usize, String>,
}

impl RepoTab {
    pub(in crate::repo) fn rebuild_graph_labels(&mut self) {
        self.graph_labels.clear();
        for (i, commit) in self.commits.iter().enumerate() {
            if commit.refs.is_empty() {
                continue;
            }
            let Some(row) = self.graph.rows.get(i) else {
                continue;
            };
            if let Some(badge) = badges(&commit.refs, &self.refs)
                .into_iter()
                .find(|b| b.kind == BadgeKind::Branch || b.kind == BadgeKind::DetachedHead)
            {
                let name = if badge.kind == BadgeKind::DetachedHead {
                    "detached".to_owned()
                } else {
                    badge.git_ref()
                };
                self.graph_labels.entry(row.origin).or_insert(name);
            }
        }
    }

    pub(super) fn readable_viewport(
        &self,
        ui: &Ui,
        range: Range<usize>,
        shown_lanes: usize,
    ) -> Viewport {
        let mut first = HashMap::new();
        let mut decorated = HashSet::new();
        let mut width = MIN_REFS_W;
        for i in range {
            let Some(row) = self.graph.rows.get(i) else {
                continue;
            };
            if let Some(needed) = self.required_refs_width(ui, i) {
                width = width.max(needed);
                decorated.insert(row.origin);
            }
            if row.col < shown_lanes
                && self.commits[i].hash != WIP_HASH
                && self.stash_at(i).is_none()
            {
                first.entry(row.origin).or_insert(i);
            }
        }
        let labels = first
            .into_iter()
            .filter(|(origin, _)| !decorated.contains(origin))
            .map(|(origin, i)| {
                (
                    i,
                    self.graph_labels
                        .get(&origin)
                        .cloned()
                        .unwrap_or_else(|| "detached".to_owned()),
                )
            })
            .collect();
        Viewport {
            now: crate::git::now(),
            // Stable buckets avoid tiny width changes between similar ref names. Long names
            // keep their ellipsis rather than consuming the description's flexible width.
            refs_width: ((width / 32.0).ceil() * 32.0)
                .clamp(MIN_REFS_W, crate::settings::DEFAULT_REFS_WIDTH),
            labels,
        }
    }
}
