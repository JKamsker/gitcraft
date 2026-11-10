//! Graph selection, keyboard movement and context interactions.

use super::*;

impl RepoTab {
    pub(in crate::repo::graph_view) fn keep_menu_open(
        &self,
        ui: &mut Ui,
        hash: Option<&str>,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(i) = hash.and_then(|h| self.index.get(h)).copied() else {
            return;
        };
        let at = egui::Rect::from_min_size(ui.min_rect().min, vec2(0.0, 0.0));
        let resp = ui.interact(at, row::row_id(ui, &self.commits[i].hash), Sense::click());
        self.row_menu(&resp, i, idle, acts);
    }

    /// Every index conversion follows the projection actually being drawn.
    pub(in crate::repo) fn graph_projection_displayed(&self) -> bool {
        self.saved_views.active() && !self.ghost.drawing() && self.timeline_view.is_none()
    }
    pub(in crate::repo) fn take_graph_scroll_row(&mut self) -> Option<usize> {
        let raw = self.scroll_to.take_if(|_| !self.ghost.drawing())?;
        Some(if self.graph_projection_displayed() {
            self.saved_views.position(raw).unwrap_or(raw)
        } else {
            raw
        })
    }

    /// Same selected-commit value for graph keyboard handling and palette dispatch.
    pub(in crate::repo) fn graph_copy_text(
        &self,
        command: crate::keymap::Command,
        i: usize,
    ) -> Option<String> {
        use crate::keymap::Command;
        let commit = self.commits.get(i).filter(|c| c.hash != WIP_HASH)?;
        match command {
            Command::GraphCopyHash => Some(commit.hash.clone()),
            Command::GraphCopyMessage => Some(commit.summary.clone()),
            Command::GraphCopyBranch => self.copy_branch(i),
            _ => None,
        }
    }

    /// Up and down select the row above or below the selected one.
    /// `ringed`: the row Tab reaches, where they start while nothing is selected.
    pub(in crate::repo::graph_view) fn arrow_keys(
        &mut self,
        ui: &Ui,
        ringed: Option<usize>,
        acts: &mut Vec<Action>,
    ) {
        use crate::keymap::{self, Command};
        keymap::active(ui.ctx(), self.key_scopes(ui.ctx()));
        if let Some(i) = self.selected.or(ringed) {
            for command in [
                Command::GraphCopyHash,
                Command::GraphCopyMessage,
                Command::GraphCopyBranch,
            ] {
                if keymap::take(ui.ctx(), command)
                    && let Some(text) = self.graph_copy_text(command, i)
                {
                    ui.ctx().copy_text(text);
                }
            }
        }
        let up = keymap::take(ui.ctx(), Command::GraphPrevious);
        let down = keymap::take(ui.ctx(), Command::GraphNext);
        let left = keymap::take(ui.ctx(), Command::GraphLeft);
        let right = keymap::take(ui.ctx(), Command::GraphRight);
        if Settings::get(ui.ctx()).graph_lane_walk {
            use crate::repo::graph_walk::Direction;
            let direction = if down {
                Some(Direction::Parent)
            } else if up {
                Some(Direction::Child)
            } else if right {
                Some(Direction::Merge)
            } else if left {
                Some(Direction::Tip)
            } else {
                None
            };
            if let Some(direction) = direction
                && let Some(next) = self.graph_walk(direction, ringed, ui.ctx())
            {
                acts.push(Action::Select(next));
                self.scroll_to = Some(next);
            }
            return;
        }
        let current = self.selected.or(ringed).unwrap_or(0);
        let next = if self.graph_projection_displayed() && (up || down) {
            self.saved_views.next(current, down)
        } else if up {
            Some(current.saturating_sub(1))
        } else if down {
            Some((current + 1).min(self.commits.len().saturating_sub(1)))
        } else {
            None
        };
        if let Some(next) = next {
            acts.push(Action::Select(next));
            self.scroll_to = Some(next);
        }
    }

    /// Right-click menu of row `i`: the changes row's, a stash's or a commit's.
    pub(in crate::repo::graph_view) fn row_menu(
        &self,
        resp: &Response,
        i: usize,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        if commit.hash == WIP_HASH {
            wip_menu(resp, idle, acts);
            return;
        }
        if let Some(stash) = self.stash_at(i).and_then(|s| self.stashes.get(s)) {
            crate::menu_item::context(resp)
                .style(crate::theme::menu_style)
                .show(|ui| crate::repo::sidebar::stash_menu(ui, acts, stash, idle));
            return;
        }
        if let Some(parked) = self.parked_at(i) {
            egui::Popup::context_menu(resp)
                .style(crate::theme::menu_style)
                .show(|ui| self.parked_menu(ui, parked, acts));
            return;
        }
        self.commit_menu(resp, i, idle, acts);
    }

    /// Compares two rows of the graph, the older one first; the changes row stands for the
    /// working tree. `None` for the same row twice.
    pub(in crate::repo::graph_view) fn compare_rows(&self, a: usize, b: usize) -> Option<Action> {
        // A merge put aside isn't a commit to compare (its files are the agent's business).
        if a == b || self.parked_at(a).is_some() || self.parked_at(b).is_some() {
            return None;
        }
        // Rows go newest first.
        let (old, new) = (a.max(b), a.min(b));
        let end = |row: usize| {
            let c = self.commits.get(row)?;
            let label = self
                .graph_badges(c)
                .into_iter()
                .find(|b| b.kind != BadgeKind::DetachedHead)
                .map(|b| b.label)
                .unwrap_or_else(|| crate::repo::short(&c.hash).to_owned());
            Some((c.hash.clone(), label))
        };
        let (base, base_label) = end(old)?;
        let (target, target_label) = if self.is_wip(new) {
            (None, crate::i18n::text("repo-core-2651").to_owned())
        } else {
            let (hash, label) = end(new)?;
            (Some(hash), label)
        };
        Some(Action::Compare {
            base,
            base_label,
            target,
            target_label,
            captions: [None, None],
            since_fork: false,
        })
    }
}
