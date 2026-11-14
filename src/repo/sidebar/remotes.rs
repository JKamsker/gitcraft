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
                title: crate::i18n::text("repo-core-3480"),
                count: self.remotes.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: crate::i18n::text("repo-core-3481"),
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
                    &crate::i18n::message(
                        "repo-core-3484",
                        &[("name", remote.name.to_string().into())],
                    ),
                    &[
                        (
                            primary,
                            crate::i18n::text("dev-integration-ui-primary-note"),
                        ),
                        (
                            hidden,
                            crate::i18n::text("dev-integration-ui-sidebar-hidden"),
                        ),
                        (
                            self.visibility.graph_remotes.contains(&remote.name),
                            crate::i18n::text("dev-integration-ui-graph-hidden"),
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
            let r = r.on_hover_text(crate::i18n::message(
                "dev-integration-ui-remote-tip",
                &[
                    ("name", remote.name.clone().into()),
                    ("url", remote.url.clone().into()),
                    (
                        "primary",
                        if primary {
                            format!("\n{}", crate::i18n::text("dev-integration-ui-primary-tip"))
                        } else {
                            String::new()
                        }
                        .into(),
                    ),
                    (
                        "hidden",
                        if hidden {
                            format!(
                                "\n{}",
                                crate::i18n::text("dev-integration-ui-sidebar-hidden-tip")
                            )
                        } else {
                            String::new()
                        }
                        .into(),
                    ),
                ],
            ));
            crate::menu_item::context(&r)
                .style(crate::theme::menu_style)
                .show(|ui| self.remote_menu_items(ui, acts, remote, idle));
            let menu_rect = Rect::from_center_size(
                pos2(rect.right() - 43.0, rect.center().y),
                vec2(22.0, ROW_H),
            );
            let menu = ui.interact(menu_rect, r.id.with("actions"), egui::Sense::click());
            widgets::a11y(&menu, egui::WidgetType::Button, || {
                crate::i18n::message(
                    "dev-integration-ui-remote-actions",
                    &[("name", remote.name.clone().into())],
                )
            });
            icons::paint(
                ui,
                menu_rect.shrink(4.0),
                "settings",
                13.0,
                Tokens::get(ui.ctx()).text_faint,
            );
            let menu = menu.on_hover_text(crate::i18n::message(
                "dev-integration-ui-remote-actions-tip",
                &[("name", remote.name.clone().into())],
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
        let r = r.on_hover_text(crate::i18n::message(
            "repo-core-3485",
            &[("name", b.name.to_string().into())],
        ));
        if r.double_clicked() && idle {
            acts.push(Action::CheckoutRemote(b.name.clone()));
        } else if r.clicked() {
            acts.push(Action::SelectBranch(b.target.clone()));
        }
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| self.remote_branch_menu_items(ui, acts, remote, short, b, idle));
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
            crate::i18n::message(
                "dev-integration-ui-remote-hidden-caption",
                &[("name", remote.name.clone().into())],
            )
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

/// A remote's branch, `full` its whole ref: only the icon and the name, in from its remote; the
/// name dimmed while the graph leaves the branch out.
fn remote_row(ui: &mut Ui, full: &str, row: LeafRow<'_>, has_pr: bool, hidden: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    // Known by the branch, not its place: a fetch adds and prunes rows under an open menu.
    let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("branch-row", full));
    widgets::a11y(&resp, egui::WidgetType::Button, || {
        let name = crate::i18n::message(
            "repo-core-2595",
            &[("git-ref", labels::ref_name(full).to_string().into())],
        );
        let notes = [
            (has_pr, crate::i18n::text("repo-core-3418")),
            (hidden, crate::i18n::text("repo-core-3419")),
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

mod menu;
