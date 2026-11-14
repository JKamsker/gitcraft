//! The local branches: their section, rows and menu.

use egui::{Align2, Rect, Response, Ui, pos2, vec2};

use super::{ROW_H, matches, remote_short, visibility};
use crate::a11y::labels;
use crate::ci::Status;
use crate::git::Branch;
use crate::icons;
use crate::repo::actions::{BranchRef, compare_branches_action, interactive_rebase_action};
use crate::repo::branch_health::Reason;
use crate::repo::branch_order::BranchOrder;
use crate::repo::branch_tree::{self, LeafRow, TreeView};
use crate::repo::branch_visibility::local_ref;
use crate::repo::conflict_check::Op;
use crate::repo::ghost::{GhostOp, offer};
use crate::repo::graph_view;
use crate::repo::pinned_sections::{Button, Header, Sections};
use crate::repo::reflog::{ReflogAct, Source};
use crate::repo::{
    Action, Dialog, RepoTab, ci_status, fast_forward_action, merge_action, ops, rebase_action,
    widgets,
};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn local_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        filter: &str,
        sections: &mut Sections,
    ) {
        let local: Vec<&Branch> = self
            .refs
            .local
            .iter()
            .filter(|b| matches(filter, b))
            .collect();
        let can_branch = idle && !self.refs.head.is_empty();
        let open = sections.add(
            ui,
            Header {
                key: crate::i18n::text("repo-core-2601"),
                icon: "laptop",
                title: crate::i18n::text("repo-core-3375"),
                count: local.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: crate::i18n::text("repo-core-3376"),
                    action: can_branch
                        .then(|| Action::Dialog(Dialog::create_branch(&self.refs.head, "HEAD"))),
                }),
            },
        );
        if !open {
            return;
        }
        let order = BranchOrder::load(ui.ctx(), self.repo_key());
        let by_date = self
            .branch_health
            .health
            .as_ref()
            .map_or(&[][..], |h| h.by_date.as_slice());
        let (pinned, local) = order.arrange(local, by_date);
        let stale_days = Settings::get(ui.ctx()).stale_branch_days;
        for b in pinned {
            let row = LeafRow {
                name: &b.name,
                indent: 0.0,
            };
            self.local_branch_row(ui, acts, b, row, idle, stale_days, true);
        }
        let tree = branch_tree::build(local.iter().map(|b| (b.name.as_str(), *b)));
        let view = TreeView::branches(
            egui::Id::new((
                "branch-folder",
                self.repo_key(),
                crate::i18n::text("repo-core-2601"),
            )),
            !filter.is_empty(),
            0.0,
        );
        branch_tree::show(
            ui,
            &tree,
            &view,
            &|b: &&Branch| b.is_head,
            &|b: &&Branch| ("branch-row", local_ref(&b.name)),
            &mut |ui, row, b: &&Branch| {
                self.local_branch_row(ui, acts, b, row, idle, stale_days, false);
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn local_branch_row(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        row: LeafRow<'_>,
        idle: bool,
        stale_days: u32,
        pinned: bool,
    ) {
        let (ahead, behind) = b.ahead_behind();
        // Checked out in another worktree: git won't check it out here too.
        let elsewhere = self
            .worktrees
            .iter()
            .find(|w| !w.current && w.branch.as_deref() == Some(b.name.as_str()));
        let has_pr = self.prs.iter().any(|p| p.branch == b.name);
        let ci = self.ci_status(&b.target);
        let full = local_ref(&b.name);
        let shown = self.branch_shown(&full, b.is_head);
        let reasons = self.branch_reasons(b, stale_days);
        let marks = Marks {
            gone: b.upstream_gone(),
            badge: badge(&reasons),
            pinned,
            in_worktree: elsewhere.is_some(),
            has_pr,
            ci: ci.map(|status| (b.target.as_str(), status)),
            hidden: !shown,
        };
        let (r, ci_act) = branch_row(ui, &full, row, b.is_head, ahead, behind, marks);
        acts.extend(ci_act);
        let r = self.branch_drag_drop(ui, r, &b.name, false, &b.target, acts);
        if !b.is_head {
            acts.extend(visibility::eye(ui, &r, &full, shown));
        }
        let mut tip = match (b.upstream.is_empty(), b.upstream_gone()) {
            (true, _) => crate::i18n::text("repo-core-3379").to_owned(),
            (false, true) => crate::i18n::message(
                "repo-core-3380",
                &[("upstream", b.upstream.to_string().into())],
            ),
            (false, false) => crate::i18n::message(
                "repo-core-3381",
                &[("upstream", b.upstream.to_string().into())],
            ),
        };
        if let Some(status) = ci {
            tip.push_str(&crate::i18n::message(
                "repo-core-3382",
                &[(
                    "state",
                    ci_status::state_word(status.state).to_string().into(),
                )],
            ));
        }
        if pinned {
            tip.push_str(crate::i18n::text("repo-core-3383"));
        }
        for reason in reasons.iter().filter(|r| **r != Reason::UpstreamGone) {
            tip.push_str(&format!("\n{}", capitalise(&reason.describe())));
        }
        let r = match elsewhere {
            Some(wt) => r.on_hover_text(crate::i18n::message(
                "repo-core-3384",
                &[
                    ("name", b.name.to_string().into()),
                    ("tip", tip.to_string().into()),
                    ("wt", format!("{}", wt.path.display()).into()),
                ],
            )),
            None => r.on_hover_text(crate::i18n::message(
                "repo-core-3385",
                &[
                    ("name", b.name.to_string().into()),
                    ("tip", tip.to_string().into()),
                ],
            )),
        };
        if r.double_clicked() && elsewhere.is_some() {
            if let Some(wt) = elsewhere {
                acts.push(Action::OpenWorktree(wt.path.clone()));
            }
        } else if r.double_clicked() && !b.is_head && idle {
            acts.push(Action::Checkout(b.name.clone()));
        } else if r.clicked() {
            acts.push(Action::SelectBranch(b.target.clone()));
        }
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| self.branch_menu_items(ui, acts, b, idle));
    }
}

/// A local or remote-tracking branch found by name.
pub(in crate::repo) struct OwnedBranch {
    pub(in crate::repo) name: String,
    pub(in crate::repo) remote: bool,
}

impl OwnedBranch {
    fn as_ref(&self) -> BranchRef<'_> {
        BranchRef {
            name: &self.name,
            remote: self.remote,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, target: char) -> Branch {
        Branch {
            name: name.to_owned(),
            target: target.to_string().repeat(40),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        }
    }

    /// A branch's menu left open while a fetch, the watcher or the terminal changes the list is
    /// that branch's: it stays open when the branch only moves down a row, and closes once the
    /// branch is gone, rather than turning into the menu of the branch that took its row, whose
    /// Merge or Rebase runs at once.
    #[test]
    fn a_branch_menu_does_not_move_to_the_branch_that_takes_its_row() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/branch-menu-test".into(), &ctx);
        tab.refs.local = vec![branch("alpha", 'a'), branch("beta", 'b')];
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 600.0));
        // Under the section's header (26 px), the second row's middle.
        let second = pos2(160.0, 26.0 + ROW_H * 1.5);
        let frame = |tab: &RepoTab, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let mut sections = Sections::new(egui::Id::new("branch-menu-test"), None);
                        tab.local_section(ui, &mut Vec::new(), true, "", &mut sections);
                    });
            })
            .drop_without_applying_deltas();
        };
        frame(&tab, vec![egui::Event::PointerMoved(second)]);
        let click = |pressed| egui::Event::PointerButton {
            pos: second,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&tab, vec![click(true), click(false)]);
        frame(&tab, Vec::new());
        let opened = egui::Popup::is_any_open(&ctx);
        // A branch made before it: "beta" moves a row down, its menu with it.
        tab.refs.local = vec![
            branch("alpha", 'a'),
            branch("ann", 'c'),
            branch("beta", 'b'),
        ];
        frame(&tab, Vec::new());
        frame(&tab, Vec::new());
        let followed = egui::Popup::is_any_open(&ctx);
        // "beta" deleted (or pruned): "gamma" is in the row the menu was opened on.
        tab.refs.local = vec![branch("alpha", 'a'), branch("gamma", 'd')];
        frame(&tab, Vec::new());
        frame(&tab, Vec::new());
        assert!(opened, "the right-click opened no menu");
        assert!(followed, "the menu closed when its branch moved down a row");
        assert!(!egui::Popup::is_any_open(&ctx));
    }

    /// A pinned branch is the first row of Local, whatever its name: right-clicking that row
    /// opens its menu.
    #[test]
    fn a_pinned_branch_is_the_first_row() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/branch-pin-test".into(), &ctx);
        tab.refs.local = vec![
            branch("alpha", 'a'),
            branch("beta", 'b'),
            branch("zeta", 'c'),
        ];
        let mut order = BranchOrder::default();
        order.toggle_pin("zeta");
        order.store(&ctx, tab.repo_key());
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 600.0));
        let first = pos2(160.0, 26.0 + ROW_H * 0.5);
        let mut zeta_menu = false;
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let mut sections = Sections::new(egui::Id::new("branch-pin-test"), None);
                        tab.local_section(ui, &mut Vec::new(), true, "", &mut sections);
                        zeta_menu =
                            widgets::keyed_row_menu_open(ui, ("branch-row", "refs/heads/zeta"));
                    });
            })
            .drop_without_applying_deltas();
        };
        frame(vec![egui::Event::PointerMoved(first)]);
        let click = |pressed| egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![click(true), click(false)]);
        frame(Vec::new());
        frame(Vec::new());
        assert!(zeta_menu, "the first row is not the pinned branch's");
    }
}

mod checkout;
mod menu;
mod publication;
mod row;
use row::{badge, branch_row, capitalise};

/// A word on a branch's row saying it could be cleaned up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Badge {
    Merged,
    Stale,
}

/// What a branch's row shows besides its name and how far ahead and behind it is.
struct Marks<'a> {
    /// Its upstream was deleted on the remote.
    gone: bool,
    badge: Option<Badge>,
    /// Kept at the top of Local: a pin instead of the branch icon.
    pinned: bool,
    /// Checked out in another worktree.
    in_worktree: bool,
    has_pr: bool,
    /// Its tip and the tip's build status.
    ci: Option<(&'a str, &'a Status)>,
    /// Left out of the graph: the name is dimmed.
    hidden: bool,
}
