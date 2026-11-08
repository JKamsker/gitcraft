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
            let galley =
                ui.painter()
                    .layout_no_wrap(display_short_owner(lock), theme::medium(10.5), color);
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
                    .refused(Some(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-menu-items-bafa9d65",
                    )))
                    .show(ui);
                if crate::menu_item::MenuItem::new(crate::i18n::text(
                    "repo-workflows-lfs_locks-lock-menu-items-03d23d29",
                ))
                .icon("command")
                .show(ui)
                .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::OpenPanel));
                }
            }
            Some(lock) => {
                let tip = if changed {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-menu-items-6526301c")
                } else if lock.ours.is_none() {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-menu-items-0c28d273")
                } else {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-menu-items-052a679c")
                };
                let r = crate::menu_item::MenuItem::new(crate::i18n::text(
                    "repo-workflows-lfs_locks-lock-menu-items-4ac709aa",
                ))
                .icon("lock")
                .refused(if busy {
                    Some(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-menu-items-b8fc75d5",
                    ))
                } else {
                    changed.then_some(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-menu-items-bea7de48",
                    ))
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
                let r = crate::menu_item::MenuItem::new(crate::i18n::text(
                    "repo-workflows-lfs_locks-lock-menu-items-db44b8db",
                ))
                .icon("lock")
                .refused(busy.then_some(crate::i18n::text(
                    "repo-workflows-lfs_locks-lock-menu-items-b8fc75d5",
                )))
                .show(ui);
                if r.on_hover_text(crate::i18n::text(
                    "repo-workflows-lfs_locks-lock-menu-items-3f09bd22",
                ))
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
                let text = crate::i18n::text("repo-workflows-lfs_locks-lock-banners-f8368582");
                let retry = Box::new(|ui: &mut Ui| {
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-lfs_locks-lock-banners-942087cc",
                        ))
                        .clicked()
                    {
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
                [one] => crate::i18n::message(
                    "repo-workflows-lfs_locks-lock-banners-11ae36a8",
                    &[
                        (
                            "if-one-owner-is-empty-someone-el-1",
                            (if one.owner.is_empty() {
                                crate::i18n::text("repo-workflows-lfs_locks-lock-banners-3233cb04")
                            } else {
                                &one.owner
                            })
                            .clone()
                            .into(),
                        ),
                        ("one-path-2", (one.path).clone().into()),
                    ],
                ),
                many => {
                    let names: Vec<String> = many
                        .iter()
                        .map(|l| format!("{} ({})", l.path, display_short_owner(l)))
                        .collect();
                    crate::i18n::message(
                        "repo-workflows-lfs_locks-lock-banners-2737026b",
                        &[("list-names-1", (list(&names)).clone().into())],
                    )
                }
            };
            let open = Box::new(|ui: &mut Ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-lfs_locks-lock-banners-03d23d29",
                    ))
                    .clicked()
                {
                    acts.push(Action::LfsLocks(LockAct::OpenPanel));
                }
            });
            banner(ui, &t, t.danger, &text, Some(open));
        }
        if !d.unlocked.is_empty() {
            let text = match d.unlocked.as_slice() {
                [one] => crate::i18n::message(
                    "repo-workflows-lfs_locks-lock-banners-82b7be09",
                    &[("one", (one).clone().into())],
                ),
                many => crate::i18n::message(
                    "repo-workflows-lfs_locks-lock-banners-96ed4625",
                    &[("many-len-1", (many.len()).clone().into())],
                ),
            };
            let paths = d.unlocked.clone();
            let lock = Box::new(|ui: &mut Ui| {
                let label = if paths.len() == 1 {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-db44b8db")
                } else {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-ac5bb7e1")
                };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    acts.push(Action::LfsLocks(LockAct::Lock(paths)));
                }
            });
            banner(ui, &t, t.warning, &text, Some(lock));
        }
        let offer = &state.offer;
        if !offer.is_empty() {
            let text = match offer.as_slice() {
                [one] => crate::i18n::message(
                    "repo-workflows-lfs_locks-lock-banners-4fa40890",
                    &[("one-path-1", (one.path).clone().into())],
                ),
                many => crate::i18n::message(
                    "repo-workflows-lfs_locks-lock-banners-4caac7c4",
                    &[("many-len-1", (many.len()).clone().into())],
                ),
            };
            let buttons = Box::new(|ui: &mut Ui| {
                let label = if offer.len() == 1 {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-4ac709aa")
                } else {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-379e383e")
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
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-bb83044b")
                } else {
                    crate::i18n::text("repo-workflows-lfs_locks-lock-banners-6df5e70f")
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
        [a, b] => crate::i18n::message(
            "repo-workflows-lfs_locks-list-04f767bd",
            &[("a", (a).clone().into()), ("b", (b).clone().into())],
        ),
        [a, b, rest @ ..] => crate::i18n::message(
            "repo-workflows-lfs_locks-list-e6b59dc4",
            &[
                ("a", (a).clone().into()),
                ("b", (b).clone().into()),
                ("rest-len-1", (rest.len()).clone().into()),
            ],
        ),
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

fn display_short_owner(lock: &LfsLock) -> String {
    match lock.ours {
        Some(true) => crate::i18n::text("repo-workflows-r1-you").into(),
        _ if lock.owner.is_empty() => crate::i18n::text("repo-workflows-r1-locked").into(),
        _ => short_owner(lock),
    }
}

#[cfg(test)]
mod fluent_r1_owner_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_short_lock_owner_keeps_provider_words_opaque() {
        let ours = LfsLock {
            id: "raw-id".into(),
            path: "raw/path".into(),
            owner: "you".into(),
            locked_at: None,
            ours: Some(true),
        };
        let named = LfsLock {
            ours: Some(false),
            ..ours.clone()
        };
        let unknown = LfsLock {
            owner: String::new(),
            ours: None,
            ..ours.clone()
        };
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            assert_eq!(short_owner(&ours), "you");
            assert_eq!(
                display_short_owner(&ours),
                if lang == "de" { "Sie" } else { "you" }
            );
            assert_eq!(display_short_owner(&named), "you");
            assert_eq!(
                display_short_owner(&unknown),
                if lang == "de" { "gesperrt" } else { "locked" }
            );
        }
        crate::i18n::set_language("en-US");
    }
}
