//! Where locks show outside their panel: a padlock on a file's row, Lock and Unlock in its menu,
//! and above the changes the files someone else holds and the offer after a push.

use egui::{CornerRadius, Rect, Response, RichText, Ui, pos2, vec2};

use super::{LockAct, lock_label};
use crate::git::{self, LfsLock};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// The colour a lock shows in: ours accent, someone else's a warning, unknown dim.
fn lock_color(t: &Tokens, lock: &LfsLock) -> egui::Color32 {
    match lock.ours {
        Some(true) => t.accent,
        Some(false) => t.warning,
        None => t.text_dim,
    }
}

/// Who holds a lock, as short as a row has room for: "you", "Ana".
fn short_owner(lock: &LfsLock) -> &str {
    match lock.ours {
        Some(true) => "you",
        _ if lock.owner.is_empty() => "locked",
        _ => &lock.owner,
    }
}

impl RepoTab {
    /// The padlock on the row of `path` (drawn by `file_row`), when someone holds its lock;
    /// hidden while the row shows its buttons. The row's tooltip says who and since when.
    pub(in crate::repo) fn lock_badge(&self, ui: &Ui, row: &Response, path: &str, buttons: bool) {
        let Some(lock) = self.lfs_locks.lock_on(path) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let color = lock_color(&t, lock);
        let rect = row.rect;
        if !(buttons && (row.hovered() || row.has_focus())) {
            let font = theme::medium(10.5);
            let galley = ui
                .painter()
                .layout_no_wrap(short_owner(lock).to_owned(), font, color);
            let w = galley.size().x + 26.0;
            let pill = Rect::from_center_size(
                pos2(rect.right() - 8.0 - w / 2.0, rect.center().y),
                vec2(w, 16.0),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(pill.expand(2.0), CornerRadius::same(4), t.card);
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
        let label = lock_label(lock, git::now());
        row.clone().on_hover_text(label);
    }

    /// Lock / Unlock items for `path` in its menu, when the repository keeps files in LFS.
    pub(in crate::repo) fn lock_menu_items(&self, ui: &mut Ui, path: &str, acts: &mut Vec<Action>) {
        if !self.lfs {
            return;
        }
        let busy = self.lfs_locks.busy.is_some();
        ui.separator();
        match self.lfs_locks.lock_on(path) {
            Some(lock) if lock.ours != Some(false) => {
                let r = ui.add_enabled(!busy, egui::Button::new("Unlock"));
                if r.on_hover_text("Give the LFS lock back so others can edit the file")
                    .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::Unlock {
                        locks: vec![(lock.id.clone(), lock.path.clone())],
                        force: false,
                    }));
                }
            }
            Some(lock) => {
                ui.add_enabled(false, egui::Button::new(lock_label(lock, git::now())));
                if ui.button("LFS locks…").clicked() {
                    acts.push(Action::LfsLocks(LockAct::OpenPanel));
                }
            }
            None => {
                let r = ui.add_enabled(!busy, egui::Button::new("Lock"));
                if r.on_hover_text("Take the file's LFS lock: nobody else can push changes to it")
                    .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::Lock(path.to_owned())));
                }
            }
        }
    }

    /// Above the changes: the changed files someone else holds the lock of (the push would be
    /// refused), changed lockable files nobody holds, and after a push, the offer to unlock.
    pub(in crate::repo) fn lock_banners(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if !self.lfs {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let changed = self.changed_paths();
        let mut theirs: Vec<&LfsLock> = self
            .lfs_locks
            .locks
            .iter()
            .filter(|l| l.theirs() && changed.contains(&l.path))
            .collect();
        theirs.sort_by(|a, b| a.path.cmp(&b.path));
        if !theirs.is_empty() {
            let names: Vec<String> = theirs
                .iter()
                .map(|l| format!("{} ({})", l.path, short_owner(l)))
                .collect();
            let text = format!(
                "Locked by someone else: {}. Pushing changes to {} will be refused until \
                 the lock is given back.",
                list(&names),
                if theirs.len() == 1 { "it" } else { "them" }
            );
            banner(ui, &t, t.danger, &text, None);
        }
        let mut unlocked: Vec<&String> = changed
            .iter()
            .filter(|p| self.lfs_locks.is_lockable(p) && self.lfs_locks.lock_on(p).is_none())
            .collect();
        unlocked.sort();
        if let [one] = unlocked.as_slice() {
            let text = format!(
                "{one} is lockable, but nobody holds its lock: lock it so that nobody else \
                 changes it meanwhile."
            );
            let busy = self.lfs_locks.busy.is_some();
            let path = (*one).clone();
            let lock = Box::new(|ui: &mut Ui| {
                if ui.add_enabled(!busy, egui::Button::new("Lock")).clicked() {
                    acts.push(Action::LfsLocks(LockAct::Lock(path)));
                }
            });
            banner(ui, &t, t.warning, &text, Some(lock));
        } else if !unlocked.is_empty() {
            let text = format!(
                "{} changed lockable files have no lock: lock them from their menu so that \
                 nobody else changes them meanwhile.",
                unlocked.len()
            );
            banner(ui, &t, t.warning, &text, None);
        }
        let offer = &self.lfs_locks.offer;
        if !offer.is_empty() {
            let text = match offer.as_slice() {
                [one] => format!("Pushed. You still hold the lock on {}.", one.path),
                many => format!(
                    "Pushed. You still hold {} locks on files with no changes left.",
                    many.len()
                ),
            };
            let busy = self.lfs_locks.busy.is_some();
            let buttons = Box::new(|ui: &mut Ui| {
                if ui.button("Keep").clicked() {
                    acts.push(Action::LfsLocks(LockAct::DismissOffer));
                }
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

type Buttons<'a> = Option<Box<dyn FnOnce(&mut Ui) + 'a>>;

#[cfg(test)]
mod tests {
    use super::list;

    #[test]
    fn names_listed_short() {
        let n = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(list(&n(&[])), "");
        assert_eq!(list(&n(&["a"])), "a");
        assert_eq!(list(&n(&["a", "b"])), "a and b");
        assert_eq!(list(&n(&["a", "b", "c", "d"])), "a, b and 2 more");
    }
}
