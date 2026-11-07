//! Small opt-in acknowledgements, armed by an operation and confirmed by a later read.
use super::RepoTab;
use std::time::{Duration, Instant};

#[derive(Clone)]
enum Event {
    FirstCommit,
    ReleaseTag { name: String, target: String },
    Merge { before: String, require_merge: bool },
}

#[derive(Default)]
pub(super) struct State {
    candidate: Option<Event>,
    pending: Vec<(u64, Event)>,
    visible: Option<(&'static str, Instant)>,
}

impl RepoTab {
    pub(super) fn celebration_begin(
        &mut self,
        label: &str,
        ops: &[Vec<String>],
        ctx: &egui::Context,
    ) {
        self.celebration.candidate = None;
        let settings = crate::settings::Settings::get(ctx);
        if !settings.celebration_motion && !settings.celebration_sound {
            return;
        }
        let command = |name: &str| {
            ops.iter()
                .find(|op| op.first().is_some_and(|arg| arg == name))
        };
        self.celebration.candidate = if command("commit").is_some()
            && self.refs.head.is_empty()
            && self.history.is_empty()
            && self.refs.local.is_empty()
            && self.refs.tags.is_empty()
            && self
                .refs
                .remotes
                .iter()
                .all(|(_, branches)| branches.is_empty())
        {
            Some(Event::FirstCommit)
        } else if let Some(name) = label.strip_prefix("Create release tag ") {
            command("tag")
                .filter(|op| {
                    !op.iter()
                        .any(|arg| matches!(arg.as_str(), "-d" | "--delete"))
                })
                .and_then(|op| op.last())
                .filter(|_| !self.refs.tags.iter().any(|b| b.name == name))
                .map(|target| Event::ReleaseTag {
                    name: name.into(),
                    target: target.clone(),
                })
        } else if (command("merge").is_some_and(|op| {
            !op.iter()
                .any(|arg| matches!(arg.as_str(), "--abort" | "--quit"))
        }) || label == "Continue merge"
            || command("pull").is_some_and(|op| op.iter().any(|a| a == "--no-rebase")))
            && !self.refs.head.is_empty()
        {
            Some(Event::Merge {
                before: self.refs.head.clone(),
                require_merge: command("pull").is_some(),
            })
        } else {
            None
        };
    }

    pub(super) fn celebration_done(&mut self, success: bool) {
        if let Some(event) = self.celebration.candidate.take().filter(|_| success) {
            self.celebration.pending.push((self.reads + 1, event));
        }
    }

    pub(super) fn celebration_loaded(&mut self, read: u64, ctx: &egui::Context) {
        let pending = std::mem::take(&mut self.celebration.pending);
        for (min_read, event) in pending {
            if read < min_read {
                self.celebration.pending.push((min_read, event));
                continue;
            }
            self.confirm_celebration(event, ctx);
        }
    }

    fn confirm_celebration(&mut self, event: Event, ctx: &egui::Context) {
        let label = match event {
            Event::FirstCommit
                if !self.refs.head.is_empty()
                    && self
                        .history
                        .iter()
                        .any(|c| c.hash == self.refs.head && c.parents.is_empty()) =>
            {
                "First commit created"
            }
            Event::ReleaseTag { name, target }
                if self
                    .refs
                    .tags
                    .iter()
                    .any(|b| b.name == name && b.target == target) =>
            {
                "Release tag created"
            }
            Event::Merge {
                before,
                require_merge,
            } if !self.refs.head.is_empty()
                && self.refs.head != before
                && (!require_merge
                    || self.history.iter().any(|c| {
                        c.hash == self.refs.head
                            && c.parents.len() > 1
                            && c.parents.first() == Some(&before)
                    })) =>
            {
                "Merge completed"
            }
            _ => return,
        };
        let settings = crate::settings::Settings::get(ctx);
        if settings.celebration_sound {
            crate::audio::play(crate::audio::Cue::Celebration);
        }
        if settings.celebration_motion && !reduced(ctx) {
            self.celebration.visible = Some((label, Instant::now()));
            ctx.request_repaint();
        }
    }

    pub(super) fn celebration_ui(&mut self, ui: &mut egui::Ui) {
        let Some((label, started)) = self.celebration.visible else {
            return;
        };
        let settings = crate::settings::Settings::get(ui.ctx());
        let progress = started.elapsed().as_secs_f32() / 0.9;
        if progress >= 1.0 || !settings.celebration_motion || reduced(ui.ctx()) {
            self.celebration.visible = None;
            return;
        }
        let t = crate::theme::Tokens::get(ui.ctx());
        egui::Panel::top("quiet_celebration").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(t.success, format!("✓ {label}"));
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(70.0, 18.0), egui::Sense::hover());
                for dot in 0..6 {
                    let x = rect.left() + 8.0 + dot as f32 * 9.0;
                    let y = rect.center().y
                        - (std::f32::consts::PI * progress).sin() * (3.0 + (dot % 3) as f32 * 2.0);
                    ui.painter().circle_filled(
                        egui::pos2(x, y),
                        1.5,
                        t.success.gamma_multiply(1.0 - progress),
                    );
                }
            });
        });
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

fn reduced(ctx: &egui::Context) -> bool {
    crate::a11y::motion::reduced(ctx) || ctx.global_style().animation_time == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_first_commit_survives_queued_push_then_failure_until_confirmed_read() {
        let ctx = egui::Context::default();
        crate::settings::Settings {
            celebration_sound: true,
            ..Default::default()
        }
        .set(&ctx);
        crate::audio::take_recorded();
        let mut tab = RepoTab::new_deferred("/nonexistent/queued-celebration".into(), &ctx);
        tab.life.phase = super::super::lifecycle::Phase::Live;
        tab.loading = true;
        tab.tx.cancel.cancel(); // queued jobs are rejected, no commands execute
        tab.celebration_begin("Commit", &[vec!["commit".into()]], &ctx);
        tab.busy_op = Some("Commit".into());
        tab.refs.current = Some("main".into());
        tab.refs.local.push(crate::git::Branch {
            name: "main".into(),
            target: "root".into(),
            is_head: true,
            upstream: "origin/main".into(),
            track: String::new(),
        });
        tab.waiting.push_back(super::super::running::Waiter::Push);
        tab.op_done(
            "Commit".into(),
            Ok(String::new()),
            super::super::Refresh::All,
            super::super::OpKind::plain(),
            true,
            &ctx,
        );
        assert_eq!(tab.busy_op.as_deref(), Some("Push"));
        assert_eq!(tab.celebration.pending.len(), 1);
        tab.op_done(
            "Push".into(),
            Err("recorded failure".into()),
            super::super::Refresh::All,
            super::super::OpKind::plain(),
            true,
            &ctx,
        );
        tab.refs.head = "root".into();
        tab.history.push(std::sync::Arc::new(crate::git::Commit {
            hash: "root".into(),
            ..Default::default()
        }));
        tab.celebration_loaded(tab.reads, &ctx);
        assert!(crate::audio::take_recorded().is_empty());
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert_eq!(
            crate::audio::take_recorded(),
            [crate::audio::Cue::Celebration]
        );
        tab.celebration_loaded(tab.reads + 2, &ctx);
        assert!(crate::audio::take_recorded().is_empty());
    }
    #[test]
    fn merging_pull_celebrates_confirmed_merge_but_not_fast_forward_or_failure() {
        let ctx = egui::Context::default();
        crate::settings::Settings {
            celebration_sound: true,
            ..Default::default()
        }
        .set(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/pull-celebration".into(), &ctx);
        let pull = super::super::ops(&[super::super::pull::PullMode::Merge.args()]);
        for (success, parents, expected) in [
            (true, vec!["old", "topic"], 1),
            (true, vec!["old"], 0),
            (false, vec!["old", "topic"], 0),
        ] {
            crate::audio::take_recorded();
            tab.refs.head = "old".into();
            tab.celebration_begin("Sync: pull", &pull, &ctx);
            tab.celebration_done(success);
            tab.celebration_begin("Sync: push", &[vec!["push".into()]], &ctx);
            tab.celebration_done(true);
            tab.refs.head = "new".into();
            tab.history = vec![std::sync::Arc::new(crate::git::Commit {
                hash: "new".into(),
                parents: parents.into_iter().map(String::from).collect(),
                ..Default::default()
            })];
            tab.celebration_loaded(tab.reads + 1, &ctx);
            assert_eq!(crate::audio::take_recorded().len(), expected);
        }
    }
    #[test]
    fn failed_noop_and_unconfirmed_operations_do_not_celebrate() {
        let ctx = egui::Context::default();
        let settings = crate::settings::Settings {
            celebration_sound: true,
            ..Default::default()
        };
        settings.set(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/celebrate".into(), &ctx);
        crate::audio::take_recorded();
        tab.refs.head = "old".into();
        let merge = vec![vec!["merge".into(), "topic".into()]];
        tab.celebration_begin("Merge topic", &merge, &ctx);
        tab.celebration_done(false);
        assert!(tab.celebration.candidate.is_none());
        tab.celebration_begin("Merge topic", &merge, &ctx);
        tab.celebration_done(true);
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert!(tab.celebration.visible.is_none());
        assert!(tab.celebration.candidate.is_none());
        assert!(crate::audio::take_recorded().is_empty());
        tab.celebration_begin("Merge topic", &merge, &ctx);
        tab.celebration_done(true);
        tab.refs.head = "merged".into();
        tab.celebration_loaded(0, &ctx);
        assert_eq!(tab.celebration.pending.len(), 1);
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert!(tab.celebration.candidate.is_none());
        assert_eq!(
            crate::audio::take_recorded(),
            [crate::audio::Cue::Celebration]
        );
        tab.celebration_loaded(tab.reads + 2, &ctx);
        assert!(crate::audio::take_recorded().is_empty());
    }
    #[test]
    fn first_commit_is_opt_in_confirmed_once_and_sound_is_independent_of_motion() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut tab = RepoTab::new_deferred("/nonexistent/first-celebrate".into(), &ctx);
        let commit = vec![vec!["commit".into(), "-m".into(), "first".into()]];
        tab.celebration_begin("Commit", &commit, &ctx);
        assert!(tab.celebration.candidate.is_none());
        crate::settings::Settings {
            celebration_sound: true,
            celebration_motion: true,
            ..Default::default()
        }
        .set(&ctx);
        crate::audio::take_recorded();
        tab.celebration_begin("Commit", &commit, &ctx);
        tab.celebration_done(true);
        tab.refs.head = "root".into();
        tab.history.push(std::sync::Arc::new(crate::git::Commit {
            hash: "root".into(),
            ..Default::default()
        }));
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert!(tab.celebration.visible.is_none());
        assert_eq!(
            crate::audio::take_recorded(),
            [crate::audio::Cue::Celebration]
        );
        tab.celebration_loaded(tab.reads + 2, &ctx);
        assert!(crate::audio::take_recorded().is_empty());
    }
    #[test]
    fn release_requires_explicit_designation_and_confirmed_target() {
        let ctx = egui::Context::default();
        crate::settings::Settings {
            celebration_sound: true,
            ..Default::default()
        }
        .set(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/release-celebrate".into(), &ctx);
        let tag = vec![vec!["tag".into(), "release-any-name".into(), "abc".into()]];
        tab.celebration_begin("Create tag release-any-name", &tag, &ctx);
        assert!(tab.celebration.candidate.is_none());
        tab.celebration_begin("Create release tag release-any-name", &tag, &ctx);
        assert!(matches!(
            tab.celebration.candidate,
            Some(Event::ReleaseTag { .. })
        ));
        tab.celebration_done(true);
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert!(tab.celebration.visible.is_none());
    }
}
