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
    observed: Option<String>,
    pending: Vec<(u64, Event, Option<String>)>,
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
        self.celebration.observed = None;
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

    pub(super) fn celebration_needs_head(&self) -> bool {
        matches!(
            self.celebration.candidate,
            Some(Event::FirstCommit | Event::Merge { .. })
        )
    }
    /// Both observations come from the same mutation worker, around this operation.
    /// The UI's last loaded HEAD may predate another queued operation.
    pub(super) fn celebration_observed_heads(&mut self, before_head: String, head: String) {
        match self.celebration.candidate.as_mut() {
            Some(Event::Merge { before, .. }) if !before_head.is_empty() => {
                *before = before_head;
            }
            Some(Event::FirstCommit) if before_head.is_empty() => {}
            _ => {
                self.celebration.candidate = None;
            }
        }
        self.celebration.observed = (!head.is_empty()).then_some(head);
    }

    pub(super) fn celebration_done(&mut self, success: bool) {
        if let Some(event) = self.celebration.candidate.take().filter(|_| success) {
            let observed = self.celebration.observed.take();
            if matches!(event, Event::ReleaseTag { .. }) || observed.is_some() {
                self.celebration
                    .pending
                    .push((self.reads + 1, event, observed));
            }
        }
    }

    pub(super) fn celebration_loaded(&mut self, read: u64, ctx: &egui::Context) {
        let pending = std::mem::take(&mut self.celebration.pending);
        for (min_read, event, observed) in pending {
            if read < min_read {
                self.celebration.pending.push((min_read, event, observed));
                continue;
            }
            self.confirm_celebration(event, observed, ctx);
        }
    }

    fn confirm_celebration(&mut self, event: Event, observed: Option<String>, ctx: &egui::Context) {
        let label = match event {
            Event::FirstCommit
                if observed.as_ref().is_some_and(|hash| {
                    self.history
                        .iter()
                        .any(|c| &c.hash == hash && c.parents.is_empty())
                }) =>
            {
                "repo-core-0278"
            }
            Event::ReleaseTag { name, target }
                if self
                    .refs
                    .tags
                    .iter()
                    .any(|b| b.name == name && b.target == target) =>
            {
                "repo-core-0279"
            }
            Event::Merge {
                before,
                require_merge,
            } if observed.as_ref().is_some_and(|hash| {
                hash != &before
                    && self.history.iter().any(|c| {
                        &c.hash == hash
                            && (!require_merge
                                || (c.parents.len() > 1 && c.parents.first() == Some(&before)))
                    })
            }) =>
            {
                "repo-core-0280"
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
                ui.colored_label(t.success, format!("✓ {}", crate::i18n::text(label)));
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
#[path = "direct_tests/celebration_tests.rs"]
mod tests;
