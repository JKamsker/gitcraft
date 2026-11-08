//! Short, operation-scoped graph transitions. Ordinary reads never create new markers.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::{RepoTab, WIP_HASH};

const SLIDE: f32 = 0.35;
const MARK: Duration = Duration::from_secs(12);
const MAX_MARKS: usize = 512;

#[derive(Default)]
pub(super) struct State {
    before: Option<Snapshot>,
    pending: Option<(u64, Snapshot)>,
    incoming: HashSet<String>,
    moved: HashMap<String, (String, usize)>,
    started: Option<Instant>,
    scroll: Option<(f32, f32, Instant)>,
    follow_head: bool,
}

struct Snapshot {
    fetch: bool,
    known: HashSet<String>,
    refs: HashMap<String, (String, usize)>,
}

impl RepoTab {
    pub(super) fn graph_motion_begin(&mut self, ops: &[Vec<String>]) {
        let fetch = ops
            .iter()
            .any(|op| op.first().is_some_and(|s| s == "fetch"));
        let moves = ops.iter().any(|op| {
            op.first()
                .is_some_and(|s| matches!(s.as_str(), "merge" | "reset" | "rebase" | "pull"))
        });
        self.graph_motion.before = (fetch || moves).then(|| Snapshot {
            fetch,
            known: self.history.iter().map(|c| c.hash.clone()).collect(),
            refs: self.motion_ref_positions(),
        });
    }

    pub(super) fn graph_motion_done(&mut self, success: bool) {
        let Some(before) = self.graph_motion.before.take().filter(|_| success) else {
            return;
        };
        if let Some((min_read, pending)) = &mut self.graph_motion.pending {
            *min_read = (*min_read).max(self.reads + 1);
            // Retain the earliest successful baseline until its confirming read.
            // A failure or an unrelated next operation only retires its own capture.
            if before.fetch && !pending.fetch {
                pending.known = before.known;
            }
            pending.fetch |= before.fetch;
            for (name, position) in before.refs {
                pending.refs.entry(name).or_insert(position);
            }
        } else {
            self.graph_motion.pending = Some((self.reads + 1, before));
        }
    }

    fn motion_ref_positions(&self) -> HashMap<String, (String, usize)> {
        self.refs
            .local
            .iter()
            .map(|b| (format!("refs/heads/{}", b.name), &b.target))
            .chain(self.refs.remotes.iter().flat_map(|(_, bs)| {
                bs.iter()
                    .map(|b| (format!("refs/remotes/{}", b.name), &b.target))
            }))
            .chain(
                self.refs
                    .tags
                    .iter()
                    .map(|b| (format!("refs/tags/{}", b.name), &b.target)),
            )
            .filter_map(|(name, hash)| {
                let raw = *self.index.get(hash)?;
                let visual = raw;
                Some((name, (hash.clone(), visual)))
            })
            .collect()
    }

    pub(super) fn graph_motion_loaded(&mut self, read: u64, ctx: &egui::Context) {
        if self
            .graph_motion
            .pending
            .as_ref()
            .is_none_or(|(min, _)| read < *min)
        {
            return;
        }
        let Some((_, before)) = self.graph_motion.pending.take() else {
            return;
        };
        self.graph_motion.incoming = if before.fetch {
            self.history
                .iter()
                .filter(|c| c.hash != WIP_HASH && !before.known.contains(&c.hash))
                .take(MAX_MARKS)
                .map(|c| c.hash.clone())
                .collect()
        } else {
            HashSet::new()
        };
        self.graph_motion.moved = self
            .motion_ref_positions()
            .into_iter()
            .filter_map(|(name, (hash, _))| {
                let (old_hash, old_row) = before.refs.get(&name)?;
                (old_hash != &hash).then_some((name, (hash, *old_row)))
            })
            .take(MAX_MARKS)
            .collect();
        self.graph_motion.started = Some(Instant::now());
        self.scroll_to = self.index.get(&self.refs.head).copied();
        self.graph_motion.follow_head = self.scroll_to.is_some();
        ctx.request_repaint();
    }
}

impl State {
    pub(super) fn take_head_scroll(&mut self) -> bool {
        std::mem::take(&mut self.follow_head)
    }
    pub(super) fn expire(&mut self) {
        if self.started.is_some_and(|t| t.elapsed() >= MARK) {
            self.incoming.clear();
            self.moved.clear();
            self.started = None;
        }
    }

    pub(super) fn schedule_expiry(&self, ctx: &egui::Context) {
        if let Some(started) = self.started
            && !self.incoming.is_empty()
        {
            ctx.request_repaint_after(MARK.saturating_sub(started.elapsed()));
        }
    }

    pub(super) fn new_commit(&self, hash: &str) -> bool {
        self.incoming.contains(hash)
    }

    pub(super) fn highlight(&self, ctx: &egui::Context) -> f32 {
        let elapsed = self.started.map_or(1.0, |t| t.elapsed().as_secs_f32());
        if reduced_motion(ctx) || elapsed >= 1.0 {
            return 0.0;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        (1.0 - elapsed) * 0.16
    }

    pub(super) fn pill_offset(
        &self,
        ctx: &egui::Context,
        name: &str,
        hash: &str,
        row: usize,
    ) -> f32 {
        if reduced_motion(ctx) {
            return 0.0;
        }
        let Some((destination, old)) = self.moved.get(name) else {
            return 0.0;
        };
        let progress = self
            .started
            .map_or(1.0, |t| t.elapsed().as_secs_f32() / SLIDE)
            .min(1.0);
        if destination != hash || progress >= 1.0 {
            return 0.0;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        ((*old as f32 - row as f32) * super::graph_view::ROW_PITCH).clamp(-360.0, 360.0)
            * (1.0 - progress).powi(3)
    }

    pub(super) fn start_scroll(&mut self, from: f32, to: f32, ctx: &egui::Context) -> f32 {
        if reduced_motion(ctx) {
            self.scroll = None;
            return to;
        }
        self.scroll = Some((from, to, Instant::now()));
        from
    }

    pub(super) fn scroll_offset(&mut self, ctx: &egui::Context) -> Option<f32> {
        let (from, to, started) = self.scroll?;
        let progress = (started.elapsed().as_secs_f32() / SLIDE).min(1.0);
        if progress >= 1.0 || reduced_motion(ctx) {
            self.scroll = None;
            return Some(to);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        Some(from + (to - from) * (1.0 - (1.0 - progress).powi(3)))
    }

    pub(super) fn cancel_scroll(&mut self) {
        self.scroll = None;
    }
}

/// Uses egui's animation policy and the graph-specific off switch.
pub(super) fn reduced_motion(ctx: &egui::Context) -> bool {
    crate::a11y::motion::reduced(ctx)
        || ctx.global_style().animation_time == 0.0
        || !crate::settings::Settings::get(ctx).graph_motion
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_push_and_its_failure_do_not_erase_successful_fetch_feedback() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/queued-motion".into(), &ctx);
        // Exercise real op_done -> after_op -> Push hooks with all task dispatch
        // cancelled before submission. No subprocess or filesystem read is needed.
        tab.life.phase = super::super::lifecycle::Phase::Live;
        tab.loading = true; // delay the confirming reload
        tab.tx.cancel.cancel();
        tab.refs.current = Some("main".into());
        tab.refs.local.push(crate::git::Branch {
            name: "main".into(),
            target: "old".into(),
            is_head: true,
            upstream: "origin/main".into(),
            track: String::new(),
        });
        tab.graph_motion_begin(&[vec!["fetch".into()]]);
        tab.busy_op = Some("Fetch".into());
        tab.waiting.push_back(super::super::running::Waiter::Push);
        tab.op_done(
            "Fetch".into(),
            Ok(String::new()),
            super::super::Refresh::All,
            super::super::OpKind::plain(),
            true,
            &ctx,
        );
        assert_eq!(tab.busy_op.as_deref(), Some("Push"));
        assert!(tab.graph_motion.pending.is_some());
        tab.op_done(
            "Push".into(),
            Err("recorded failure".into()),
            super::super::Refresh::All,
            super::super::OpKind::plain(),
            true,
            &ctx,
        );
        assert!(tab.graph_motion.pending.is_some());
        tab.history.push(std::sync::Arc::new(crate::git::Commit {
            hash: "incoming".into(),
            ..Default::default()
        }));
        tab.graph_motion_loaded(tab.reads, &ctx);
        assert!(!tab.graph_motion.new_commit("incoming"));
        tab.graph_motion_loaded(tab.reads + 1, &ctx);
        assert!(tab.graph_motion.new_commit("incoming"));
        assert!(tab.graph_motion.pending.is_none());
    }
    #[test]
    fn chained_successes_keep_earliest_refs_and_failed_fetch_adds_no_markers() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/chained-motion".into(), &ctx);
        tab.refs.local.push(crate::git::Branch {
            name: "main".into(),
            target: "old".into(),
            is_head: true,
            upstream: String::new(),
            track: String::new(),
        });
        tab.index.insert("old".into(), 5);
        tab.graph_motion_begin(&[vec!["pull".into()]]);
        tab.graph_motion_done(true);
        tab.reads += 1; // a read already in flight cannot confirm this next success
        tab.graph_motion_begin(&[vec!["rebase".into()]]);
        tab.graph_motion_done(true);
        tab.graph_motion_begin(&[vec!["fetch".into()]]);
        tab.graph_motion_done(false);
        tab.refs.local[0].target = "new".into();
        tab.index.insert("new".into(), 0);
        tab.history.push(std::sync::Arc::new(crate::git::Commit {
            hash: "new".into(),
            ..Default::default()
        }));
        tab.graph_motion_loaded(tab.reads, &ctx);
        assert!(tab.graph_motion.pending.is_some());
        tab.graph_motion_loaded(tab.reads + 1, &ctx);
        assert_eq!(
            tab.graph_motion.moved.get("refs/heads/main"),
            Some(&("new".into(), 5))
        );
        assert!(!tab.graph_motion.new_commit("new"));
    }
    #[test]
    fn markers_are_operation_scoped_and_bounded() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/motion".into(), &ctx);
        tab.graph_motion.incoming.insert("old".into());
        tab.graph_motion.started = Some(Instant::now() - MARK);
        tab.graph_motion.expire();
        assert!(!tab.graph_motion.new_commit("old"));
        tab.graph_motion_begin(&[vec!["fetch".into()]]);
        tab.graph_motion_done(false);
        assert!(tab.graph_motion.before.is_none());
        tab.graph_motion_loaded(u64::MAX, &ctx);
        assert!(tab.graph_motion.incoming.is_empty());
    }
    #[test]
    fn a_successful_fetch_marks_new_history_once_and_ignores_early_reads() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/motion-success".into(), &ctx);
        let old = std::sync::Arc::new(crate::git::Commit {
            hash: "old".into(),
            ..Default::default()
        });
        tab.history.push(old.clone());
        tab.graph_motion_begin(&[vec!["fetch".into()]]);
        tab.graph_motion_done(true);
        tab.history.insert(
            0,
            std::sync::Arc::new(crate::git::Commit {
                hash: "fresh".into(),
                ..Default::default()
            }),
        );
        tab.graph_motion_loaded(0, &ctx);
        assert!(!tab.graph_motion.new_commit("fresh"));
        tab.graph_motion_loaded(tab.reads + 1, &ctx);
        assert!(tab.graph_motion.new_commit("fresh"));
        assert!(!tab.graph_motion.new_commit("old"));
        tab.history.push(std::sync::Arc::new(crate::git::Commit {
            hash: "ordinary".into(),
            ..Default::default()
        }));
        tab.graph_motion_loaded(tab.reads + 2, &ctx);
        assert!(!tab.graph_motion.new_commit("ordinary"));
    }
    #[test]
    fn reduced_motion_finishes_scroll_without_repaints() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut state = State::default();
        assert_eq!(state.start_scroll(0.0, 500.0, &ctx), 500.0);
        assert!(state.scroll.is_none());
    }
}
