//! Where bisecting is offered outside its banner: a commit's menu, the command palette, and the
//! colours its marks show in on the graph.

use egui::{Color32, Ui};

use super::BisectAct;
use crate::git::Mark;
use crate::repo::{Action, Dialog, RepoTab, short};
use crate::theme::Tokens;

/// The colour a commit marked so shows in: the theme's own for success, danger and faint text.
pub(in crate::repo) fn mark_color(t: &Tokens, mark: Mark) -> Color32 {
    match mark {
        Mark::Good => t.success,
        Mark::Bad => t.danger,
        Mark::Skip => t.text_faint,
    }
}

impl RepoTab {
    /// A commit's bisect entries: start one there, or mark it in the one going on.
    pub(in crate::repo) fn bisect_menu_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(bisect) = self.bisect.state() else {
            let blocker = self.bisect_blocker();
            let resp = crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-bisect-bisect-menu-items-43fa49b5",
            ))
            .icon("command")
            .refused(blocker.as_deref())
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-bisect-bisect-menu-items-5cc56687",
            ));
            let resp = match &blocker {
                Some(why) => resp.on_disabled_hover_text(why.as_str()),
                None => resp,
            };
            if resp.clicked() {
                acts.push(Action::Dialog(self.start_bisect_dialog(hash)));
            }
            return;
        };
        let at = short(hash);
        let running = self.bisect.running();
        for (mark, label) in [
            (
                Mark::Good,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-menu-items-5f1847b9",
                    &[
                        ("at", (at).clone().into()),
                        (
                            "bisect-word-mark-good-1",
                            (bisect.word(Mark::Good)).clone().into(),
                        ),
                    ],
                ),
            ),
            (
                Mark::Bad,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-menu-items-5f1847b9",
                    &[
                        ("at", (at).clone().into()),
                        (
                            "bisect-word-mark-bad-1",
                            (bisect.word(Mark::Bad)).clone().into(),
                        ),
                    ],
                ),
            ),
            (
                Mark::Skip,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-menu-items-72281c73",
                    &[("at", (at).clone().into())],
                ),
            ),
        ] {
            let wait = crate::i18n::text(if running {
                super::banner::RUNNING
            } else {
                super::banner::BUSY
            });
            if crate::menu_item::MenuItem::new(label)
                .icon("command")
                .refused((!idle || running).then_some(wait))
                .show(ui)
                .on_disabled_hover_text(wait)
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Mark {
                    mark,
                    rev: Some(hash.to_owned()),
                }));
            }
        }
    }

    /// The dialog that starts a bisect from the commit `hash`: it is good and HEAD bad, unless
    /// it is HEAD, which is then the bad one.
    pub(in crate::repo) fn start_bisect_dialog(&self, hash: &str) -> Dialog {
        Dialog::start_bisect(hash, hash != self.refs.head)
    }

    /// What the command palette offers for bisecting.
    pub(in crate::repo) fn bisect_palette_items(&self) -> Vec<(String, String, Action)> {
        let item = |label: String, action| (label, "bisect".to_owned(), action);
        let Some(bisect) = self.bisect.state() else {
            if self.refs.head.is_empty() {
                return Vec::new();
            }
            return vec![item(
                crate::i18n::text("repo-workflows-bisect-bisect-palette-items-ba89bd47").to_owned(),
                Action::Dialog(Dialog::start_bisect("HEAD", false)),
            )];
        };
        let mark = |mark| Action::Bisect(BisectAct::Mark { mark, rev: None });
        let mut items = Vec::new();
        // Nothing that would do nothing: no marks while a run tests the commits.
        if self.bisect.running() {
            return items;
        }
        if bisect.outcome.is_none() {
            let at = short(&bisect.current);
            items.extend([
                item(
                    crate::i18n::message(
                        "repo-workflows-bisect-bisect-palette-items-417fbb2c",
                        &[
                            ("at", (at).clone().into()),
                            (
                                "bisect-word-mark-good-1",
                                (bisect.word(Mark::Good)).clone().into(),
                            ),
                        ],
                    ),
                    mark(Mark::Good),
                ),
                item(
                    crate::i18n::message(
                        "repo-workflows-bisect-bisect-palette-items-417fbb2c",
                        &[
                            ("at", (at).clone().into()),
                            (
                                "bisect-word-mark-bad-1",
                                (bisect.word(Mark::Bad)).clone().into(),
                            ),
                        ],
                    ),
                    mark(Mark::Bad),
                ),
                item(
                    crate::i18n::message(
                        "repo-workflows-bisect-bisect-palette-items-011b904c",
                        &[("at", (at).clone().into())],
                    ),
                    mark(Mark::Skip),
                ),
            ]);
            if bisect.progress.is_some() {
                items.push(item(
                    crate::i18n::text("repo-workflows-bisect-bisect-palette-items-96774cb9")
                        .to_owned(),
                    self.bisect_run_dialog(),
                ));
            }
            items.push(item(
                crate::i18n::text("repo-workflows-bisect-bisect-palette-items-23b8f037").to_owned(),
                self.stop_bisect_action(),
            ));
        } else {
            items.push(item(
                crate::i18n::text("repo-workflows-bisect-bisect-palette-items-34a86ddf").to_owned(),
                self.stop_bisect_action(),
            ));
        }
        items
    }
}
