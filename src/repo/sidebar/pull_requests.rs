//! The pull (or merge) requests of the repository's forge.

use std::path::{Path, PathBuf};

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use super::remote_short;
use crate::forge::{Forge, PullRequest};
use crate::git::Branch;
use crate::icons;
use crate::repo::pinned_sections::{Header, Sections};
use crate::repo::{Action, RepoTab, ci_status, widgets};
use crate::theme::Tokens;

impl RepoTab {
    pub(super) fn pr_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        sections: &mut Sections,
    ) {
        let Some(forge) = self.forge else { return };
        let t = Tokens::get(ui.ctx());
        let title = match forge {
            Forge::GitHub => "PULL REQUESTS",
            Forge::GitLab => "MERGE REQUESTS",
        };
        let open = sections.add(
            ui,
            Header {
                key: "prs",
                icon: "git-pull-request",
                title,
                count: self.prs.len(),
                default_open: true,
                button: None,
            },
        );
        if !open {
            return;
        }
        if let Some(err) = &self.pr_error {
            let (rect, r) =
                ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            ui.painter_at(rect).text(
                pos2(rect.left() + 34.0, rect.center().y),
                Align2::LEFT_CENTER,
                err.lines().next().unwrap_or_default(),
                egui::FontId::proportional(11.0),
                t.text_faint,
            );
            r.on_hover_text(err.as_str());
            return;
        }
        for pr in &self.prs {
            self.pr_row(ui, acts, forge, pr, idle);
        }
    }

    fn pr_row(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        forge: Forge,
        pr: &PullRequest,
        idle: bool,
    ) {
        let t = Tokens::get(ui.ctx());
        // Known by its number, not its place: the list reloads (and reorders) on its own, and a
        // menu left open (Check out) must stay this one's.
        let (rect, r) = widgets::keyed_row(ui, 36.0, ("pr-row", pr.number));
        let shown = self.pr_view.as_ref().is_some_and(|v| v.number == pr.number);
        if shown {
            ui.painter().rect_filled(rect, 0.0, t.row_selected);
        } else if r.hovered() {
            ui.painter().rect_filled(rect, 0.0, t.hover);
        }
        let y = rect.center().y;
        icons::paint(
            ui,
            Rect::from_center_size(pos2(rect.left() + 34.0, y), vec2(16.0, 16.0)),
            "git-pull-request",
            13.0,
            if pr.draft { t.text_faint } else { t.success },
        );
        let ci = self.ci_status(&pr.sha);
        // The build status on the right, and the texts up to it.
        let text_right = if ci.is_some() {
            rect.right() - 24.0
        } else {
            rect.right()
        };
        let p = ui.painter_at(rect).with_clip_rect(Rect::from_min_max(
            rect.min,
            pos2(text_right, rect.bottom()),
        ));
        p.text(
            pos2(rect.left() + 48.0, y - 7.0),
            Align2::LEFT_CENTER,
            format!("#{} {}", pr.number, pr.title),
            egui::FontId::proportional(12.5),
            t.text,
        );
        let sub = format!(
            "{} · {}{}",
            pr.branch,
            pr.author,
            if pr.draft { " · draft" } else { "" }
        );
        p.text(
            pos2(rect.left() + 48.0, y + 8.0),
            Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
        if let Some(status) = ci {
            let center = pos2(rect.right() - 14.0, y);
            acts.extend(ci_status::dot(ui, r.id.with("ci"), center, &pr.sha, status));
        }
        let has_view = forge == Forge::GitHub;
        let r = r.on_hover_text(if has_view {
            format!("{}\nClick to open", pr.url)
        } else {
            format!("{}\nDouble-click to open in the browser", pr.url)
        });
        if has_view {
            // A double-click opens it as a click does: its first click already did.
            if r.clicked() && !r.double_clicked() {
                acts.push(Action::OpenPr(pr.number));
            }
        } else if r.double_clicked() {
            acts.push(Action::OpenUrl(pr.url.clone()));
        } else if r.clicked()
            && let Some(target) = self.pr_target(pr)
        {
            acts.push(Action::SelectHash(target));
        }
        egui::Popup::context_menu(&r)
            .style(crate::theme::menu_style)
            .show(|ui| {
                if has_view && ui.button("Open").clicked() {
                    acts.push(Action::OpenPr(pr.number));
                }
                if ui.button("Open in the browser").clicked() {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                }
                self.pr_worktree_button(ui, acts, pr.number, &pr.branch, &pr.sha, idle, true);
                if ui
                    .add_enabled(idle, egui::Button::new(format!("Check out {}", pr.branch)))
                    .clicked()
                {
                    if self.refs.local.iter().any(|b| b.name == pr.branch) {
                        acts.push(Action::Checkout(pr.branch.clone()));
                    } else if let Some(b) = self.pr_remote_branch(pr) {
                        acts.push(Action::CheckoutRemote(b.name.clone()));
                    } else {
                        let remote = self.default_remote().unwrap_or_else(|| "origin".to_owned());
                        acts.push(Action::CheckoutRemote(format!("{remote}/{}", pr.branch)));
                    }
                }
                if ui.button("Copy URL").clicked() {
                    ui.ctx().copy_text(pr.url.clone());
                }
            });
    }

    /// The tip of `pr`'s branch, local or on a remote.
    fn pr_target(&self, pr: &PullRequest) -> Option<String> {
        self.refs
            .local
            .iter()
            .find(|b| b.name == pr.branch)
            .or_else(|| self.pr_remote_branch(pr))
            .map(|b| b.target.clone())
    }

    /// The remote-tracking branch `pr` proposes, which can be on any remote (a fork's): the
    /// one at the pull request's latest commit, else the default remote's, else the first.
    fn pr_remote_branch(&self, pr: &PullRequest) -> Option<&Branch> {
        self.remote_branch_of(&pr.branch, &pr.sha)
    }

    /// The remote-tracking branch named `branch` on any remote, at `sha` if one is.
    fn remote_branch_of(&self, branch: &str, sha: &str) -> Option<&Branch> {
        let default = self.default_remote();
        let found: Vec<(&str, &Branch)> = self
            .remotes
            .iter()
            .flat_map(|r| {
                self.remote_branches(&r.name)
                    .filter(|b| remote_short(&b.name, &r.name) == Some(branch))
                    .map(|b| (r.name.as_str(), b))
            })
            .collect();
        found
            .iter()
            .find(|(_, b)| !sha.is_empty() && b.target == sha)
            .or_else(|| found.iter().find(|(r, _)| default.as_deref() == Some(*r)))
            .or_else(|| found.first())
            .map(|(_, b)| *b)
    }

    /// The button (`in_menu`: the menu item) that opens the worktree of pull request `number`
    /// (proposing `branch`, at `sha`), making it first when there is none.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::repo) fn pr_worktree_button(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        number: u64,
        branch: &str,
        sha: &str,
        idle: bool,
        in_menu: bool,
    ) {
        let plan = self.pr_worktree_plan(number, branch, sha);
        let (enabled, tip) = match &plan {
            PrWorktree::Here => (false, "Its branch is checked out here".to_owned()),
            PrWorktree::Open(path) => (true, format!("Opens {}", path.display())),
            PrWorktree::Create { path, .. } => (
                idle,
                format!("Makes a worktree in {} and opens it", path.display()),
            ),
        };
        let button = if in_menu {
            egui::Button::new("Open in worktree")
        } else {
            let t = Tokens::get(ui.ctx());
            egui::Button::image_and_text(
                icons::image("folder-git-2", 14.0, t.icon),
                "Open in worktree",
            )
            .min_size(vec2(0.0, 26.0))
        };
        let r = ui
            .add_enabled(enabled, button)
            .on_hover_text(&tip)
            .on_disabled_hover_text(tip);
        if r.clicked() {
            acts.push(Action::OpenPrWorktree {
                number,
                branch: branch.to_owned(),
                sha: sha.to_owned(),
            });
        }
    }

    /// Opens the worktree of pull request `number` (proposing `branch`, at `sha`), making it
    /// first when there is none.
    pub(in crate::repo) fn open_pr_worktree(
        &mut self,
        number: u64,
        branch: &str,
        sha: &str,
        ctx: &egui::Context,
    ) {
        match self.pr_worktree_plan(number, branch, sha) {
            PrWorktree::Here => {
                self.status = Some((format!("#{number}'s branch is checked out here"), false));
            }
            PrWorktree::Open(path) => {
                self.open_request = Some(crate::repo::OpenRequest::Worktree(path));
            }
            PrWorktree::Create { ops, path } => {
                if self.busy_op.is_some() {
                    return;
                }
                let label = format!("Open #{number} in a worktree");
                self.open_when_done = Some((label.clone(), path));
                self.run_ops(
                    &label,
                    ops,
                    crate::repo::Refresh::All,
                    crate::repo::OpKind::plain(),
                    ctx,
                );
            }
        }
    }

    /// How to get to the worktree of pull request `number`, proposing `branch` at `sha`: its
    /// branch where it is checked out, or a new worktree on it. The branch is the local one of
    /// that name, made from the remote branch when there is only that. A pull request from a
    /// fork that isn't a remote here is fetched from the forge into a branch of its own (named
    /// `pr-N` when one of the name is already another).
    fn pr_worktree_plan(&self, number: u64, branch: &str, sha: &str) -> PrWorktree {
        let local = self.refs.local.iter().find(|b| b.name == branch);
        let remote = self.remote_branch_of(branch, sha);
        let mut fetch = None;
        let name = if remote.is_some() || local.is_some_and(|b| sha.is_empty() || b.target == sha) {
            branch.to_owned()
        } else {
            let name = if local.is_some() {
                format!("pr-{number}")
            } else {
                branch.to_owned()
            };
            let pull_ref = match self.forge {
                Some(Forge::GitLab) => format!("refs/merge-requests/{number}/head"),
                _ => format!("refs/pull/{number}/head"),
            };
            let from = self.default_remote().unwrap_or_else(|| "origin".to_owned());
            fetch = Some(vec![
                "fetch".to_owned(),
                from,
                format!("+{pull_ref}:refs/heads/{name}"),
            ]);
            name
        };
        if let Some(wt) = self
            .worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some(name.as_str()))
        {
            return if wt.current {
                PrWorktree::Here
            } else {
                PrWorktree::Open(wt.path.clone())
            };
        }
        let path = free_folder(&self.worktree_folder(&name));
        let path_arg = path.display().to_string();
        let add: Vec<String> = match (fetch.is_some(), local.is_some(), remote) {
            (false, false, Some(remote)) => vec![
                "worktree".into(),
                "add".into(),
                "--track".into(),
                "-b".into(),
                name.clone(),
                path_arg,
                remote.name.clone(),
            ],
            _ => vec!["worktree".into(), "add".into(), path_arg, name.clone()],
        };
        PrWorktree::Create {
            ops: fetch.into_iter().chain([add]).collect(),
            path,
        }
    }

    /// The folder a worktree on `branch` goes in, beside the main one: `app` + `feature/x` →
    /// `app-feature-x`, as the dialog that adds one suggests.
    fn worktree_folder(&self, branch: &str) -> PathBuf {
        let main = self.main_worktree().unwrap_or(&self.path);
        let repo_name = main
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "worktree".to_owned());
        let parent = main.parent().unwrap_or(main);
        parent.join(format!(
            "{repo_name}-{}",
            branch.replace(['/', '\\', ' '], "-")
        ))
    }
}

/// How to get to a pull request's worktree.
#[derive(Debug, PartialEq, Eq)]
enum PrWorktree {
    /// Its branch is checked out in the worktree shown.
    Here,
    Open(PathBuf),
    /// Run `ops` (which add the worktree), then open `path`.
    Create {
        ops: Vec<Vec<String>>,
        path: PathBuf,
    },
}

/// `path`, or `path-2`, `path-3`… when there is something there already.
fn free_folder(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (2..1000)
        .map(|i| path.with_file_name(format!("{name}-{i}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{self, Remote};

    fn branch(name: &str, target: &str) -> Branch {
        Branch {
            name: name.to_owned(),
            target: target.to_owned(),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        }
    }

    fn pr(branch: &str, sha: &str) -> PullRequest {
        PullRequest {
            number: 1,
            title: String::new(),
            branch: branch.to_owned(),
            sha: sha.to_owned(),
            author: String::new(),
            url: String::new(),
            draft: false,
        }
    }

    #[test]
    fn a_pull_request_branch_is_found_on_the_remote_that_has_it() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-remote-test".into(), &ctx);
        tab.remotes = ["origin", "fork"]
            .map(|name| Remote {
                name: name.to_owned(),
                url: String::new(),
            })
            .into();
        tab.refs.remotes = vec![
            ("origin".to_owned(), vec![branch("origin/main", "m")]),
            (
                "fork".to_owned(),
                vec![branch("fork/feat", "f"), branch("fork/main", "fm")],
            ),
        ];
        let name = |pr: &PullRequest| tab.pr_remote_branch(pr).map(|b| b.name.clone());
        // Only on the fork: not `origin/feat`, which does not exist.
        assert_eq!(name(&pr("feat", "f")).as_deref(), Some("fork/feat"));
        assert_eq!(name(&pr("feat", "")).as_deref(), Some("fork/feat"));
        // On both: the one at the pull request's commit, else the default remote's.
        assert_eq!(name(&pr("main", "fm")).as_deref(), Some("fork/main"));
        assert_eq!(name(&pr("main", "x")).as_deref(), Some("origin/main"));
        assert_eq!(name(&pr("gone", "x")), None);
    }

    fn worktree(path: &str, branch: &str, current: bool) -> git::Worktree {
        git::Worktree {
            path: path.into(),
            branch: Some(branch.to_owned()),
            current,
            main: path == "/r/app",
            ..git::Worktree::default()
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn a_pull_request_opens_in_its_worktree_made_when_needed() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/r/app".into(), &ctx);
        tab.forge = Some(Forge::GitHub);
        tab.remotes = vec![Remote {
            name: "origin".to_owned(),
            url: String::new(),
        }];
        tab.refs.local = vec![
            branch("main", "m"),
            branch("mine", "x"),
            branch("feat", "f"),
        ];
        tab.refs.remotes = vec![(
            "origin".to_owned(),
            vec![branch("origin/main", "m"), branch("origin/feat", "f2")],
        )];
        tab.worktrees = vec![
            worktree("/nonexistent/r/app", "main", true),
            worktree("/nonexistent/r/app-mine", "mine", false),
        ];
        let folder = |name: &str| PathBuf::from(format!("/nonexistent/r/app-{name}"));
        // Checked out here, or in another worktree.
        assert_eq!(tab.pr_worktree_plan(1, "main", "m"), PrWorktree::Here);
        assert_eq!(
            tab.pr_worktree_plan(2, "mine", "x"),
            PrWorktree::Open(folder("mine"))
        );
        // A local branch: a worktree on it.
        assert_eq!(
            tab.pr_worktree_plan(3, "feat", "f2"),
            PrWorktree::Create {
                ops: vec![strings(&[
                    "worktree",
                    "add",
                    "/nonexistent/r/app-feat",
                    "feat"
                ])],
                path: folder("feat"),
            }
        );
        // Only on the remote: a local branch tracking it, made with the worktree.
        tab.refs.remotes[0].1.push(branch("origin/fix/x", "fx"));
        assert_eq!(
            tab.pr_worktree_plan(4, "fix/x", "fx"),
            PrWorktree::Create {
                ops: vec![strings(&[
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    "fix/x",
                    "/nonexistent/r/app-fix-x",
                    "origin/fix/x",
                ])],
                path: folder("fix-x"),
            }
        );
        // From a fork that isn't a remote: fetched from the pull request's ref, into a branch of
        // its own when a local one has its name.
        assert_eq!(
            tab.pr_worktree_plan(5, "topic", "t"),
            PrWorktree::Create {
                ops: vec![
                    strings(&["fetch", "origin", "+refs/pull/5/head:refs/heads/topic"]),
                    strings(&["worktree", "add", "/nonexistent/r/app-topic", "topic"]),
                ],
                path: folder("topic"),
            }
        );
        tab.refs.remotes[0].1.retain(|b| b.name != "origin/main");
        assert_eq!(
            tab.pr_worktree_plan(6, "main", "fork-main"),
            PrWorktree::Create {
                ops: vec![
                    strings(&["fetch", "origin", "+refs/pull/6/head:refs/heads/pr-6"]),
                    strings(&["worktree", "add", "/nonexistent/r/app-pr-6", "pr-6"]),
                ],
                path: folder("pr-6"),
            }
        );
        tab.forge = Some(Forge::GitLab);
        assert!(matches!(
            tab.pr_worktree_plan(7, "topic", "t"),
            PrWorktree::Create { ops, .. } if ops[0][2] == "+refs/merge-requests/7/head:refs/heads/topic"
        ));
    }
}
