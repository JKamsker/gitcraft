//! Small opt-in acknowledgements, armed by an operation and confirmed by a later read.
use super::RepoTab;
use std::time::{Duration, Instant};

#[derive(Clone)]
enum Event {
    FirstCommit,
    ReleaseTag { name: String, target: String },
    Merge { before: String },
}

#[derive(Default)]
pub(super) struct State {
    candidate: Option<Event>,
    min_read: Option<u64>,
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
        self.celebration.min_read = None;
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
        }) || label == "Continue merge")
            && !self.refs.head.is_empty()
        {
            Some(Event::Merge {
                before: self.refs.head.clone(),
            })
        } else {
            None
        };
    }

    pub(super) fn celebration_done(&mut self, success: bool) {
        if success && self.celebration.candidate.is_some() {
            self.celebration.min_read = Some(self.reads + 1);
        } else {
            self.celebration.candidate = None;
            self.celebration.min_read = None;
        }
    }

    pub(super) fn celebration_loaded(&mut self, read: u64, ctx: &egui::Context) {
        if !self.celebration.min_read.is_some_and(|min| read >= min) {
            return;
        }
        self.celebration.min_read = None;
        let Some(event) = self.celebration.candidate.take() else {
            return;
        };
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
            Event::Merge { before } if !self.refs.head.is_empty() && self.refs.head != before => {
                "Merge completed"
            }
            _ => return,
        };
        let settings = crate::settings::Settings::get(ctx);
        if settings.celebration_sound {
            crate::audio::play(crate::audio::Cue::Celebration);
        }
        if settings.celebration_motion && !super::system_motion::reduced(ctx) {
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
        if progress >= 1.0
            || !settings.celebration_motion
            || super::system_motion::reduced(ui.ctx())
        {
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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(tab.celebration.candidate.is_some());
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
