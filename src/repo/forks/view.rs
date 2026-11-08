//! What the forks show: badges in the list of pull requests, and the items and buttons that act
//! on a pull request from a fork.

use egui::{Align2, Painter, Pos2, Ui};

use super::ForkAct;
use super::plan::{BranchSetup, fork_remote};
use crate::forge::{Forge, ForkInfo};
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// The badges of pull request `number`, the most useful first: (text, why, warn).
pub(crate) fn badges(tab: &RepoTab, number: u64, sha: &str) -> Vec<(&'static str, String, bool)> {
    let Some(info) = tab.forks.of(number) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let waiting = tab.forks.waiting(sha).len();
    if waiting > 0 {
        out.push((
            crate::i18n::text("repo-workflows-forks-badges-84e3eb92"),
            crate::i18n::message(
                "repo-workflows-fork-waiting-runs",
                &[("count", (waiting).into())],
            ),
            true,
        ));
    }
    if info.cross && info.maintainers_can_edit && !info.repo_path.is_empty() {
        out.push((
            crate::i18n::text("repo-workflows-forks-badges-42d0a9e0"),
            crate::i18n::message(
                "repo-workflows-forks-badges-aea3a983",
                &[
                    ("info-owner-1", (info.owner).clone().into()),
                    ("info-branch-2", (info.branch).clone().into()),
                    ("info-owner-3", (info.owner).clone().into()),
                ],
            ),
            false,
        ));
    }
    if info.first_time {
        out.push((
            "first-time",
            crate::i18n::message(
                "repo-workflows-forks-badges-e3d8fbdd",
                &[("info-owner-1", (info.owner).clone().into())],
            ),
            false,
        ));
    }
    out
}

/// Paints the badges of pull request `number` from `at` (left, centre); the text that says why,
/// for the row's tooltip.
pub(crate) fn paint_badges(
    tab: &RepoTab,
    painter: &Painter,
    at: Pos2,
    number: u64,
    sha: &str,
) -> Option<String> {
    let t = Tokens::get(painter.ctx());
    let badges = badges(tab, number, sha);
    let mut x = at.x;
    for (text, _, warn) in &badges {
        let r = painter.text(
            egui::pos2(x, at.y),
            Align2::LEFT_CENTER,
            *text,
            egui::FontId::proportional(10.5),
            if *warn { t.warning } else { t.accent },
        );
        x = r.right() + 8.0;
    }
    (!badges.is_empty()).then(|| {
        badges
            .iter()
            .map(|(_, why, _)| why.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// What the forge calls letting maintainers push to a pull request's branch.
fn allow_edits(forge: Option<Forge>) -> &'static str {
    match forge {
        Some(Forge::GitHub) => crate::i18n::text("repo-workflows-forks-allow-edits-6bb2bbb0"),
        Some(Forge::GitLab) => crate::i18n::text("repo-workflows-forks-allow-edits-9e60d9b6"),
        _ => crate::i18n::text("repo-workflows-forks-allow-edits-ddc30ba0"),
    }
}

impl RepoTab {
    /// The local branch set up to push to `info`'s fork (found by its URL), if any.
    fn pushing_to_fork(&self, info: &ForkInfo) -> Option<&BranchSetup> {
        let remote = fork_remote(info, &self.remotes, self.hosting.as_ref())?;
        self.forks.setup.pushing_to(&remote, &info.branch)
    }

    /// Whether pull request `number` has anything for [`Self::fork_items`] to show.
    pub(in crate::repo) fn has_fork_items(&self, number: u64) -> bool {
        self.forks
            .of(number)
            .is_some_and(|i| i.cross || !self.forks.waiting(&i.sha).is_empty())
    }

    /// The items of a pull request's menu (or, `buttons`, the buttons of its page) that act on
    /// a pull request from a fork.
    pub(in crate::repo) fn fork_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        number: u64,
        idle: bool,
        buttons: bool,
    ) {
        let Some(info) = self.forks.of(number) else {
            return;
        };
        let item = |ui: &mut Ui, enabled: bool, text: &str, tip: &str| {
            let button = if buttons {
                egui::Button::new(text).min_size(egui::vec2(0.0, 26.0))
            } else {
                egui::Button::new(text)
            };
            ui.add_enabled(enabled, button)
                .on_hover_text(tip)
                .on_disabled_hover_text(tip)
                .clicked()
        };
        if info.cross && info.repo_path.is_empty() {
            item(
                ui,
                false,
                crate::i18n::text("repo-workflows-forks-fork-items-bdee4479"),
                crate::i18n::text("repo-workflows-forks-fork-items-9862a39c"),
            );
        } else if info.cross {
            let set_up = self.pushing_to_fork(info);
            let refused = crate::i18n::message(
                "repo-workflows-forks-fork-items-6c1751f8",
                &[
                    ("info-owner-1", (info.owner).clone().into()),
                    ("info-branch-2", (info.branch).clone().into()),
                    (
                        "allow-edits-self-forge-3",
                        (allow_edits(self.forge())).into(),
                    ),
                ],
            );
            let (text, tip) = if !info.maintainers_can_edit {
                (
                    crate::i18n::message(
                        "repo-workflows-forks-fork-items-c8fd7141",
                        &[("info-owner-1", (info.owner).clone().into())],
                    ),
                    refused.clone(),
                )
            } else if let Some(b) = set_up {
                (
                    crate::i18n::message(
                        "repo-workflows-forks-fork-items-9e40421a",
                        &[
                            ("b-name-1", (b.name).clone().into()),
                            ("info-owner-2", (info.owner).clone().into()),
                        ],
                    ),
                    crate::i18n::message(
                        "repo-workflows-forks-fork-items-7e44168f",
                        &[
                            ("info-owner-1", (info.owner).clone().into()),
                            ("info-branch-2", (info.branch).clone().into()),
                        ],
                    ),
                )
            } else {
                (
                    crate::i18n::message(
                        "repo-workflows-forks-fork-items-c8fd7141",
                        &[("info-owner-1", (info.owner).clone().into())],
                    ),
                    crate::i18n::message(
                        "repo-workflows-forks-fork-items-a33fed38",
                        &[
                            ("info-owner-1", (info.owner).clone().into()),
                            ("info-branch-2", (info.branch).clone().into()),
                        ],
                    ),
                )
            };
            if item(ui, idle && info.maintainers_can_edit, &text, &tip) {
                acts.push(Action::Fork(ForkAct::Checkout(
                    number,
                    self.forge_generation,
                )));
            }
            let here = set_up.is_some_and(|b| self.refs.current.as_deref() == Some(&b.name));
            let tip = if !info.maintainers_can_edit {
                refused
            } else if here {
                crate::i18n::message(
                    "repo-workflows-forks-fork-items-a0b3b5ed",
                    &[
                        ("info-base-1", (info.base).clone().into()),
                        ("info-owner-2", (info.owner).clone().into()),
                        ("info-base-3", (info.base).clone().into()),
                    ],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-forks-fork-items-2b48edeb",
                    &[("info-owner-1", (info.owner).clone().into())],
                )
            };
            let text = crate::i18n::message(
                "repo-workflows-forks-fork-items-3450c709",
                &[
                    ("info-base-1", (info.base).clone().into()),
                    ("info-owner-2", (info.owner).clone().into()),
                ],
            );
            if item(ui, idle && here && info.maintainers_can_edit, &text, &tip) {
                acts.push(Action::Fork(ForkAct::Rebase(number, self.forge_generation)));
            }
        }
        let waiting = self.forks.waiting(&info.sha).len();
        if waiting > 0 {
            let tip = crate::i18n::text("repo-workflows-forks-fork-items-afc89aab");
            if item(
                ui,
                idle,
                &crate::i18n::message(
                    "repo-workflows-forks-fork-items-99b20173",
                    &[("waiting", (waiting).into())],
                ),
                tip,
            ) {
                acts.push(Action::Fork(ForkAct::Approve(
                    number,
                    self.forge_generation,
                )));
            }
        }
    }
}
