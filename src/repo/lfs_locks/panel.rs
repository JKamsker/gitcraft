//! The LFS locks panel: every lock on the server, filtered by path or owner, unlocked several at
//! once (someone else's by force, for admins), and the patterns that make files lockable.

use std::collections::BTreeSet;

use egui::{RichText, Ui};

use super::banner::lock_color;
use super::{LockAct, LockState, Panel, lock_label};
use crate::git::{self, LfsLock};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn locks_panel(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let state = &mut self.lfs_locks;
        let Some(mut panel) = state.panel.take() else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut close = false;
        let resp = egui::Modal::new(egui::Id::new("lfs-locks"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                close = header(ui, &t, state, acts);
                ui.add_space(8.0);
                filters(ui, &mut panel);
                ui.add_space(6.0);
                let shown: Vec<&LfsLock> = state.shown(&panel);
                lock_list(ui, &t, state, &shown, &mut panel);
                ui.add_space(8.0);
                unlock_buttons(ui, &t, state, &shown, &mut panel, acts);
                ui.add_space(12.0);
                ui.separator();
                patterns(ui, &t, state, &mut panel, acts);
            });
        if close || resp.should_close() {
            acts.push(Action::LfsLocks(LockAct::ClosePanel));
        }
        self.lfs_locks.panel = Some(panel);
    }
}

/// Title, refresh and close, and what went wrong; true when closed.
fn header(ui: &mut Ui, t: &Tokens, state: &LockState, acts: &mut Vec<Action>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        icons::inline(ui, "lock", 16.0, t.icon);
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-lfs_locks-header-b815cb09",
            ))
            .font(theme::semibold(16.0)),
        );
        if state.reading.is_some() {
            ui.add(crate::spinner::Spinner::new());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(
                ui,
                "x",
                crate::i18n::text("repo-workflows-lfs_locks-header-961b2a04"),
            )
            .clicked()
            {
                close = true;
            }
            let reading = state.reading.is_some();
            if !reading
                && icons::small_button(
                    ui,
                    "refresh-cw",
                    crate::i18n::text("repo-workflows-lfs_locks-header-2e532790"),
                )
                .clicked()
            {
                acts.push(Action::LfsLocks(LockAct::Refresh));
            }
        });
    });
    if let Some(e) = &state.error {
        ui.add(egui::Label::new(RichText::new(state.display_read_error(e)).color(t.danger)).wrap());
    }
    if let Some(e) = &state.job_error {
        ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    }
    if let Some(busy) = &state.busy {
        ui.label(
            RichText::new(format!(
                "{}…",
                crate::repo::messages::display_operation(busy)
            ))
            .color(t.text_dim),
        );
    }
    close
}

fn filters(ui: &mut Ui, panel: &mut Panel) {
    ui.horizontal(|ui| {
        let typed = ui.add(
            egui::TextEdit::singleline(&mut panel.filter)
                .hint_text(crate::i18n::text(
                    "repo-workflows-lfs_locks-filters-53518a4c",
                ))
                .desired_width(320.0),
        );
        let mine = ui.checkbox(
            &mut panel.only_mine,
            crate::i18n::text("repo-workflows-lfs_locks-filters-5da4f78e"),
        );
        if typed.changed() || mine.changed() {
            panel.confirm_force = None;
        }
    });
}

fn lock_list(ui: &mut Ui, t: &Tokens, state: &LockState, shown: &[&LfsLock], panel: &mut Panel) {
    let now = git::now();
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown.is_empty() {
                let note = if state.locks.is_empty() {
                    if state.reading.is_some() {
                        Some(crate::i18n::text(
                            "repo-workflows-lfs_locks-lock-list-e1890cf8",
                        ))
                    } else if state.error.is_some() {
                        None
                    } else {
                        Some(crate::i18n::text(
                            "repo-workflows-lfs_locks-lock-list-5619b99f",
                        ))
                    }
                } else if panel.only_mine && panel.filter.trim().is_empty() {
                    Some(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-list-1a9f2707",
                    ))
                } else {
                    Some(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-list-3ef08b56",
                    ))
                };
                if let Some(note) = note {
                    ui.label(RichText::new(note).color(t.text_dim));
                }
            }
            for lock in shown {
                ui.horizontal(|ui| {
                    let mut on = panel.chosen.contains(&lock.id);
                    let path = RichText::new(&lock.path).font(theme::medium(12.5));
                    if ui.checkbox(&mut on, path).changed() {
                        if on {
                            panel.chosen.insert(lock.id.clone());
                        } else {
                            panel.chosen.remove(&lock.id);
                        }
                    }
                    icons::inline(ui, "lock", 13.0, lock_color(t, lock));
                    ui.label(
                        RichText::new(lock_label(lock, now))
                            .color(t.text_dim)
                            .small(),
                    );
                });
            }
        });
}

/// Unlock for the chosen locks that are ours (or whose owner isn't known), and Force unlock,
/// asked twice, for those not known to be ours; the confirmation holds for the locks it named.
fn unlock_buttons(
    ui: &mut Ui,
    t: &Tokens,
    state: &LockState,
    shown: &[&LfsLock],
    panel: &mut Panel,
    acts: &mut Vec<Action>,
) {
    let chosen: Vec<&LfsLock> = shown
        .iter()
        .copied()
        .filter(|l| panel.chosen.contains(&l.id))
        .collect();
    let pair = |l: &&LfsLock| (l.id.clone(), l.path.clone());
    let mine: Vec<(String, String)> = chosen.iter().filter(|l| !l.theirs()).map(pair).collect();
    let forceable: Vec<(String, String)> = chosen
        .iter()
        .filter(|l| l.ours != Some(true))
        .map(pair)
        .collect();
    let forceable_ids: BTreeSet<String> = forceable.iter().map(|(id, _)| id.clone()).collect();
    if panel
        .confirm_force
        .as_ref()
        .is_some_and(|c| *c != forceable_ids)
    {
        panel.confirm_force = None;
    }
    let any_forceable = shown.iter().any(|l| l.ours != Some(true));
    let idle = state.busy.is_none();
    ui.horizontal(|ui| {
        let all = !shown.is_empty() && chosen.len() == shown.len();
        let pick = if all {
            crate::i18n::text("repo-workflows-lfs_locks-unlock-buttons-e2d5673c")
        } else {
            crate::i18n::text("repo-workflows-lfs_locks-unlock-buttons-ccf3d9ea")
        };
        if ui
            .add_enabled(!shown.is_empty(), egui::Button::new(pick))
            .clicked()
        {
            if all {
                panel.chosen.clear();
            } else {
                panel.chosen.extend(shown.iter().map(|l| l.id.clone()));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if any_forceable {
                let n = forceable.len();
                let label = match (&panel.confirm_force, forceable.first()) {
                    (Some(_), Some((id, _))) if n == 1 => {
                        let owner = chosen
                            .iter()
                            .find(|l| &l.id == id)
                            .map(|l| l.owner.as_str())
                            .filter(|o| !o.is_empty())
                            .unwrap_or(crate::i18n::text(
                                "repo-workflows-lfs_locks-unlock-buttons-97edf7d6",
                            ));
                        crate::i18n::message(
                            "repo-workflows-lfs_locks-unlock-buttons-b57b0be8",
                            &[("owner", (owner).clone().into())],
                        )
                    }
                    (Some(_), _) => crate::i18n::message(
                        "repo-workflows-lfs_locks-unlock-buttons-9971cec2",
                        &[("n", (n).clone().into())],
                    ),
                    (None, _) if n == 0 => {
                        crate::i18n::text("repo-workflows-lfs_locks-unlock-buttons-f16646da")
                            .to_owned()
                    }
                    (None, _) => crate::i18n::message(
                        "repo-workflows-lfs_locks-unlock-buttons-6021bccb",
                        &[("n", (n).clone().into())],
                    ),
                };
                let force = ui
                    .add_enabled(idle && n > 0, egui::Button::new(label))
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-lfs_locks-unlock-buttons-c214395e",
                    ));
                if force.clicked() {
                    if panel.confirm_force.is_some() {
                        acts.push(Action::LfsLocks(LockAct::Unlock {
                            locks: forceable.clone(),
                            force: true,
                        }));
                    } else {
                        panel.confirm_force = Some(forceable_ids.clone());
                    }
                }
            }
            let label = match mine.len() {
                0 => {
                    crate::i18n::text("repo-workflows-lfs_locks-unlock-buttons-4ac709aa").to_owned()
                }
                n => crate::i18n::message(
                    "repo-workflows-lfs_locks-unlock-buttons-24110cc4",
                    &[("n", (n).clone().into())],
                ),
            };
            if ui
                .add_enabled(idle && !mine.is_empty(), egui::Button::new(label))
                .clicked()
            {
                acts.push(Action::LfsLocks(LockAct::Unlock {
                    locks: mine.clone(),
                    force: false,
                }));
            }
        });
    });
    if panel.confirm_force.is_some() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-lfs-force-unlock-confirmation",
                    &[("count", (forceable.len()).into())],
                ))
                .color(t.warning)
                .small(),
            );
            if ui
                .small_button(crate::i18n::text(
                    "repo-workflows-lfs_locks-unlock-buttons-19766ed6",
                ))
                .clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                panel.confirm_force = None;
            }
        });
    }
}

/// The lockable patterns of `.gitattributes`, and a field to add one.
fn patterns(ui: &mut Ui, t: &Tokens, state: &LockState, panel: &mut Panel, acts: &mut Vec<Action>) {
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-lfs_locks-patterns-abf43528",
        ))
        .font(theme::semibold(13.0)),
    );
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-lfs_locks-patterns-ddfb1d6f",
        ))
        .color(t.text_dim)
        .small(),
    );
    if state.patterns.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-lfs_locks-patterns-ed7c75cc",
            ))
            .color(t.text_faint),
        );
    } else {
        ui.horizontal_wrapped(|ui| {
            for p in &state.patterns {
                crate::repo::widgets::chip(ui, p, t.text_dim);
            }
        });
    }
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut panel.new_pattern)
                .hint_text("*.psd")
                .desired_width(200.0),
        );
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = !panel.new_pattern.trim().is_empty() && state.busy.is_none();
        let add = ui
            .add_enabled(
                ok,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-lfs_locks-patterns-6105d5e3",
                )),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-lfs_locks-patterns-119fe07d",
            ));
        if ok && (add.clicked() || enter) {
            acts.push(Action::LfsLocks(LockAct::TrackLockable(
                panel.new_pattern.trim().to_owned(),
            )));
        }
    });
}
