//! The LFS locks panel: every lock on the server, filtered by path or owner, unlocked several at
//! once (someone else's by force, for admins), and the patterns that make files lockable.

use egui::{RichText, Ui};

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
                let shown: Vec<LfsLock> = state.shown(&panel).into_iter().cloned().collect();
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

/// Title, refresh and close; true when closed.
fn header(ui: &mut Ui, t: &Tokens, state: &LockState, acts: &mut Vec<Action>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        icons::inline(ui, "lock", 16.0, t.icon);
        ui.label(RichText::new("LFS locks").font(theme::semibold(16.0)));
        if state.reading.is_some() {
            ui.add(crate::spinner::Spinner::new());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                close = true;
            }
            let reading = state.reading.is_some();
            if !reading && icons::small_button(ui, "refresh-cw", "Read the locks again").clicked() {
                acts.push(Action::LfsLocks(LockAct::Refresh));
            }
        });
    });
    if let Some(e) = &state.error {
        ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    }
    if let Some(busy) = &state.busy {
        ui.label(RichText::new(format!("{busy}…")).color(t.text_dim));
    }
    close
}

fn filters(ui: &mut Ui, panel: &mut Panel) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panel.filter)
                .hint_text("Filter by path or owner")
                .desired_width(320.0),
        );
        ui.checkbox(&mut panel.only_mine, "Only mine");
    });
}

fn lock_list(ui: &mut Ui, t: &Tokens, state: &LockState, shown: &[LfsLock], panel: &mut Panel) {
    let now = git::now();
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown.is_empty() {
                let note = if state.locks.is_empty() {
                    if state.reading.is_some() {
                        "Reading the locks…"
                    } else {
                        "Nobody holds a lock."
                    }
                } else {
                    "No lock matches the filter."
                };
                ui.label(RichText::new(note).color(t.text_dim));
            }
            for lock in shown {
                ui.horizontal(|ui| {
                    let mut on = panel.chosen.contains(&lock.id);
                    if ui.checkbox(&mut on, "").changed() {
                        if on {
                            panel.chosen.insert(lock.id.clone());
                        } else {
                            panel.chosen.remove(&lock.id);
                        }
                        panel.confirm_force = false;
                    }
                    let color = match lock.ours {
                        Some(true) => t.accent,
                        Some(false) => t.warning,
                        None => t.text_dim,
                    };
                    icons::inline(ui, "lock", 13.0, color);
                    ui.add(
                        egui::Label::new(RichText::new(&lock.path).font(theme::medium(12.5)))
                            .truncate(),
                    );
                    ui.label(
                        RichText::new(lock_label(lock, now))
                            .color(t.text_dim)
                            .small(),
                    );
                });
            }
        });
}

/// Unlock for the chosen locks that are ours (or not known to be someone else's), and Force
/// unlock, asked twice, for those of someone else.
fn unlock_buttons(
    ui: &mut Ui,
    t: &Tokens,
    state: &LockState,
    shown: &[LfsLock],
    panel: &mut Panel,
    acts: &mut Vec<Action>,
) {
    let chosen: Vec<&LfsLock> = shown
        .iter()
        .filter(|l| panel.chosen.contains(&l.id))
        .collect();
    let pair = |l: &&LfsLock| (l.id.clone(), l.path.clone());
    let mine: Vec<(String, String)> = chosen.iter().filter(|l| !l.theirs()).map(pair).collect();
    let theirs: Vec<(String, String)> = chosen.iter().filter(|l| l.theirs()).map(pair).collect();
    let idle = state.busy.is_none();
    ui.horizontal(|ui| {
        let all = !shown.is_empty() && chosen.len() == shown.len();
        if ui
            .add_enabled(
                !shown.is_empty(),
                egui::Button::new(if all { "Pick none" } else { "Pick all" }),
            )
            .clicked()
        {
            panel.confirm_force = false;
            if all {
                panel.chosen.clear();
            } else {
                panel.chosen.extend(shown.iter().map(|l| l.id.clone()));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let force_label = if panel.confirm_force {
                format!("Take {} locks from their owners", theirs.len())
            } else {
                format!("Force unlock {}…", theirs.len())
            };
            let force = ui
                .add_enabled(idle && !theirs.is_empty(), egui::Button::new(force_label))
                .on_hover_text(
                    "Takes the lock from whoever holds it. The server allows this to admins \
                     only, and their unpushed work on the file may then be refused.",
                );
            if force.clicked() {
                if panel.confirm_force {
                    acts.push(Action::LfsLocks(LockAct::Unlock {
                        locks: theirs.clone(),
                        force: true,
                    }));
                } else {
                    panel.confirm_force = true;
                }
            }
            let unlock = ui.add_enabled(
                idle && !mine.is_empty(),
                egui::Button::new(format!("Unlock {}", mine.len())),
            );
            if unlock.clicked() {
                acts.push(Action::LfsLocks(LockAct::Unlock {
                    locks: mine.clone(),
                    force: false,
                }));
            }
        });
    });
    if panel.confirm_force {
        ui.label(
            RichText::new("Click again to force: the owners lose their locks.")
                .color(t.warning)
                .small(),
        );
    }
}

/// The lockable patterns of `.gitattributes`, and a field to add one.
fn patterns(ui: &mut Ui, t: &Tokens, state: &LockState, panel: &mut Panel, acts: &mut Vec<Action>) {
    ui.label(RichText::new("Lockable files").font(theme::semibold(13.0)));
    ui.label(
        RichText::new(
            "Files matching these patterns are kept in LFS and read-only until you lock them.",
        )
        .color(t.text_dim)
        .small(),
    );
    if state.patterns.is_empty() {
        ui.label(RichText::new("No pattern in .gitattributes is lockable.").color(t.text_faint));
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
            .add_enabled(ok, egui::Button::new("Make lockable"))
            .on_hover_text("git lfs track --lockable: writes the pattern to .gitattributes");
        if ok && (add.clicked() || enter) {
            acts.push(Action::LfsLocks(LockAct::TrackLockable(
                panel.new_pattern.trim().to_owned(),
            )));
        }
    });
}
