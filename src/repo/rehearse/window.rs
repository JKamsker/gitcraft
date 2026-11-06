//! The rehearsal's window: each operation rehearsed in a card (how it went, the files that
//! conflict, the test, and a button to do it now), and how they compare.

use egui::{RichText, Ui};

use super::report::{compare_line, heading, is_good, played_line, tested_line};
use super::{RehearseAct, RehearseRun};
use crate::git::{Played, Rehearsal, RehearseKind, Tested};
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// The most conflicted files listed in a card.
const MAX_FILES: usize = 8;

impl RepoTab {
    pub(in crate::repo) fn rehearse_window(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(run) = &self.rehearsing.run else {
            return;
        };
        let mut open = true;
        egui::Window::new(format!("Rehearsal: {} with {}", run.branch, run.other))
            .id(egui::Id::new(("gitcraft-rehearsal", self.path.clone())))
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                let idle = self.busy_op.is_none();
                for &kind in &run.kinds {
                    let result = run.results.iter().find(|(k, _)| *k == kind).map(|(_, r)| r);
                    card(ui, run, kind, result, idle && !run.applying, acts);
                    ui.add_space(8.0);
                }
                let t = Tokens::get(ui.ctx());
                if let [(_, Ok(a)), (_, Ok(b))] = run.results.as_slice()
                    && let Some(line) = compare_line(a, b)
                {
                    ui.label(RichText::new(line).color(t.text_dim));
                }
                ui.horizontal(|ui| {
                    if run.ended {
                        ui.label(
                            RichText::new(
                                "Nothing was changed. Doing it now checks first that neither \
                                 branch moved; Ctrl+Z undoes it.",
                            )
                            .color(t.text_faint),
                        );
                    } else {
                        ui.add(crate::spinner::Spinner::new());
                        if run.stop.stopped() {
                            // A rebase or merge under way finishes its step first.
                            ui.label(RichText::new("Stopping…").color(t.text_dim));
                        } else {
                            ui.label(RichText::new("Rehearsing…").color(t.text_dim));
                            if ui.button("Stop").clicked() {
                                acts.push(Action::Rehearse(RehearseAct::Stop));
                            }
                        }
                    }
                });
            });
        if !open {
            acts.push(Action::Rehearse(if run.ended {
                RehearseAct::Close
            } else {
                RehearseAct::Stop
            }));
        }
    }
}

/// One operation's card.
fn card(
    ui: &mut Ui,
    run: &RehearseRun,
    kind: RehearseKind,
    result: Option<&Result<Rehearsal, String>>,
    can_apply: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.chrome)
        .stroke(egui::Stroke::new(1.0, t.separator))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = heading(kind, &run.branch, &run.other);
            ui.label(RichText::new(&title).font(theme::semibold(13.5)));
            match result {
                None if run.ended => {
                    ui.label(RichText::new("Not rehearsed: stopped").color(t.text_dim));
                }
                None => {
                    ui.label(RichText::new("Waiting…").color(t.text_dim));
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("Couldn't rehearse: {e}")).color(t.danger));
                }
                Some(Ok(r)) => outcome(ui, &t, run, kind, r, can_apply, acts),
            }
        });
}

fn outcome(
    ui: &mut Ui,
    t: &Tokens,
    run: &RehearseRun,
    kind: RehearseKind,
    r: &Rehearsal,
    can_apply: bool,
    acts: &mut Vec<Action>,
) {
    let tone = match r.played {
        Played::Stopped { .. } => t.warning,
        Played::Failed(_) => t.danger,
        _ => t.success,
    };
    ui.label(RichText::new(played_line(kind, &r.played)).color(tone));
    if let Played::Stopped { conflicts, .. } = &r.played {
        for file in conflicts.iter().take(MAX_FILES) {
            ui.label(RichText::new(format!("  {file}")).font(theme::mono(11.5)));
        }
        if conflicts.len() > MAX_FILES {
            ui.label(
                RichText::new(format!("  … and {} more", conflicts.len() - MAX_FILES))
                    .color(t.text_dim),
            );
        }
    }
    if let (Some(tested), Some(command)) = (&r.tested, &run.command) {
        let color = match tested {
            Tested::NotRun(_) => t.text_dim,
            _ if is_good(r) => t.success,
            _ => t.danger,
        };
        let resp = ui.label(RichText::new(tested_line(tested, command)).color(color));
        if let Tested::Ran { lines, .. } = tested
            && !lines.is_empty()
        {
            let tail: Vec<&str> = lines
                .iter()
                .rev()
                .take(15)
                .rev()
                .map(String::as_str)
                .collect();
            resp.on_hover_text(RichText::new(tail.join("\n")).font(theme::mono(11.0)));
        }
    }
    if run.ended && !matches!(r.played, Played::UpToDate | Played::Failed(_)) {
        ui.add_space(4.0);
        let label = match kind {
            RehearseKind::Rebase => "Rebase now",
            RehearseKind::Merge => "Merge now",
        };
        let tip = match &r.played {
            Played::Stopped { .. } => {
                "Does it for real: it stops at the same place, for you to solve the conflicts"
            }
            _ => "Does it for real, once it has checked that neither branch moved",
        };
        if widgets::primary_button(ui, label, can_apply, 110.0)
            .on_hover_text(tip)
            .clicked()
        {
            acts.push(Action::Rehearse(RehearseAct::Apply(kind)));
        }
    }
}
