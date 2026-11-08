//! Where locks show outside their panel: a padlock on a file's row, Lock and Unlock in its menu,
//! and above the changes the files someone else holds and the offer after a push.

use egui::{CornerRadius, Rect, Response, RichText, Ui, pos2, vec2};

use super::{LockAct, lock_label};
use crate::git::{self, LfsLock};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// The most characters of an owner's name a row shows.
const OWNER_CHARS: usize = 12;

/// The colour a lock shows in: ours accent, someone else's a warning, unknown dim.
pub(super) fn lock_color(t: &Tokens, lock: &LfsLock) -> egui::Color32 {
    match lock.ours {
        Some(true) => t.accent,
        Some(false) => t.warning,
        None => t.text_dim,
    }
}

/// Who holds a lock, as short as a row has room for: "you", "Ana", "jonathan.sm…".
pub(super) fn short_owner(lock: &LfsLock) -> String {
    match lock.ours {
        Some(true) => "you".to_owned(),
        _ if lock.owner.is_empty() => "locked".to_owned(),
        _ if lock.owner.chars().count() > OWNER_CHARS => {
            let cut: String = lock.owner.chars().take(OWNER_CHARS - 1).collect();
            format!("{cut}…")
        }
        _ => lock.owner.clone(),
    }
}

impl RepoTab {
    /// The padlock on the row of `path` (drawn by `file_row`), when someone holds its lock;
    /// hidden while the row shows its buttons (`buttons`: it has some, shown when it is hovered
    /// or `selected`). The row's tooltip says who and since when.
    pub(in crate::repo) fn lock_badge(
        &self,
        ui: &Ui,
        row: &Response,
        path: &str,
        (buttons, selected): (bool, bool),
    ) {
        let Some(lock) = self.lfs_locks.lock_on(path) else {
            return;
        };
        let rect = row.rect;
        let hovered = ui.rect_contains_pointer(rect);
        if !(buttons && (hovered || selected)) {
            let t = Tokens::get(ui.ctx());
            let color = lock_color(&t, lock);
            let galley = ui
                .painter()
                .layout_no_wrap(short_owner(lock), theme::medium(10.5), color);
            let w = galley.size().x + 26.0;
            let pill = Rect::from_center_size(
                pos2(rect.right() - 8.0 - w / 2.0, rect.center().y),
                vec2(w, 16.0),
            );
            // Over the end of the name: the row's own background first, as file_row paints it.
            let under = if selected {
                t.row_selected
            } else if hovered {
                t.hover
            } else {
                t.card
            };
            let painter = ui.painter_at(rect);
            let cover = Rect::from_min_max(pos2(pill.left() - 6.0, rect.top()), rect.max);
            painter.rect_filled(cover, 0.0, t.card);
            painter.rect_filled(cover, 0.0, under);
            painter.rect_filled(pill, CornerRadius::same(8), color.gamma_multiply(0.18));
            let icon =
                Rect::from_center_size(pos2(pill.left() + 10.0, pill.center().y), vec2(11.0, 11.0));
            icons::paint(ui, icon, "lock", 11.0, color);
            painter.galley(
                pos2(pill.left() + 18.0, pill.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
        }
        row.clone().on_hover_ui(|ui| {
            ui.label(lock_label(lock, git::now()));
        });
    }

    /// Lock / Unlock items for `path` in its menu, when the repository keeps files in LFS and
    /// the menu is of the checkout (`rev` none or HEAD), where locks apply.
    pub(in crate::repo) fn lock_menu_items(
        &self,
        ui: &mut Ui,
        path: &str,
        rev: Option<&str>,
        acts: &mut Vec<Action>,
    ) {
        let checkout = rev.is_none_or(|r| !self.refs.head.is_empty() && r == self.refs.head);
        if !self.lfs || !checkout {
            return;
        }
        let busy = self.lfs_locks.busy.is_some();
        // git-lfs won't give back the lock of a file with changes.
        let changed = self.lfs_locks.derived.changed.contains(path);
        ui.separator();
        match self.lfs_locks.lock_on(path) {
            Some(lock) if lock.theirs() => {
                crate::menu_item::MenuItem::new(lock_label(lock, git::now()))
                    .icon("lock")
                    .refused(Some("Someone else holds this file’s lock"))
                    .show(ui);
                if crate::menu_item::MenuItem::new("LFS locks…")
                    .icon("command")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::OpenPanel));
                }
            }
            Some(lock) => {
                let tip = if changed {
                    "git-lfs won't give back the lock of a file with changes: commit them first"
                } else if lock.ours.is_none() {
                    "The server can't say whose lock this is: unlocking fails if it isn't yours"
                } else {
                    "Give the LFS lock back so others can push changes to the file"
                };
                let r = crate::menu_item::MenuItem::new("Unlock")
                    .icon("lock")
                    .refused(if busy {
                        Some("An LFS lock operation is running")
                    } else {
                        changed.then_some("Commit the file’s changes before unlocking it")
                    })
                    .show(ui);
                if r.on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
                    acts.push(Action::LfsLocks(LockAct::Unlock {
                        locks: vec![(lock.id.clone(), lock.path.clone())],
                        force: false,
                    }));
                }
            }
            None => {
                let r = crate::menu_item::MenuItem::new("Lock")
                    .icon("lock")
                    .refused(busy.then_some("An LFS lock operation is running"))
                    .show(ui);
                if r.on_hover_text("Take the file's LFS lock: nobody else can push changes to it")
                    .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::Lock(vec![path.to_owned()])));
                }
            }
        }
    }

    /// Above the changes: the changed files someone else holds the lock of (the push would be
    /// refused), changed lockable files nobody holds, and after a push, the offer to unlock.
    /// Only once the locks were read: until then nothing is known of who holds what.
    pub(in crate::repo) fn lock_banners(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let state = &self.lfs_locks;
        if !self.lfs {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let d = &state.derived;
        if !state.read_ok {
            if state.error.is_some() && !d.unlocked.is_empty() && state.reading.is_none() {
                let text =
                    "Couldn't read the LFS locks, so who holds your changed files isn't known.";
                let retry = Box::new(|ui: &mut Ui| {
                    if ui.button("Retry").clicked() {
                        acts.push(Action::LfsLocks(LockAct::Refresh));
                    }
                });
                banner(ui, &t, t.text_dim, text, Some(retry));
            }
            return;
        }
        let busy = state.busy.is_some();
        if !d.theirs.is_empty() {
            let theirs: Vec<&LfsLock> = d
                .theirs
                .iter()
                .filter_map(|&i| state.locks.get(i))
                .collect();
            let text = match theirs.as_slice() {
                [one] => format!(
                    "{} holds the lock on {}: your push of it will be refused until the lock is \
                     given back.",
                    if one.owner.is_empty() {
                        "Someone else"
                    } else {
                        &one.owner
                    },
                    one.path
                ),
                many => {
                    let names: Vec<String> = many
                        .iter()
                        .map(|l| format!("{} ({})", l.path, short_owner(l)))
                        .collect();
                    format!(
                        "Locked by someone else: {}. Your push of them will be refused until the \
                         locks are given back.",
                        list(&names)
                    )
                }
            };
            let open = Box::new(|ui: &mut Ui| {
                if ui.button("LFS locks…").clicked() {
                    acts.push(Action::LfsLocks(LockAct::OpenPanel));
                }
            });
            banner(ui, &t, t.danger, &text, Some(open));
        }
        if !d.unlocked.is_empty() {
            let text = match d.unlocked.as_slice() {
                [one] => format!(
                    "{one} can be locked and nobody holds it. Lock it so nobody else pushes \
                     changes to it while you work."
                ),
                many => format!(
                    "{} changed files can be locked and nobody holds them. Lock them so nobody \
                     else pushes changes to them while you work.",
                    many.len()
                ),
            };
            let paths = d.unlocked.clone();
            let lock = Box::new(|ui: &mut Ui| {
                let label = if paths.len() == 1 { "Lock" } else { "Lock all" };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    acts.push(Action::LfsLocks(LockAct::Lock(paths)));
                }
            });
            banner(ui, &t, t.warning, &text, Some(lock));
        }
        let offer = &state.offer;
        if !offer.is_empty() {
            let text = match offer.as_slice() {
                [one] => format!(
                    "You pushed {} and still hold its lock: give it back so others can change it?",
                    one.path
                ),
                many => format!(
                    "You pushed {} files and still hold their locks: give them back so others \
                     can change them?",
                    many.len()
                ),
            };
            let buttons = Box::new(|ui: &mut Ui| {
                let label = if offer.len() == 1 {
                    "Unlock"
                } else {
                    "Unlock all"
                };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    acts.push(Action::LfsLocks(LockAct::Unlock {
                        locks: offer
                            .iter()
                            .map(|l| (l.id.clone(), l.path.clone()))
                            .collect(),
                        force: false,
                    }));
                }
                let keep = if offer.len() == 1 {
                    "Keep it locked"
                } else {
                    "Keep them locked"
                };
                if ui.button(keep).clicked() {
                    acts.push(Action::LfsLocks(LockAct::DismissOffer));
                }
            });
            banner(ui, &t, t.accent, &text, Some(buttons));
        }
    }
}

/// "a, b and 3 more".
fn list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

type Buttons<'a> = Option<Box<dyn FnOnce(&mut Ui) + 'a>>;

/// A tinted strip with a padlock and `text`, and `buttons` below it on the right.
fn banner(ui: &mut Ui, t: &Tokens, color: egui::Color32, text: &str, buttons: Buttons<'_>) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, "lock", 14.0, color);
                ui.add(egui::Label::new(RichText::new(text).color(t.text).small()).wrap());
            });
            if let Some(buttons) = buttons {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{list, short_owner};
    use crate::git::LfsLock;

    #[test]
    fn names_listed_short() {
        let n = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(list(&n(&[])), "");
        assert_eq!(list(&n(&["a"])), "a");
        assert_eq!(list(&n(&["a", "b"])), "a and b");
        assert_eq!(list(&n(&["a", "b", "c", "d"])), "a, b and 2 more");
    }

    #[test]
    fn owners_cut_to_fit_a_row() {
        let lock = |owner: &str, ours| LfsLock {
            id: "1".into(),
            path: "a".into(),
            owner: owner.into(),
            locked_at: None,
            ours,
        };
        assert_eq!(short_owner(&lock("Ana", Some(false))), "Ana");
        assert_eq!(short_owner(&lock("me", Some(true))), "you");
        assert_eq!(short_owner(&lock("", None)), "locked");
        assert_eq!(
            short_owner(&lock("jonathan.smithers@corp", None)),
            "jonathan.sm…"
        );
        assert_eq!(short_owner(&lock("ÄÖÜäöüßéèêë", None)), "ÄÖÜäöüßéèêë");
    }
}
