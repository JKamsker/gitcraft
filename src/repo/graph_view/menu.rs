//! The right-click menu of a commit row, and of a branch pill on it.

mod references;

mod rewrite;
use rewrite::*;
pub(in crate::repo) use rewrite::{Rewrite, reset_items};

use egui::{Response, Ui};

use super::badge::BadgeKind;
use super::compare_items;
use crate::forge::links::Target;
use crate::git;
use crate::menu_item::{MenuItem, bounded_menu};
use crate::repo::actions::{BranchRef, cherry_pick_action, interactive_rebase_action};
use crate::repo::commit_actions::revert_action;
use crate::repo::conflict_check::Op;
use crate::repo::ghost::{GhostOp, offer};
use crate::repo::operations::{expect_ref_at, expect_symref};
use crate::repo::{
    Action, Dialog, RepoTab, Undo, WIP_HASH, fast_forward_action, merge_action, ops, rebase_action,
};

/// Where a row remembers which branch pill was right-clicked, for its menu.
pub(super) fn pill_menu_id(hash: &str) -> egui::Id {
    egui::Id::new(("pill_menu", hash))
}

/// The HEAD context-menu entry, kept separate so its actual paint and inert dispatch can
/// be exercised without constructing a repository tab.
fn apply_patch_item(ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2630"))
        .icon("file-diff")
        .enabled(idle)
        .show(ui)
        .on_hover_text(crate::i18n::text(super::APPLY_PATCH_TIP))
        .clicked()
    {
        acts.push(Action::ApplyPatch);
    }
}

impl RepoTab {
    pub(super) fn copy_branch(&self, i: usize) -> Option<String> {
        let commit = self.commits.get(i)?;
        self.graph_badges(commit)
            .into_iter()
            .find(|b| b.kind == BadgeKind::Branch)
            .map(|b| b.git_ref())
    }

    /// Right-click menu of a commit row.
    pub(super) fn commit_menu(
        &self,
        resp: &Response,
        i: usize,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        // The branch whose pill was right-clicked, if any.
        let pill: Option<String> = resp
            .ctx
            .data(|d| d.get_temp::<Option<String>>(pill_menu_id(&commit.hash)))
            .flatten();
        crate::menu_item::context(resp)
            .style(crate::theme::menu_style)
            .show(|ui| self.commit_menu_items(ui, i, pill, idle, acts));
    }

    /// The entries of a commit's menu; `pill` is the branch whose pill was right-clicked, whose
    /// actions go on top.
    pub(super) fn commit_menu_items(
        &self,
        ui: &mut Ui,
        i: usize,
        pill: Option<String>,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        bounded_menu(ui, ("commit-menu", &commit.hash), |ui| {
            self.commit_menu_contents(ui, i, pill, idle, acts)
        });
    }

    fn commit_menu_contents(
        &self,
        ui: &mut Ui,
        i: usize,
        pill: Option<String>,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(commit) = self.commits.get(i) else {
            return;
        };
        let hash = commit.hash.clone();
        let short = hash.get(..8).unwrap_or(&hash).to_owned();
        let summary = commit.summary.clone();
        let is_head = hash == self.refs.head;
        // The right-clicked branch, else the commit's first branch, for copying.
        let first_branch = self.copy_branch(i);
        let branch_name = pill.clone().or_else(|| {
            self.graph_badges(commit)
                .into_iter()
                .find(|b| b.kind == BadgeKind::Branch)
                .map(|b| b.git_ref())
        });
        let target = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let options = crate::settings::Settings::get(ui.ctx()).context_menus;
        if let Some(branch) = &pill {
            crate::menu_item::group_named(
                ui,
                &format!("Branch {branch}"),
                &crate::i18n::message(
                    "dev-integration-ui-branch-group",
                    &[("branch", branch.to_string().into())],
                ),
                |ui| {
                    ui.label(crate::i18n::message(
                        "dev-integration-ui-branch-heading",
                        &[("branch", branch.to_string().into())],
                    ));
                    if self.refs.current.as_deref() != Some(branch.as_str()) {
                        self.pill_items(ui, branch, &hash, idle, acts);
                    } else if let Some(b) = self.refs.local.iter().find(|b| b.name == *branch) {
                        self.branch_menu_items(ui, acts, b, idle);
                    }
                },
            );
            ui.separator();
        }
        crate::menu_item::group_named(
            ui,
            "Branches / Checkout",
            crate::i18n::text("dev-integration-ui-branches-checkout"),
            |ui| {
                self.branch_here_items(ui, &hash, &short, idle, is_head, acts);
            },
        );
        crate::menu_item::group_named(
            ui,
            "Compare",
            crate::i18n::text("dev-integration-ui-compare"),
            |ui| self.compare_with_items(ui, i, &hash, &short, acts),
        );
        let upstream = self
            .refs
            .current
            .as_deref()
            .and_then(|b| self.upstream_ref(b));
        let rewrite = Rewrite {
            tab: self,
            merge: commit.parents.len() > 1,
            hash: &hash,
            short: &short,
            summary: &summary,
            target: &target,
            head: &self.refs.head,
            upstream: upstream.as_deref(),
        };
        crate::menu_item::group_named(
            ui,
            "Rewrite",
            crate::i18n::text("dev-integration-ui-rewrite"),
            |ui| {
                if !idle {
                    ui.label(crate::i18n::text("dev-integration-ui-busy-menu"));
                }
                crate::menu_item::group_named(
                    ui,
                    "Edit / Drop / Reorder",
                    crate::i18n::text("dev-integration-ui-edit-drop-reorder"),
                    |ui| self.quick_edit_items(ui, &hash, idle, acts),
                );
                self.fold_item(ui, &hash, idle, acts);
                rewrite_items(ui, &rewrite, idle, is_head, acts);
                if options.surgery {
                    crate::menu_item::group_named(
                        ui,
                        "Split / Move changes",
                        crate::i18n::text("dev-integration-ui-split-move"),
                        |ui| self.surgery_items(ui, &hash, idle, acts),
                    );
                }
            },
        );
        if options.diagnostics || self.bisect.shows() {
            crate::menu_item::group_named(
                ui,
                "History / Diagnostics",
                crate::i18n::text("dev-integration-ui-history-diagnostics"),
                |ui| {
                    self.bisect_menu_items(ui, &hash, idle, acts);
                    if options.diagnostics {
                        self.parallel_menu_items(ui, &hash, acts);
                        self.test_menu_items(ui, &hash, acts);
                    }
                },
            );
        }
        crate::menu_item::group_named(
            ui,
            "Patches",
            crate::i18n::text("dev-integration-ui-patches"),
            |ui| {
                if MenuItem::new(crate::i18n::text("repo-core-2627"))
                    .icon("file-diff")
                    .refused(
                        (commit.parents.len() > 1).then_some(crate::i18n::text("repo-core-2628")),
                    )
                    .show(ui)
                    .on_hover_text(crate::i18n::text("repo-core-2629"))
                    .clicked()
                {
                    acts.push(Action::SavePatch {
                        hash: hash.clone(),
                        summary: summary.clone(),
                    });
                }
                if is_head {
                    apply_patch_item(ui, idle, acts);
                }
            },
        );
        crate::menu_item::group_named(
            ui,
            "Links / Copy",
            crate::i18n::text("dev-integration-ui-links-copy"),
            |ui| {
                self.link_items(ui, &hash, pill.as_deref(), branch_name.as_deref(), acts);
                self.gerrit_menu(ui, commit, acts);
                copy_items(
                    ui,
                    &hash,
                    &summary,
                    branch_name.as_deref(),
                    self.selected == Some(i),
                    branch_name == first_branch,
                );
            },
        );
        // Custom commands retain the clicked commit/branch identity, even in a submenu.
        let scope = self.commit_scope(&hash, branch_name.as_deref());
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Commit,
            scope,
            acts,
        );
    }

    /// Copy a link to the commit, or to its branch, or open them on the forge; links go to
    /// the remote of the right-clicked branch (`pill`), else of the checked-out one.
    /// Comparing row `i` with the selected row, or with the working tree.
    fn compare_with_items(
        &self,
        ui: &mut Ui,
        i: usize,
        hash: &str,
        short: &str,
        acts: &mut Vec<Action>,
    ) {
        if let Some(compare) = self
            .selected
            .filter(|&s| s != i && self.commits.get(s).is_some_and(|c| c.hash != WIP_HASH))
            .and_then(|s| self.compare_rows(s, i))
        {
            let other = self
                .selected
                .and_then(|s| self.commits.get(s))
                .map(|c| {
                    if c.hash == WIP_HASH {
                        crate::i18n::text("repo-core-0926").to_owned()
                    } else {
                        crate::repo::short(&c.hash).to_owned()
                    }
                })
                .unwrap_or_default();
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-2648",
                &[("other", other.to_string().into())],
            ))
            .icon("file-diff")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2649"))
            .clicked()
            {
                acts.push(compare);
            }
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2650"))
            .icon("file-diff")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Compare {
                base: hash.to_owned(),
                base_label: short.to_owned(),
                target: None,
                target_label: crate::i18n::text("repo-core-2651").to_owned(),
                captions: [None, None],
                since_fork: false,
            });
        }
    }
}

/// The commit a menu rewrites the checked-out branch from.
/// Copying the commit's hash, message, and `branch` when there is one.
fn copy_items(
    ui: &mut Ui,
    hash: &str,
    summary: &str,
    branch: Option<&str>,
    selected: bool,
    first_branch: bool,
) {
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2663"))
        .icon("copy")
        .keys_when(crate::keymap::Command::GraphCopyHash, selected)
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(hash.to_owned());
    }
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2664"))
        .icon("copy")
        .keys_when(crate::keymap::Command::GraphCopyMessage, selected)
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(summary.to_owned());
    }
    if let Some(name) = branch
        && crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2665",
            &[("name", name.to_string().into())],
        ))
        .icon("copy")
        .keys_when(
            crate::keymap::Command::GraphCopyBranch,
            selected && first_branch,
        )
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(name.to_owned());
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "menu_registry_menu_tests.rs"]
mod registry_menu_tests;

#[cfg(test)]
#[path = "menu_apply_patch_tooltip_tests.rs"]
mod apply_patch_tooltip_tests;
