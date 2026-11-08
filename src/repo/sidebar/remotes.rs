//! The remotes and their branches.

use crate::repo::ghost::{GhostOp, offer};
use egui::{Align2, Rect, Response, Ui, pos2, vec2};

use super::{REMOTE_INDENT, ROW_H, matches, visibility};
use crate::a11y::labels;
use crate::git::{Branch, Remote};
use crate::icons;
use crate::repo::actions::BranchRef;
use crate::repo::branch_tree::{self, LeafRow, TreeView};
use crate::repo::branch_visibility::{Change, remote_ref};
use crate::repo::conflict_check::Op;
use crate::repo::graph_view;
use crate::repo::pinned_sections::{Button, Header, Sections};
use crate::repo::{
    Action, Dialog, Refresh, RepoTab, fast_forward_action, merge_action, ops, rebase_action,
    widgets,
};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// The remote-tracking branches of `remote`. Told apart by name, not by the first part of
    /// it: a remote can have a slash in its name (`my/fork`).
    pub(in crate::repo) fn remote_branches<'a>(
        &'a self,
        remote: &'a str,
    ) -> impl Iterator<Item = &'a Branch> + 'a {
        let names: Vec<&str> = self.remotes.iter().map(|r| r.name.as_str()).collect();
        self.refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list)
            .filter(move |b| remote_of(&b.name, &names) == Some(remote))
    }

    pub(super) fn remotes_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        filter: &str,
        sections: &mut Sections,
    ) {
        let open = sections.add(
            ui,
            Header {
                key: "remotes",
                icon: "cloud",
                title: "REMOTES",
                count: self.remotes.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: "Add a remote…",
                    action: idle.then(|| Action::Dialog(Dialog::remote(None))),
                }),
            },
        );
        if !open {
            return;
        }
        for remote in &self.remotes {
            let hidden = self.visibility.sidebar_remotes.contains(&remote.name);
            let primary = self.primary_forge_remote() == Some(remote.name.as_str());
            let branches: Vec<&Branch> = self
                .remote_branches(&remote.name)
                .filter(|b| matches(filter, b))
                .collect();
            // A collapsible row per remote, open or closed in this repository only.
            let id = egui::Id::new(("remote-open", self.repo_key(), &remote.name));
            let mut expanded = ui.data_mut(|d| d.get_persisted::<bool>(id)).unwrap_or(true);
            // By the remote: its menu's Fetch must not move to another one.
            let (rect, r) = widgets::keyed_row(ui, ROW_H, ("remote-row", &remote.name));
            widgets::a11y(&r, egui::WidgetType::CollapsingHeader, || {
                let count = labels::count(branches.len(), "branch", "branches");
                let name = labels::with_notes(
                    &format!("remote {}", remote.name),
                    &[
                        (primary, "primary forge"),
                        (hidden, "branches hidden in sidebar"),
                        (
                            self.visibility.graph_remotes.contains(&remote.name),
                            "branches hidden in graph",
                        ),
                    ],
                );
                labels::section(&name, &count, expanded && !hidden)
            });
            if r.clicked() {
                expanded = !expanded;
                ui.data_mut(|d| d.insert_persisted(id, expanded));
            }
            paint_remote_header(
                ui,
                rect,
                &r,
                remote,
                branches.len(),
                RemoteDisplay {
                    expanded: expanded && !hidden,
                    primary,
                    hidden,
                },
            );
            let r = r.on_hover_text(format!(
                "{}\n{}{}{}\nRight-click for primary forge and branch visibility",
                remote.name,
                remote.url,
                if primary {
                    "\nPrimary forge: PRs, issues and builds"
                } else {
                    ""
                },
                if hidden {
                    "\nBranches hidden in sidebar; right-click to show"
                } else {
                    ""
                }
            ));
            egui::Popup::context_menu(&r)
                .style(crate::theme::menu_style)
                .show(|ui| self.remote_menu_items(ui, acts, remote, idle));
            let menu_rect = Rect::from_center_size(
                pos2(rect.right() - 43.0, rect.center().y),
                vec2(22.0, ROW_H),
            );
            let menu = ui.interact(menu_rect, r.id.with("actions"), egui::Sense::click());
            widgets::a11y(&menu, egui::WidgetType::Button, || {
                format!("Remote actions for {}", remote.name)
            });
            icons::paint(
                ui,
                menu_rect.shrink(4.0),
                "settings",
                13.0,
                Tokens::get(ui.ctx()).text_faint,
            );
            let menu = menu.on_hover_text(format!(
                "Actions for {}: primary forge and branch visibility",
                remote.name
            ));
            egui::Popup::menu(&menu)
                .style(crate::theme::menu_style)
                .show(|ui| self.remote_menu_items(ui, acts, remote, idle));
            if !expanded || hidden {
                continue;
            }
            let tree = branch_tree::build(branches.iter().map(|b| {
                let short = remote_short(&b.name, &remote.name).unwrap_or(&b.name);
                (short, *b)
            }));
            let view = TreeView::branches(
                egui::Id::new(("branch-folder", self.repo_key(), "remote", &remote.name)),
                !filter.is_empty(),
                REMOTE_INDENT,
            );
            branch_tree::show(
                ui,
                &tree,
                &view,
                &|_: &&Branch| false,
                &|b: &&Branch| ("branch-row", remote_ref(&b.name)),
                &mut |ui, row, b: &&Branch| {
                    self.remote_branch_row(ui, acts, &remote.name, b, row, idle);
                },
            );
        }
    }

    fn remote_branch_row(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        remote: &str,
        b: &Branch,
        row: LeafRow<'_>,
        idle: bool,
    ) {
        let short = remote_short(&b.name, remote).unwrap_or(&b.name);
        let has_pr = self.primary_forge_remote() == Some(remote)
            && self.prs.iter().any(|p| !p.from_fork && p.branch == short);
        let full = remote_ref(&b.name);
        let shown = self.branch_shown(&full, false);
        let r = remote_row(ui, &full, row, has_pr, !shown);
        let r = self.branch_drag_drop(ui, r, &b.name, true, &b.target, acts);
        acts.extend(visibility::eye(ui, &r, &full, shown));
        let r = r.on_hover_text(format!(
            "{}\nDouble-click to check out, or drag onto a branch or a commit",
            b.name
        ));
        if r.double_clicked() && idle {
            acts.push(Action::CheckoutRemote(b.name.clone()));
        } else if r.clicked() {
            acts.push(Action::SelectHash(b.target.clone()));
        }
        egui::Popup::context_menu(&r)
            .style(crate::theme::menu_style)
            .show(|ui| self.remote_branch_menu_items(ui, acts, remote, short, b, idle));
    }

    fn remote_branch_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        remote: &str,
        short: &str,
        b: &Branch,
        idle: bool,
    ) {
        if ui
            .add_enabled(idle, egui::Button::new("Check out as local branch"))
            .clicked()
        {
            acts.push(Action::CheckoutRemote(b.name.clone()));
        }
        if let Some(current) = self.refs.current.as_deref() {
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(format!("Fast-forward {current} to it")),
                )
                .clicked()
            {
                acts.push(fast_forward_action(BranchRef::remote(&b.name), current));
            }
            let merge = format!("Merge into {current}");
            if self
                .conflict_button(ui, merge, idle, Op::Merge, &b.target)
                .clicked()
            {
                acts.push(merge_action(BranchRef::remote(&b.name), current));
            }
            let rebase = format!("Rebase {current} onto it");
            let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, &b.target);
            offer(&rebase, || GhostOp::Rebase {
                onto: BranchRef::remote(&b.name).full(),
                label: b.name.clone(),
            });
            if rebase.clicked() {
                acts.push(rebase_action(BranchRef::remote(&b.name), current));
            }
            crate::repo::rehearse::rehearse_item(ui, acts, BranchRef::remote(&b.name));
            ui.separator();
            graph_view::compare_items(ui, acts, BranchRef::remote(&b.name), current);
            ui.separator();
        }
        if ui.button("Create branch here…").clicked() {
            acts.push(Action::Dialog(Dialog::create_branch(&b.target, &b.name)));
        }
        ui.separator();
        if ui.button("Copy name").clicked() {
            ui.ctx().copy_text(b.name.clone());
        }
        if let Some((menu, target)) = self.branch_link_menu(&b.name) {
            menu.items(ui, "branch", &target, acts);
        }
        ui.separator();
        self.visibility_items(ui, acts, &remote_ref(&b.name), false);
        ui.separator();
        if ui
            .add_enabled(idle, egui::Button::new("Delete from remote…"))
            .clicked()
        {
            acts.push(Action::Dialog(
                Dialog::confirm(
                    "Delete remote branch",
                    format!(
                        "Delete “{short}” from {remote}? This changes the remote for everyone.",
                    ),
                    "Delete",
                    // Only while it is where it was last fetched: commits pushed since are others'.
                    ops(&[&[
                        "push",
                        &format!("--force-with-lease=refs/heads/{short}:{}", b.target),
                        remote,
                        "--delete",
                        &format!("refs/heads/{short}"),
                    ]]),
                    None,
                )
                .preview_of(
                    self.delete_preview(&[&b.target], vec![format!("refs/remotes/{}", b.name)]),
                ),
            ));
        }
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Branch,
            self.branch_scope(&b.name, &b.target, Some(remote)),
            acts,
        );
    }
}

/// The name of `remote`'s branch `name` on the remote: `main` for `origin/main`. git writes
/// `remotes/origin/main` when a local branch is called `origin/main` too.
pub(in crate::repo) fn remote_short<'a>(name: &'a str, remote: &str) -> Option<&'a str> {
    let under = |n: &'a str| n.strip_prefix(remote)?.strip_prefix('/');
    under(name).or_else(|| under(name.strip_prefix("remotes/")?))
}

/// Which of `remotes` the remote-tracking branch `name` is of: the longest that fits, so that
/// `my/fork/main` is `my/fork`'s even when there is a remote `my`.
pub(in crate::repo) fn remote_of<'r>(name: &str, remotes: &[&'r str]) -> Option<&'r str> {
    remotes
        .iter()
        .copied()
        .filter(|r| remote_short(name, r).is_some())
        .max_by_key(|r| r.len())
}

/// The collapsible row of a remote at `rect`: chevron, icon, name and how many branches it
/// shows.
struct RemoteDisplay {
    expanded: bool,
    primary: bool,
    hidden: bool,
}

fn paint_remote_header(
    ui: &Ui,
    rect: Rect,
    r: &Response,
    remote: &Remote,
    count: usize,
    display: RemoteDisplay,
) {
    let RemoteDisplay {
        expanded,
        primary,
        hidden,
    } = display;
    let t = Tokens::get(ui.ctx());
    if r.hovered() {
        ui.painter().rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 24.0, y), vec2(12.0, 12.0)),
        if expanded {
            "chevron-down"
        } else {
            "chevron-right"
        },
        11.0,
        t.text_faint,
    );
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 40.0, y), vec2(14.0, 14.0)),
        if primary {
            "star"
        } else if hidden {
            "eye-off"
        } else {
            "cloud"
        },
        13.0,
        if primary { t.accent } else { t.icon },
    );
    let p = ui.painter_at(rect.with_max_x(rect.right() - 60.0));
    p.text(
        pos2(rect.left() + 54.0, y),
        Align2::LEFT_CENTER,
        if hidden {
            format!("{} (hidden)", remote.name)
        } else {
            remote.name.clone()
        },
        theme::medium(12.5),
        t.text,
    );
    ui.painter_at(rect).text(
        pos2(rect.right() - 10.0, y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        theme::medium(11.0),
        t.text_faint,
    );
}

impl RepoTab {
    fn remote_menu_items(&self, ui: &mut Ui, acts: &mut Vec<Action>, remote: &Remote, idle: bool) {
        let before = acts.len();
        // Popup placement can move a menu, but cannot shrink an unbroken menu label.
        // Wrap within the logical viewport at high UI scales and on narrow windows.
        ui.set_max_width((ui.ctx().content_rect().width() - 16.0).clamp(32.0, 280.0));
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let primary = self.primary_forge_remote() == Some(remote.name.as_str());
        let usable = self
            .forge_remotes
            .iter()
            .any(|(name, _)| name == &remote.name);
        let label = if primary {
            "Primary forge ✓"
        } else {
            "Set as primary forge"
        };
        let tip = if usable {
            "Use this repository for PRs, issues, builds and default links. Does not change Git push/fetch settings."
        } else {
            "This URL is not on a recognized forge. Configure gitcraft.forge for a self-hosted forge."
        };
        if ui
            .add_enabled(
                usable && !self.forge_change_blocked(),
                egui::Button::new(label),
            )
            .on_hover_text(tip)
            .on_disabled_hover_text(if self.forge_change_blocked() {
                "Wait for the running operation or review to finish"
            } else {
                tip
            })
            .clicked()
        {
            acts.push(Action::PrimaryForge(Some(remote.name.clone())));
        }
        if !self.forge_selection.name.is_empty()
            && ui
                .add_enabled(
                    !self.forge_change_blocked(),
                    egui::Button::new("Use automatic forge selection"),
                )
                .clicked()
        {
            acts.push(Action::PrimaryForge(None));
        }
        let sidebar_hidden = self.visibility.sidebar_remotes.contains(&remote.name);
        if ui
            .button(if sidebar_hidden {
                "Show remote branches in sidebar"
            } else {
                "Hide remote branches in sidebar"
            })
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::RemoteSidebar(
                remote.name.clone(),
            )));
        }
        let graph_hidden = self.visibility.graph_remotes.contains(&remote.name);
        let label = if graph_hidden {
            "Show remote branches in graph"
        } else {
            "Hide remote branches in graph"
        };
        if ui.button(label)
            .on_hover_text("Display only. Keeps local HEAD and soloed branches; fetching and upstreams are unchanged.")
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::RemoteGraph(remote.name.clone())));
        }
        ui.separator();
        if ui
            .add_enabled(idle, egui::Button::new(format!("Fetch {}", remote.name)))
            .clicked()
        {
            acts.push(Action::Ops {
                label: format!("Fetch {}", remote.name),
                ops: ops(&[&["fetch", "--prune", &remote.name]]),
                refresh: Refresh::All,
            });
        }
        if ui.add_enabled(idle, egui::Button::new("Edit…")).clicked() {
            acts.push(Action::Dialog(Dialog::remote(Some((
                remote.name.clone(),
                remote.url.clone(),
            )))));
        }
        if ui.button("Copy URL").clicked() {
            ui.ctx().copy_text(remote.url.clone());
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(idle, egui::Button::new("Remove…")).clicked() {
            let confirmation = Dialog::confirm(
                "Remove remote",
                format!(
                    "Remove the remote “{}” ({})? Its branches disappear from this repository; nothing changes on the server.",
                    remote.name, remote.url
                ),
                "Remove",
                ops(&[&["remote", "remove", &remote.name]]),
                None,
            );
            acts.push(Action::Dialog(confirmation));
        }
        // Pointer clicks close a Popup automatically; keyboard activation must close too.
        if acts.len() != before {
            ui.close();
        }
    }
}

/// A remote's branch, `full` its whole ref: only the icon and the name, in from its remote; the
/// name dimmed while the graph leaves the branch out.
fn remote_row(ui: &mut Ui, full: &str, row: LeafRow<'_>, has_pr: bool, hidden: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    // Known by the branch, not its place: a fetch adds and prunes rows under an open menu.
    let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("branch-row", full));
    widgets::a11y(&resp, egui::WidgetType::Button, || {
        let name = format!("remote branch {}", labels::ref_name(full));
        let notes = [
            (has_pr, "has a pull request"),
            (hidden, "hidden from the graph"),
        ];
        labels::with_notes(&name, &notes)
    });
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 34.0 + row.indent, y), vec2(16.0, 16.0)),
        "git-branch",
        13.0,
        t.text_faint,
    );
    let mut right = rect.right() - 8.0;
    if has_pr {
        let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
        icons::paint(ui, ir, "git-pull-request", 12.0, t.success);
        right = ir.left() - 6.0;
    }
    ui.painter_at(rect.with_max_x(right)).text(
        pos2(rect.left() + 48.0 + row.indent, y),
        Align2::LEFT_CENTER,
        row.name,
        egui::FontId::proportional(12.5),
        if hidden { t.text_faint } else { t.text },
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_branch_is_named_without_its_remote() {
        assert_eq!(remote_short("origin/main", "origin"), Some("main"));
        assert_eq!(remote_short("origin/feat/x", "origin"), Some("feat/x"));
        assert_eq!(remote_short("my/fork/main", "my/fork"), Some("main"));
        assert_eq!(remote_short("origin/main", "orig"), None);
        assert_eq!(remote_short("upstream/main", "origin"), None);
    }

    #[test]
    fn an_ambiguous_remote_branch_is_still_found() {
        // `refname:short` when a local branch is called `origin/main` too.
        assert_eq!(remote_short("remotes/origin/main", "origin"), Some("main"));
    }

    #[test]
    fn a_remote_with_a_slash_owns_its_branches() {
        let remotes = ["origin", "my/fork", "my"];
        assert_eq!(remote_of("my/fork/main", &remotes), Some("my/fork"));
        assert_eq!(remote_of("my/other", &remotes), Some("my"));
        assert_eq!(remote_of("origin/my/fork/x", &remotes), Some("origin"));
        assert_eq!(remote_of("gone/main", &remotes), None);
    }
}

#[cfg(test)]
#[path = "remotes_ui_tests.rs"]
mod ui_tests;
