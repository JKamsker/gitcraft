//! Drag and drop in the graph and the sidebar: a branch's pill or row onto another branch or a
//! commit merges, rebases or fast-forwards; a commit onto what is checked out cherry-picks it;
//! a commit of the checked-out branch onto another of it moves or squashes it. A drop opens a
//! small menu of what makes sense there, each entry the action of the menu entry of the same
//! name, so it asks, handles conflicts and undoes the same way.

mod ancestry;
mod menu;
#[cfg(test)]
mod offer_tests;
mod offers;
mod reorder;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use egui::{PointerButton, Response, Ui};

use super::actions::{BranchRef, interactive_rebase_action};
use super::operations::{expect_ref, expect_ref_at, expect_symref};
use super::{
    Action, Refresh, RepoTab, Undo, fast_forward_action, merge_action, ops, rebase_action, short,
};
pub(crate) use ancestry::Reach;
pub(crate) use menu::DropMenu;
pub(crate) use offers::{DragBranch, DragCommit, DragItem, DropTarget, Line, Offer, Rev, Scene};
pub(crate) use reorder::CommitDrop;

/// What is being dragged, with what its drop targets are worked out from, read when the drag
/// started.
pub(crate) struct Payload {
    /// The repository it comes from: a drag that ends in another tab does nothing there.
    repo: PathBuf,
    pub(crate) item: DragItem,
    reach: Reach,
    line: Line,
}

impl Payload {
    pub(in crate::repo) fn repository(&self) -> &std::path::Path {
        &self.repo
    }
}

/// The pointer is over a drop target this frame: whether it takes what is dragged, and the
/// first thing it offers.
#[derive(Clone, Default)]
pub(crate) struct Hover {
    pub(crate) first: Option<String>,
    /// How many more it offers.
    pub(crate) more: usize,
    /// Why it offers nothing, when so.
    pub(crate) why: Option<String>,
}

/// Where the hover of this frame is kept for the drag ghost.
fn hover_id() -> egui::Id {
    egui::Id::new("drag-drop-hover")
}

impl RepoTab {
    /// Starts dragging `item` (made only then) once `resp` is dragged past egui's click
    /// distance with the primary button: clicks, double-clicks and right-clicks stay as they
    /// were.
    pub(in crate::repo) fn drag_source(
        &self,
        resp: &Response,
        item: impl FnOnce() -> Option<DragItem>,
    ) {
        if !resp.drag_started_by(PointerButton::Primary) {
            return;
        }
        if let Some(item) = item() {
            egui::DragAndDrop::set_payload(&resp.ctx, self.payload(item));
        }
    }

    /// `item` as dragged now, with what its targets are worked out from.
    fn payload(&self, item: DragItem) -> Payload {
        let tip = match &item {
            DragItem::Branch(b) => b.tip.clone(),
            DragItem::Commit(c) => c.hash.clone(),
        };
        let reach = Reach::new(&self.commits, &self.index, &[&tip, &self.refs.head]);
        let line = match &item {
            DragItem::Commit(_) if self.refs.current.is_some() => Line::new(self.head_line()),
            _ => Line::default(),
        };
        Payload {
            repo: self.path.clone(),
            item,
            reach,
            line,
        }
    }

    /// What is being dragged in this tab, if anything.
    pub(in crate::repo) fn dragged(&self, ctx: &egui::Context) -> Option<Arc<Payload>> {
        egui::DragAndDrop::payload::<Payload>(ctx).filter(|p| p.repo == self.path)
    }

    /// The checked-out branch from HEAD down its first parents, as far as loaded.
    fn head_line(&self) -> Vec<Arc<crate::git::Commit>> {
        let mut line = Vec::new();
        let mut next = Some(self.refs.head.as_str());
        while let Some(c) = next
            .and_then(|h| self.index.get(h))
            .and_then(|&i| self.commits.get(i))
        {
            line.push(Arc::clone(c));
            next = c.parents.first().map(String::as_str);
        }
        line
    }

    /// Why a target takes nothing of `item`, said in the ghost over it.
    fn nothing_here(&self, item: &DragItem) -> String {
        match item {
            _ if self.refs.current.is_none() => crate::i18n::text("repo-core-0842").to_owned(),
            DragItem::Commit(c) if c.merge => crate::i18n::text("repo-core-4081").to_owned(),
            DragItem::Commit(_) => crate::i18n::text("repo-core-4082").to_owned(),
            DragItem::Branch(_) => crate::i18n::text("repo-core-4083").to_owned(),
        }
    }

    /// Why nothing can be dropped now, if so.
    pub(in crate::repo) fn drop_blocked(&self) -> Option<String> {
        if let Some(op) = &self.busy_op {
            return Some(crate::i18n::message(
                "repo-core-4084",
                &[("op", format!("{}", op).into())],
            ));
        }
        self.work.in_progress.map(|s| {
            crate::i18n::message(
                "repo-core-4085",
                &[(
                    "to-lowercase",
                    format!("{}", s.label().to_lowercase()).into(),
                )],
            )
        })
    }

    /// What dropping `payload` on `target` offers now.
    pub(in crate::repo) fn drop_offers(
        &self,
        payload: &Payload,
        target: &DropTarget,
    ) -> Vec<Offer> {
        if self.drop_blocked().is_some() {
            return Vec::new();
        }
        let elsewhere: Vec<String> = self
            .worktrees
            .iter()
            .filter(|w| !w.current)
            .filter_map(|w| w.branch.clone())
            .collect();
        let ancestry = payload.reach.ancestry(&self.commits, &self.index);
        let scene = Scene {
            current: self.refs.current.as_deref(),
            head: &self.refs.head,
            line: &payload.line,
            elsewhere: &elsewhere,
            ancestry: &ancestry,
        };
        offers::offers(&payload.item, target, &scene)
    }

    /// A branch's row in the sidebar (`resp`): dragged, it drags the branch `name` (at `tip`);
    /// dropped on, it is that branch. Returns the row's response, now also sensing drags.
    pub(in crate::repo) fn branch_drag_drop(
        &self,
        ui: &Ui,
        resp: Response,
        name: &str,
        remote: bool,
        tip: &str,
        acts: &mut Vec<Action>,
    ) -> Response {
        let resp = resp.interact(egui::Sense::drag());
        let branch = || DragBranch {
            name: name.to_owned(),
            remote,
            tip: tip.to_owned(),
        };
        self.drag_source(&resp, || Some(DragItem::Branch(branch())));
        if self.dragged(ui.ctx()).is_some() {
            self.drop_zone(ui, &resp, &DropTarget::Branch(branch()), acts);
        }
        resp
    }

    /// `resp` as a drop target for `target` alone: see [`Self::drop_zone_with`].
    pub(in crate::repo) fn drop_zone(
        &self,
        ui: &Ui,
        resp: &Response,
        target: &DropTarget,
        acts: &mut Vec<Action>,
    ) {
        let Some(payload) = self.dragged(ui.ctx()) else {
            return;
        };
        let offers = self.drop_offers(&payload, target);
        let hovered = resp.contains_pointer();
        self.drop_zone_with(ui, &payload, resp.rect, target, offers, hovered, acts);
    }

    /// A drop target for `target`, offering `offers` (empty: it doesn't take what is dragged):
    /// lights up `light` while it takes it, and while `hovered` opens the drop menu once the
    /// drag ends there.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::repo) fn drop_zone_with(
        &self,
        ui: &Ui,
        payload: &Payload,
        light: egui::Rect,
        target: &DropTarget,
        offers: Vec<Offer>,
        hovered: bool,
        acts: &mut Vec<Action>,
    ) {
        menu::paint_zone(ui, light, !offers.is_empty(), hovered);
        // Over where the drag started, nothing is said yet.
        if !hovered || is_source(&payload.item, target) {
            return;
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                hover_id(),
                Hover {
                    first: offers.first().map(Offer::label),
                    more: offers.len().saturating_sub(1),
                    why: offers.is_empty().then(|| self.nothing_here(&payload.item)),
                },
            );
        });
        if ui.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            if !offers.is_empty() {
                let at = ui.ctx().pointer_interact_pos().unwrap_or(light.center());
                acts.push(Action::DropMenu(DropMenu::new(
                    at,
                    &payload.item,
                    target,
                    offers,
                )));
            }
        } else if offers.is_empty() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
        }
    }
}

/// Whether `target` is what `item` was dragged from.
fn is_source(item: &DragItem, target: &DropTarget) -> bool {
    match (item, target) {
        (DragItem::Branch(a), DropTarget::Branch(b)) => a.name == b.name && a.remote == b.remote,
        (DragItem::Commit(a), DropTarget::Commit(b)) => a.hash == b.hash,
        _ => false,
    }
}

/// The local or remote branch `name` as an action takes it.
fn branch_ref(name: &str, remote: bool) -> BranchRef<'_> {
    BranchRef { name, remote }
}

/// `action`'s steps refused unless `checked_out` (a branch, or `HEAD` when detached) is still
/// what is checked out when it runs: the drop named it, and another branch checked out since
/// (in the terminal) must not be the one merged into or rebased.
fn on_checked_out(action: Action, checked_out: &str) -> Action {
    match action {
        Action::UndoableOps {
            label,
            ops,
            refresh,
            undo,
        } => {
            let branch = if checked_out == "HEAD" {
                String::new()
            } else {
                format!("refs/heads/{checked_out}")
            };
            Action::UndoableOps {
                label,
                ops: [vec![expect_symref("HEAD", &branch)], ops].concat(),
                refresh,
                undo,
            }
        }
        other => other,
    }
}

/// An undoable git command on what is checked out.
fn head_move(label: String, args: &[&str]) -> Action {
    Action::UndoableOps {
        label,
        ops: ops(&[args]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Moves the local branch `branch`, which isn't checked out, from `from_tip` forward to `to`
/// (at `to_tip`, which the drop found it behind). Only while it is still at `from_tip`: a
/// commit made on it since would not be a fast-forward of what was shown. `git branch -f`
/// refuses a branch checked out here or in another worktree, or being rebased or bisected
/// there, which may have happened since the drop. Undo puts it back while it is still where
/// this left it, under the same checks.
fn fast_forward_branch(branch: &str, to: &Rev, to_tip: &str, from_tip: &str) -> Action {
    let full = format!("refs/heads/{branch}");
    let force = |at: &str| ops(&[&["branch", "-f", "--end-of-options", branch, at]]);
    Action::UndoableOps {
        label: crate::i18n::message(
            "repo-core-2425",
            &[
                ("branch", format!("{}", branch).into()),
                ("label", format!("{}", to.label()).into()),
            ],
        ),
        ops: [vec![expect_ref_at(&full, from_tip)], force(to_tip)].concat(),
        refresh: Refresh::All,
        undo: Undo::Custom {
            undo: [vec![expect_ref(&full)], force(from_tip)].concat(),
            redo: [vec![expect_ref_at(&full, from_tip)], force(to_tip)].concat(),
        },
    }
}

impl Offer {
    /// The action of the menu entry: the same one the commit's or branch's menu runs.
    pub(crate) fn action(&self) -> Action {
        match self {
            Offer::Merge { from, into } => on_checked_out(
                match from {
                    Rev::Branch { name, remote } => merge_action(branch_ref(name, *remote), into),
                    Rev::Commit { hash } => head_move(
                        crate::i18n::message(
                            "repo-core-2422",
                            &[
                                ("hash", format!("{}", short(hash)).into()),
                                ("into", format!("{}", into).into()),
                            ],
                        ),
                        &["merge", "--no-edit", hash],
                    ),
                },
                into,
            ),
            Offer::Rebase { branch, onto } => on_checked_out(
                match onto {
                    Rev::Branch { name, remote } => {
                        rebase_action(branch_ref(name, *remote), branch)
                    }
                    Rev::Commit { hash } => head_move(
                        crate::i18n::message(
                            "repo-core-2423",
                            &[
                                ("branch", format!("{}", branch).into()),
                                ("hash", format!("{}", short(hash)).into()),
                            ],
                        ),
                        &["rebase", hash],
                    ),
                },
                branch,
            ),
            Offer::InteractiveRebase { onto, .. } => match onto {
                Rev::Branch { name, remote } => {
                    interactive_rebase_action(branch_ref(name, *remote))
                }
                Rev::Commit { hash } => Action::InteractiveRebase {
                    onto: hash.clone(),
                    label: short(hash).to_owned(),
                },
            },
            Offer::FastForward {
                branch,
                to,
                to_tip,
                checked_out: true,
                ..
            } => on_checked_out(
                match to {
                    Rev::Branch { name, remote } => {
                        fast_forward_action(branch_ref(name, *remote), branch)
                    }
                    Rev::Commit { hash } => head_move(
                        crate::i18n::message(
                            "repo-core-2425",
                            &[
                                ("branch", format!("{}", branch).into()),
                                ("hash", format!("{}", short(hash)).into()),
                            ],
                        ),
                        &["merge", "--ff-only", to_tip],
                    ),
                },
                branch,
            ),
            Offer::FastForward {
                branch,
                to,
                to_tip,
                from_tip,
                checked_out: false,
            } => fast_forward_branch(branch, to, to_tip, from_tip),
            Offer::CherryPick { hash, onto } => {
                on_checked_out(super::actions::cherry_pick_action(hash), onto)
            }
            Offer::Rewrite(drop) => Action::CommitDrop(drop.clone()),
        }
    }
}
