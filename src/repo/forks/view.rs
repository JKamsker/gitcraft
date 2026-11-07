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
            "awaiting approval",
            format!(
                "{waiting} workflow run{} of its latest commit wait for a maintainer to approve \
                 them: Approve and run in its menu",
                if waiting == 1 { "" } else { "s" }
            ),
            true,
        ));
    }
    if info.cross && info.maintainers_can_edit && !info.repo_path.is_empty() {
        out.push((
            "can edit",
            format!(
                "{} lets maintainers push to their branch {}: Check out to push to {} in its menu",
                info.owner, info.branch, info.owner
            ),
            false,
        ));
    }
    if info.first_time {
        out.push((
            "first-time",
            format!("{}'s first contribution to the repository", info.owner),
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
        Some(Forge::GitHub) => "Allow edits by maintainers",
        Some(Forge::GitLab) => "Allow commits from members who can merge to the target branch",
        _ => "Allow edits from maintainers",
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
                "Its fork was deleted",
                "The branch can't be checked out to push to: its repository is gone",
            );
        } else if info.cross {
            let set_up = self.pushing_to_fork(info);
            let refused = format!(
                "{} doesn't let maintainers push to {}: ask them to tick “{}”",
                info.owner,
                info.branch,
                allow_edits(self.forge())
            );
            let (text, tip) = if !info.maintainers_can_edit {
                (
                    format!("Check out to push to {}", info.owner),
                    refused.clone(),
                )
            } else if let Some(b) = set_up {
                (
                    format!("Check out {} (pushes to {})", b.name, info.owner),
                    format!(
                        "Set up already: a push goes to {}'s {}",
                        info.owner, info.branch
                    ),
                )
            } else {
                (
                    format!("Check out to push to {}", info.owner),
                    format!(
                        "Adds {}'s fork as a remote (unless one is it already), checks their \
                         branch {} out tracking it, and sets the branch's pushRemote so that \
                         Push goes back to them. Ctrl+Z undoes it",
                        info.owner, info.branch
                    ),
                )
            };
            if item(ui, idle && info.maintainers_can_edit, &text, &tip) {
                acts.push(Action::Fork(ForkAct::Checkout(number)));
            }
            let here = set_up.is_some_and(|b| self.refs.current.as_deref() == Some(&b.name));
            let tip = if !info.maintainers_can_edit {
                refused
            } else if here {
                format!(
                    "Fetches {} and {}'s branch, rebases it onto {}, then force-pushes it to \
                     them with a lease, after showing what it will do",
                    info.base, info.owner, info.base
                )
            } else {
                format!("Check out {}'s branch to push to them first", info.owner)
            };
            let text = format!("Rebase onto {} and push to {}", info.base, info.owner);
            if item(ui, idle && here && info.maintainers_can_edit, &text, &tip) {
                acts.push(Action::Fork(ForkAct::Rebase(number)));
            }
        }
        let waiting = self.forks.waiting(&info.sha).len();
        if waiting > 0 {
            let tip = "Approves the workflow runs of its latest commit, which then run the \
                       contributor's code on your CI: look through its changes first";
            if item(ui, idle, &format!("Approve and run ({waiting})…"), tip) {
                acts.push(Action::Fork(ForkAct::Approve(number)));
            }
        }
    }
}
