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
                key: "local",
                icon: "laptop",
                title: "LOCAL",
                count: local.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: "New branch at HEAD…",
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
            egui::Id::new(("branch-folder", self.repo_key(), "local")),
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
            (true, _) => "Not tracking a remote branch".to_owned(),
            (false, true) => format!("{} was deleted on the remote", b.upstream),
            (false, false) => format!("Tracks {}", b.upstream),
        };
        if let Some(status) = ci {
            tip.push_str(&format!("\nCI: {}", ci_status::state_word(status.state)));
        }
        if pinned {
            tip.push_str("\nPinned to top");
        }
        for reason in reasons.iter().filter(|r| **r != Reason::UpstreamGone) {
            tip.push_str(&format!("\n{}", capitalise(&reason.describe())));
        }
        let r = match elsewhere {
            Some(wt) => r.on_hover_text(format!(
                "{}\n{tip}\nChecked out in worktree {}\nDouble-click to open it, or drag onto \
                 a branch or a commit",
                b.name,
                wt.path.display()
            )),
            None => r.on_hover_text(format!(
                "{}\n{tip}\nDouble-click to check out, or drag onto a branch or a commit",
                b.name
            )),
        };
        if r.double_clicked() && elsewhere.is_some() {
            if let Some(wt) = elsewhere {
                acts.push(Action::OpenWorktree(wt.path.clone()));
            }
        } else if r.double_clicked() && !b.is_head && idle {
            acts.push(Action::Checkout(b.name.clone()));
        } else if r.clicked() {
            acts.push(Action::SelectHash(b.target.clone()));
        }
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| self.branch_menu_items(ui, acts, b, idle));
    }

    /// The menu of a local branch: in the sidebar and in the graph's commit menu.
    pub(in crate::repo) fn branch_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        self.checkout_items(ui, acts, b, idle);
        if b.is_head
            && b.name.starts_with(&self.hotfix_prefix())
            && crate::menu_item::MenuItem::new("Finish hotfix…")
                .icon("package")
                .enabled(idle)
                .show(ui)
                .on_hover_text(
                    "Tags it, merges it back into the main branch (and the development one), \
                     deletes it and pushes it all, after showing what it will do",
                )
                .clicked()
        {
            acts.push(Action::Hotfix(crate::repo::hotfix::HotfixAct::OpenFinish));
        }
        if let Some(current) = self.refs.current.as_deref()
            && !b.is_head
        {
            ui.separator();
            self.integrate_items(ui, acts, b, current, idle);
        }
        // The checked-out branch against the one pull requests usually go to.
        if b.is_head
            && let Some(base) = self.base_ref()
            && base.name != b.name
        {
            ui.separator();
            if crate::menu_item::MenuItem::new(format!("Changes {} adds to {}", b.name, base.name))
                .icon("command")
                .show(ui)
                .on_hover_text("What it changed since the two diverged, as a pull request shows it")
                .clicked()
            {
                acts.push(compare_branches_action(
                    base.as_ref(),
                    BranchRef::local(&b.name),
                    true,
                ));
            }
        }
        self.review_item(ui, acts, b);
        self.versions_items(ui, acts, b);
        ui.separator();
        self.publish_items(ui, acts, b, idle);
        self.stack_menu_items(ui, acts, b, idle);
        ui.separator();
        if crate::menu_item::MenuItem::new("Create branch here…")
            .icon("git-branch-plus")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::create_branch(&b.target, &b.name)));
        }
        if crate::menu_item::MenuItem::new("Create tag here…")
            .icon("tag")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::tag(
                &b.target,
                &b.name,
                self.default_remote(),
            )));
        }
        if crate::menu_item::MenuItem::new("Rename…")
            .icon("wrench")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::rename_branch(&b.name)));
        }
        if crate::menu_item::MenuItem::new("Copy name")
            .icon("copy")
            .show(ui)
            .clicked()
        {
            ui.ctx().copy_text(b.name.clone());
        }
        if crate::menu_item::MenuItem::new("Show reflog")
            .icon("history")
            .show(ui)
            .on_hover_text(
                "Where the branch has been: each commit it pointed at, and what moved it",
            )
            .clicked()
        {
            acts.push(Action::Reflog(ReflogAct::Show(Source::branch(&b.name))));
        }
        if let Some((menu, target)) = self.branch_link_menu(&b.name) {
            menu.items(ui, "branch", &target, acts);
        }
        self.pin_item(ui, &b.name);
        if crate::menu_item::MenuItem::new("How much agents wrote")
            .icon("bot")
            .show(ui)
            .on_hover_text(
                "Of the lines this branch added since it left the main branch: how many agents \
                 wrote (as the commits record it), and how many of those people edited since",
            )
            .clicked()
        {
            acts.push(Action::BranchAuthorship(b.name.clone()));
        }
        ui.separator();
        self.visibility_items(ui, acts, &local_ref(&b.name), b.is_head);
        ui.separator();
        self.delete_items(ui, acts, b, idle);
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Branch,
            self.branch_scope(&b.name, &b.target, None),
            acts,
        );
    }

    /// _Review this branch_: the AI tool reviews what `b` adds to the branch pull requests
    /// usually go to (to the checked-out one, for that branch itself).
    fn review_item(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch) {
        let base = self
            .base_ref()
            .map(|base| base.name)
            .filter(|base| *base != b.name)
            .or_else(|| self.refs.current.clone().filter(|c| *c != b.name));
        ui.separator();
        let button = crate::menu_item::MenuItem::new("Review this branch")
            .icon("git-pull-request")
            .refused(
                base.is_none()
                    .then_some("No base branch is available for this review"),
            )
            .show(ui);
        let Some(base) = base else {
            button.on_disabled_hover_text("There is no other branch to review it against");
            return;
        };
        let tip = if Settings::get(ui.ctx()).ai.enabled() {
            format!("Have your AI tool review what {} adds to {base}", b.name)
        } else {
            "Pick an AI tool first (Settings → AI, or the sparkles button of the commit box)"
                .to_owned()
        };
        if button.on_hover_text(tip).clicked() {
            acts.push(Action::ReviewBranch {
                branch: b.name.clone(),
                base,
            });
        }
    }

    /// Pin to top, or Unpin: kept for this repository.
    fn pin_item(&self, ui: &mut Ui, name: &str) {
        let mut order = BranchOrder::load(ui.ctx(), self.repo_key());
        let label = if order.is_pinned(name) {
            "Unpin"
        } else {
            "Pin to top"
        };
        if crate::menu_item::MenuItem::new(label)
            .icon("pin")
            .show(ui)
            .on_hover_text("Pinned branches stay at the top of Local")
            .clicked()
        {
            order.toggle_pin(name);
            order.store(ui.ctx(), self.repo_key());
        }
    }

    /// Check out `b`, here or in a new worktree; or open the worktree it is checked out in.
    fn checkout_items(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch, idle: bool) {
        let elsewhere = self
            .worktrees
            .iter()
            .find(|w| !w.current && w.branch.as_deref() == Some(b.name.as_str()));
        if let Some(wt) = elsewhere {
            let label = if Settings::get(ui.ctx()).worktrees_in_same_tab {
                "Open its worktree"
            } else {
                "Open its worktree in a tab"
            };
            if crate::menu_item::MenuItem::new(label)
                .icon("folder-git-2")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenWorktree(wt.path.clone()));
            }
            return;
        }
        if crate::menu_item::MenuItem::new("Check out")
            .icon("log-in")
            .enabled(idle)
            .refused(b.is_head.then_some("This commit is already checked out"))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Checkout(b.name.clone()));
        }
        if crate::menu_item::MenuItem::new("Check out in a new worktree…")
            .icon("folder-git-2")
            .enabled(idle)
            .refused(b.is_head.then_some("This commit is already checked out"))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::add_worktree(
                &self.path,
                Some(&b.name),
                &self.refs.head,
            )));
        }
    }

    /// Push `b` (checked out or not), set what it tracks, and open or create its pull request.
    fn publish_items(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch, idle: bool) {
        if let Some((to, push)) = self.push_branch(b)
            && crate::menu_item::MenuItem::new(format!("Push to {to}"))
                .icon("arrow-up-from-line")
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            acts.push(push);
        }
        self.force_push_item(ui, acts, b, idle);
        if let Some(remote) = self.default_remote()
            && crate::menu_item::MenuItem::new("Set upstream…")
                .icon("git-branch")
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            let (remote, name) = self
                .split_upstream(&b.upstream)
                .map(|(r, n)| (r, n.to_owned()))
                .unwrap_or((remote, b.name.clone()));
            let undo = if b.upstream.is_empty() {
                ops(&[&["branch", "--unset-upstream", &b.name]])
            } else {
                self.track_ops(&b.name, &b.upstream)
            };
            acts.push(Action::Dialog(Dialog::set_upstream(
                &b.name, remote, &name, undo,
            )));
        }
        if !b.upstream.is_empty()
            && crate::menu_item::MenuItem::new(format!("Stop tracking {}", b.upstream))
                .icon("git-branch")
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            acts.push(self.unset_upstream_action(b));
        }
        let Some(forge) = self.forge() else { return };
        match self.prs.iter().find(|p| p.branch == b.name) {
            Some(pr) => {
                if crate::menu_item::MenuItem::new(format!("Open {} #{}", forge.noun(), pr.number))
                    .icon("external-link")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                }
            }
            None => {
                if crate::menu_item::MenuItem::new(format!("Create {}…", forge.noun()))
                    .icon("external-link")
                    .enabled(idle)
                    .show(ui)
                    .clicked()
                {
                    // A branch made for an issue: its title, closing it once merged, and into the
                    // branch it started from.
                    let (title, body, base) =
                        self.issue_pull_request(&b.name).unwrap_or_else(|| {
                            let summary = self.history.iter().find(|c| c.hash == b.target);
                            (
                                summary.map(|c| c.summary.clone()).unwrap_or_default(),
                                String::new(),
                                self.default_base(),
                            )
                        });
                    acts.push(Action::Dialog(Dialog::create_pr(
                        forge.noun(),
                        &b.name,
                        &base,
                        &title,
                        &body,
                    )));
                }
            }
        }
    }

    /// Force push (with lease) of `b` to the branch it tracks, when that has commits `b`
    /// hasn't (a plain push is refused). Hovering it shows what would leave the remote.
    fn force_push_item(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch, idle: bool) {
        let (_, behind) = b.ahead_behind();
        if behind == 0 || b.upstream_gone() {
            return;
        }
        let Some((remote, name)) = self.split_upstream(&b.upstream) else {
            return;
        };
        let Some(tracked) = self
            .refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list)
            .find(|r| r.name == b.upstream)
        else {
            return;
        };
        let button =
            crate::menu_item::MenuItem::new(format!("Force push to {} (with lease)…", b.upstream))
                .icon("arrow-up-from-line")
                .enabled(idle)
                .show(ui)
                .on_hover_text(format!(
                    "Replace {} with {}, unless someone pushed to it since it was fetched",
                    b.upstream, b.name
                ));
        offer(&button, || GhostOp::ForcePush {
            branch: b.name.clone(),
        });
        if button.clicked() {
            acts.push(Action::Dialog(Dialog::confirm(
                "Force push",
                format!(
                    "Replace “{name}” on {remote} with {}? {} it has that {} hasn't {} it, for \
                     everyone.",
                    b.name,
                    if behind == 1 {
                        "The commit".to_owned()
                    } else {
                        format!("The {behind} commits")
                    },
                    b.name,
                    if behind == 1 { "leaves" } else { "leave" },
                ),
                "Force push",
                ops(&[&[
                    "push",
                    &format!("--force-with-lease=refs/heads/{name}:{}", tracked.target),
                    &remote,
                    &format!("refs/heads/{}:refs/heads/{name}", b.name),
                ]]),
                None,
            )));
        }
    }

    /// The branch to compare the current one with: the local default base, or its remote one.
    /// The name of the branch pull requests go into (see [`Self::base_ref`]).
    pub(in crate::repo) fn base_branch_name(&self) -> Option<String> {
        self.base_ref().map(|b| b.name)
    }

    pub(in crate::repo) fn base_ref(&self) -> Option<OwnedBranch> {
        let base = self.default_base();
        if self.refs.local.iter().any(|b| b.name == base) {
            return Some(OwnedBranch {
                name: base,
                remote: false,
            });
        }
        let remote = format!("{}/{base}", self.default_remote()?);
        self.refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list)
            .any(|b| b.name == remote)
            .then_some(OwnedBranch {
                name: remote,
                remote: true,
            })
    }

    /// The branch pull requests go into by default: main, master or develop on the default
    /// remote, else main.
    pub(in crate::repo) fn default_base(&self) -> String {
        let remote = self.default_remote().unwrap_or_else(|| "origin".to_owned());
        let names: Vec<&str> = self
            .remote_branches(&remote)
            .filter_map(|b| remote_short(&b.name, &remote))
            .collect();
        for candidate in ["main", "master", "develop"] {
            if names.contains(&candidate) {
                return candidate.to_owned();
            }
        }
        "main".to_owned()
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

impl RepoTab {
    /// Fast-forward, merge, rebase and compare `current` with `b`.
    fn integrate_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        current: &str,
        idle: bool,
    ) {
        if crate::menu_item::MenuItem::new(format!("Fast-forward {current} to {}", b.name))
            .icon("arrow-right")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(fast_forward_action(BranchRef::local(&b.name), current));
        }
        let merge = format!("Merge {} into {current}", b.name);
        if self
            .conflict_button(ui, merge, idle, Op::Merge, &b.target)
            .clicked()
        {
            acts.push(merge_action(BranchRef::local(&b.name), current));
        }
        let rebase = format!("Rebase {current} onto {}", b.name);
        let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, &b.target);
        offer(&rebase, || GhostOp::Rebase {
            onto: BranchRef::local(&b.name).full(),
            label: b.name.clone(),
        });
        if rebase.clicked() {
            acts.push(rebase_action(BranchRef::local(&b.name), current));
        }
        if crate::menu_item::MenuItem::new(format!("Interactive rebase {current} onto {}…", b.name))
            .icon("git-merge")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(interactive_rebase_action(BranchRef::local(&b.name)));
        }
        ui.separator();
        graph_view::compare_items(ui, acts, BranchRef::local(&b.name), current);
        crate::repo::interaction::interaction_item(ui, acts, &b.name);
        crate::repo::rehearse::rehearse_item(ui, acts, BranchRef::local(&b.name));
    }
}

/// The badge a branch's row shows for the first of `reasons` that has one: Merged, else Stale.
fn badge(reasons: &[Reason]) -> Option<Badge> {
    if reasons.iter().any(Reason::is_merged) {
        Some(Badge::Merged)
    } else if reasons.iter().any(|r| matches!(r, Reason::Stale(_))) {
        Some(Badge::Stale)
    } else {
        None
    }
}

/// `Merged into main` from `merged into main`.
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

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

/// A branch, `full` its whole ref: icon (a check for the checked-out one), name, ahead/behind
/// counts; and what clicking its build status asked for.
fn branch_row(
    ui: &mut Ui,
    full: &str,
    row: LeafRow<'_>,
    is_head: bool,
    ahead: u32,
    behind: u32,
    marks: Marks,
) -> (Response, Option<Action>) {
    let Marks {
        gone,
        badge,
        pinned,
        in_worktree,
        has_pr,
        ci,
        hidden,
    } = marks;
    let t = Tokens::get(ui.ctx());
    // Known by the branch, not its place: a fetch or a branch made in the terminal shifts the
    // rows, and a menu left open (Merge, Rebase, Check out…) must stay this branch's.
    let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("branch-row", full));
    widgets::a11y(&resp, egui::WidgetType::Button, || {
        let branch = labels::branch(labels::ref_name(full), is_head, ahead as _, behind as _);
        let notes = [
            (gone, "upstream gone"),
            (in_worktree, "checked out in another worktree"),
            (has_pr, "has a pull request"),
            (hidden, "hidden from the graph"),
        ];
        labels::with_notes(&branch, &notes)
    });
    let painter = ui.painter_at(rect);
    if is_head {
        painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.14));
    }
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    let icon_rect =
        Rect::from_center_size(pos2(rect.left() + 34.0 + row.indent, y), vec2(16.0, 16.0));
    if is_head {
        icons::paint(ui, icon_rect, "check", 14.0, t.accent);
    } else if pinned {
        icons::paint(ui, icon_rect, "pin", 13.0, t.text_dim);
    } else {
        icons::paint(ui, icon_rect, "git-branch", 13.0, t.text_faint);
    }

    let mut right = rect.right() - 8.0;
    let small = theme::mono(11.0);
    if has_pr {
        let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
        icons::paint(ui, ir, "git-pull-request", 12.0, t.success);
        right = ir.left() - 6.0;
    }
    if in_worktree {
        let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
        icons::paint(ui, ir, "folder-tree", 12.0, t.text_dim);
        right = ir.left() - 6.0;
    }
    if gone {
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            "gone",
            theme::medium(10.5),
            t.text_faint,
        );
        right = r.left() - 6.0;
    }
    if let Some(badge) = badge {
        let (word, color) = match badge {
            Badge::Merged => ("merged", t.success),
            Badge::Stale => ("stale", t.warning),
        };
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            word,
            theme::medium(10.5),
            color,
        );
        right = r.left() - 6.0;
    }
    let mut ci_act = None;
    if let Some((sha, status)) = ci {
        let center = pos2(right - ci_status::DOT_R, y);
        ci_act = ci_status::dot(ui, resp.id.with("ci"), center, sha, status);
        right = center.x - ci_status::DOT_R - 6.0;
    }
    for (n, icon) in [(behind, "arrow-down"), (ahead, "arrow-up")] {
        if n > 0 {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                n.to_string(),
                small.clone(),
                t.text_dim,
            );
            let ir = Rect::from_center_size(pos2(r.left() - 7.0, y), vec2(12.0, 12.0));
            icons::paint(ui, ir, icon, 11.0, t.text_dim);
            right = ir.left() - 6.0;
        }
    }
    let font = if is_head {
        theme::semibold(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    painter
        .with_clip_rect(Rect::from_min_max(
            pos2(rect.left(), rect.top()),
            pos2(right, rect.bottom()),
        ))
        .text(
            pos2(rect.left() + 48.0 + row.indent, y),
            Align2::LEFT_CENTER,
            row.name,
            font,
            if hidden { t.text_faint } else { t.text },
        );
    (resp, ci_act)
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
