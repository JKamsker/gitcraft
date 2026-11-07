//! The pull (or merge) requests of the repository's forge.

use std::path::{Path, PathBuf};

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use super::remote_short;
use crate::a11y::labels;
use crate::forge::{Forge, PullRequest};
use crate::git::Branch;
use crate::icons;
use crate::menu_item::MenuItem;
use crate::repo::issue_work::IssueAct;
use crate::repo::pinned_sections::{Button, Header, Sections};
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
        let Some(forge) = self.forge() else { return };
        let t = Tokens::get(ui.ctx());
        let title = forge.list_title();
        let open = sections.add(
            ui,
            Header {
                key: "prs",
                icon: "git-pull-request",
                title,
                count: self.prs.len(),
                default_open: true,
                button: Some(Button {
                    icon: "git-branch-plus",
                    tooltip: "Start work from an issue assigned to you…",
                    action: Some(Action::Issue(IssueAct::OpenPicker)),
                }),
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
        widgets::a11y(&r, egui::WidgetType::Button, || {
            labels::pull_request(pr.number, &pr.title, &pr.author, &pr.branch, pr.draft)
        });
        let shown = self.pr_view.as_ref().is_some_and(|v| v.number == pr.number)
            || self.compare.as_ref().is_some_and(|c| {
                c.target_label == pr.branch && c.target.as_deref() == Some(pr.sha.as_str())
            });
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
        let moved = self.pr_moved_since_seen(pr);
        // First, where a long branch name can't cut it off.
        let sub = format!(
            "{}{} · {}{}",
            if moved {
                "changed since you last looked · "
            } else {
                ""
            },
            pr.branch,
            pr.author,
            if pr.draft { " · draft" } else { "" }
        );
        let sub = p.text(
            pos2(rect.left() + 48.0, y + 8.0),
            Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
        let badges = crate::repo::forks::paint_badges(
            self,
            &p,
            pos2(sub.right() + 8.0, y + 8.0),
            pr.number,
            &pr.sha,
        );
        if let Some(status) = ci {
            let center = pos2(rect.right() - 14.0, y);
            acts.extend(ci_status::dot(ui, r.id.with("ci"), center, &pr.sha, status));
        }
        let has_view = forge.has_pages();
        // Without a page here, opening it in the browser is looking at it.
        // Not when it changed since: that would take away what changed before it is compared.
        let seen_on_open = (!has_view && !moved).then_some(Action::Versions(
            crate::repo::versions::VersionsAct::PrSeen(pr.number),
        ));
        let r = r.on_hover_text(if has_view {
            format!(
                "{}\nClick to show its changes; double-click to open details",
                pr.url
            )
        } else {
            format!(
                "{}\nClick to show its changes; double-click to open in the browser",
                pr.url
            )
        });
        let r = match badges {
            Some(why) => r.on_hover_text(why),
            None => r,
        };
        if r.double_clicked() {
            if has_view {
                acts.push(Action::OpenPr(pr.number));
            } else {
                acts.push(Action::OpenUrl(pr.url.clone()));
                acts.extend(seen_on_open);
            }
        } else if r.clicked() {
            acts.push(Action::SelectPr(pr.number));
        }
        egui::Popup::context_menu(&r)
            .style(crate::theme::menu_style)
            .show(|ui| {
                if has_view && ui.button("Open").clicked() {
                    acts.push(Action::OpenPr(pr.number));
                }
                if ui.button("Open in the browser").clicked() {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                    if !has_view && !moved {
                        acts.push(Action::Versions(
                            crate::repo::versions::VersionsAct::PrSeen(pr.number),
                        ));
                    }
                }
                self.pr_since_items(ui, acts, pr);
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
                self.fork_items(ui, acts, pr.number, idle, false);
                if ui.button("Copy URL").clicked() {
                    ui.ctx().copy_text(pr.url.clone());
                }
            });
    }

    /// The tip of `pr`'s branch, local or on a remote.
    pub(in crate::repo) fn pr_target(&self, pr: &PullRequest) -> Option<String> {
        if !pr.sha.is_empty() {
            return Some(pr.sha.clone());
        }
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
            PrWorktree::Open(path) => (true, format!("Opens {}", crate::remote::display(path))),
            PrWorktree::Create { path, .. } => (
                idle,
                format!(
                    "Makes a worktree in {} and opens it",
                    crate::remote::display(path)
                ),
            ),
        };
        let r = if in_menu {
            // Why it can't open, under it; what it would do, on hover.
            let here = matches!(plan, PrWorktree::Here);
            MenuItem::new("Open in worktree")
                .enabled(enabled || here)
                .refused(here.then_some(tip.as_str()))
                .show(ui)
                .on_hover_text(&tip)
                // Greyed out while an operation runs: what it would do, still.
                .on_disabled_hover_text(&tip)
        } else {
            let t = Tokens::get(ui.ctx());
            let button = egui::Button::image_and_text(
                icons::image("folder-git-2", 14.0, t.icon),
                "Open in worktree",
            )
            .min_size(vec2(0.0, 26.0));
            ui.add_enabled(enabled, button)
                .on_hover_text(&tip)
                .on_disabled_hover_text(tip)
        };
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
        // A branch is the pull request's only when it is at its commit (any, when that isn't
        // known): a fork's `main` is not ours.
        let at = |target: &str| sha.is_empty() || target == sha;
        let local_named = |name: &str| self.refs.local.iter().find(|b| b.name == name);
        let own_name = format!("pr-{number}");
        let local = local_named(branch);
        let remote = self.remote_branch_of(branch, sha).filter(|b| at(&b.target));
        // What the forge says is a name only where git takes it as one (never an option).
        let usable = plain_branch_name(branch);
        enum Source<'a> {
            Local(String),
            Track(&'a Branch),
            FromRemote(&'a Branch),
            Fetch,
        }
        let (name, source) = if let Some(b) = local.filter(|b| usable && at(&b.target)) {
            (b.name.clone(), Source::Local(b.name.clone()))
        } else if local_named(&own_name).is_some_and(|b| at(&b.target)) {
            (own_name.clone(), Source::Local(own_name.clone()))
        } else if let Some(remote) = remote.filter(|_| usable && local.is_none()) {
            (branch.to_owned(), Source::Track(remote))
        } else if let Some(remote) = remote {
            (own_name.clone(), Source::FromRemote(remote))
        } else if usable && local.is_none() {
            (branch.to_owned(), Source::Fetch)
        } else {
            (own_name.clone(), Source::Fetch)
        };
        if let Some(wt) = self
            .worktrees
            .iter()
            .filter(|w| w.prunable.is_none())
            .find(|w| w.branch.as_deref() == Some(name.as_str()))
        {
            return if wt.current {
                PrWorktree::Here
            } else {
                PrWorktree::Open(wt.path.clone())
            };
        }
        let path = self.free_folder(&self.worktree_folder(&name));
        // As git of the repository's machine names it.
        let path_arg = crate::remote::git_path(&path);
        let strings =
            |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).collect() };
        // A worktree whose folder was deleted by hand still holds its branch for git.
        let mut ops = vec![strings(&["worktree", "prune"])];
        match source {
            Source::Local(name) => {
                ops.push(strings(&["worktree", "add", "--", &path_arg, &name]));
            }
            Source::Track(remote) => ops.push(strings(&[
                "worktree",
                "add",
                "--track",
                "-b",
                &name,
                "--",
                &path_arg,
                &remote.name,
            ])),
            Source::FromRemote(remote) => ops.push(strings(&[
                "worktree",
                "add",
                "-b",
                &name,
                "--",
                &path_arg,
                &remote.name,
            ])),
            Source::Fetch => {
                let pull_ref = match self.forge() {
                    Some(Forge::GitLab) => format!("refs/merge-requests/{number}/head"),
                    _ => format!("refs/pull/{number}/head"),
                };
                let from = self.default_remote().unwrap_or_else(|| "origin".to_owned());
                // Not forced: a `pr-N` of the user's own that went elsewhere is refused by git
                // rather than overwritten.
                ops.push(strings(&[
                    "fetch",
                    "--",
                    &from,
                    &format!("{pull_ref}:refs/heads/{name}"),
                ]));
                ops.push(strings(&["worktree", "add", "--", &path_arg, &name]));
            }
        }
        PrWorktree::Create { ops, path }
    }

    /// A branch folder under the repository's shared worktree root.
    pub(in crate::repo) fn worktree_folder(&self, branch: &str) -> PathBuf {
        let main = self
            .worktrees
            .first()
            .map_or(self.path.as_path(), |w| w.path.as_path());
        let root = crate::git::resolve_worktree_root(main, &self.worktree_root)
            .unwrap_or_else(|_| crate::git::default_worktree_root(main));
        crate::git::branch_worktree_folder(&root, branch)
    }

    /// `path`, or `path-2`, `path-3`… when there is something there already. On another
    /// machine, only the worktrees known here count: asking it would hold up the window, and
    /// git refuses a folder that isn't empty anyway.
    pub(in crate::repo) fn free_folder(&self, path: &Path) -> PathBuf {
        let taken = |p: &Path| {
            if crate::remote::is_remote(p) {
                self.worktrees.iter().any(|w| w.path == p)
            } else {
                p.exists()
            }
        };
        if !taken(path) {
            return path.to_path_buf();
        }
        // Appended to the path as written: a path of another machine keeps its `/` on Windows.
        let base = path.to_string_lossy();
        (2..1000)
            .map(|i| PathBuf::from(format!("{base}-{i}")))
            .find(|p| !taken(p))
            .unwrap_or_else(|| path.to_path_buf())
    }
}

/// Whether `name` (a pull request's branch, as the forge says it) is a branch name git takes
/// as it is (`git check-ref-format --branch`), and never as an option.
fn plain_branch_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && name != "@"
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
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
            base: "main".to_owned(),
            from_fork: false,
            sha: sha.to_owned(),
            author: String::new(),
            url: String::new(),
            draft: false,
        }
    }

    /// The second click of a double-click must open details without dispatching another
    /// selection that could replace the page with a late comparison result.
    #[test]
    fn a_click_selects_the_whole_pr_and_a_double_click_opens_its_details() {
        for forge in Forge::ALL {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let tab = RepoTab::new_deferred("/nonexistent/pr-click-test".into(), &ctx);
            let pr = PullRequest {
                number: 7,
                title: "Two commits to review".to_owned(),
                url: "https://example.org/team/app/pull/7".to_owned(),
                ..pr("feature", "head")
            };
            let point = pos2(100.0, 18.0);
            let frame = |time: f64, events: Vec<egui::Event>| {
                let mut acts = Vec::new();
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(360.0, 120.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| tab.pr_row(ui, &mut acts, forge, &pr, true));
                    },
                )
                .drop_without_applying_deltas();
                acts
            };
            let click = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(1.0, vec![egui::Event::PointerMoved(point)]);
            let first = frame(1.1, vec![click(true), click(false)]);
            assert!(
                matches!(first.as_slice(), [Action::SelectPr(7)]),
                "{forge:?}"
            );
            let second = frame(1.2, vec![click(true), click(false)]);
            if forge.has_pages() {
                assert!(
                    matches!(second.as_slice(), [Action::OpenPr(7)]),
                    "{forge:?}"
                );
            } else {
                assert!(
                    matches!(second.as_slice(), [
                    Action::OpenUrl(url),
                    Action::Versions(crate::repo::versions::VersionsAct::PrSeen(7)),
                ] if url == &pr.url),
                    "{forge:?}"
                );
            }
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

    /// On another machine, the worktree goes beside the repository there, named to git as a
    /// path of that machine, and nothing is asked of it to pick the folder.
    #[test]
    fn a_pull_request_on_another_machine_gets_a_worktree_there() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("ssh://build/home/me/app".into(), &ctx);
        tab.refs.local = vec![branch("main", "m"), branch("feat", "f")];
        tab.worktrees = vec![
            worktree("ssh://build/home/me/app", "main", true),
            worktree("ssh://build/home/me/app.wt/feat", "other", false),
        ];
        assert_eq!(
            tab.pr_worktree_plan(3, "feat", "f"),
            PrWorktree::Create {
                ops: vec![
                    strings(&["worktree", "prune"]),
                    strings(&["worktree", "add", "--", "/home/me/app.wt/feat-2", "feat"]),
                ],
                path: PathBuf::from("ssh://build/home/me/app.wt/feat-2"),
            }
        );
    }

    #[test]
    fn a_pull_request_opens_in_its_worktree_made_when_needed() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/r/app".into(), &ctx);
        let hosting = |forge| crate::forge::Hosting {
            forge,
            host: "example.org".to_owned(),
            path: "team/app".to_owned(),
            web: "https://example.org".to_owned(),
        };
        tab.hosting = Some(hosting(Forge::GitHub));
        tab.remotes = vec![Remote {
            name: "origin".to_owned(),
            url: String::new(),
        }];
        tab.refs.local = vec![
            branch("main", "m"),
            branch("mine", "x"),
            branch("feat", "f"),
            branch("pr-9", "elsewhere"),
        ];
        tab.refs.remotes = vec![(
            "origin".to_owned(),
            vec![branch("origin/main", "m"), branch("origin/feat", "f2")],
        )];
        tab.worktrees = vec![
            worktree("/nonexistent/r/app", "main", true),
            worktree("/nonexistent/r/app-mine", "mine", false),
        ];
        // Joined as the code joins it (with `\\` on Windows).
        let folder = |name: &str| Path::new("/nonexistent/r/app.wt").join(name);
        let prune = strings(&["worktree", "prune"]);
        // As this system joins paths (with `\\` on Windows).
        let arg = |name: &str| folder(name).display().to_string();
        let add = |name: &str| strings(&["worktree", "add", "--", &arg(name), name]);
        // Checked out here, or in another worktree.
        assert_eq!(tab.pr_worktree_plan(1, "main", "m"), PrWorktree::Here);
        assert_eq!(
            tab.pr_worktree_plan(2, "mine", "x"),
            PrWorktree::Open(PathBuf::from("/nonexistent/r/app-mine"))
        );
        // A local branch at the pull request's commit: a worktree on it.
        assert_eq!(
            tab.pr_worktree_plan(3, "feat", "f"),
            PrWorktree::Create {
                ops: vec![prune.clone(), add("feat")],
                path: folder("feat"),
            }
        );
        // The remote branch is at it and the local one isn't: a branch of its own from the
        // remote one, the local one left alone.
        assert_eq!(
            tab.pr_worktree_plan(3, "feat", "f2"),
            PrWorktree::Create {
                ops: vec![
                    prune.clone(),
                    strings(&[
                        "worktree",
                        "add",
                        "-b",
                        "pr-3",
                        "--",
                        &arg("pr-3"),
                        "origin/feat",
                    ]),
                ],
                path: folder("pr-3"),
            }
        );
        // Only on the remote: a local branch tracking it, made with the worktree.
        tab.refs.remotes[0].1.push(branch("origin/fix/x", "fx"));
        assert_eq!(
            tab.pr_worktree_plan(4, "fix/x", "fx"),
            PrWorktree::Create {
                ops: vec![
                    prune.clone(),
                    strings(&[
                        "worktree",
                        "add",
                        "--track",
                        "-b",
                        "fix/x",
                        "--",
                        &arg("fix/x"),
                        "origin/fix/x",
                    ]),
                ],
                path: folder("fix/x"),
            }
        );
        // From a fork that isn't a remote: fetched from the pull request's ref (never forced).
        let fetch = |n: u64, name: &str| {
            strings(&[
                "fetch",
                "--",
                "origin",
                &format!("refs/pull/{n}/head:refs/heads/{name}"),
            ])
        };
        assert_eq!(
            tab.pr_worktree_plan(5, "topic", "t"),
            PrWorktree::Create {
                ops: vec![prune.clone(), fetch(5, "topic"), add("topic")],
                path: folder("topic"),
            }
        );
        // A fork's `main`, though ours is a branch here and on origin: not ours, fetched into a
        // branch of its own.
        assert_eq!(
            tab.pr_worktree_plan(6, "main", "fork-main"),
            PrWorktree::Create {
                ops: vec![prune.clone(), fetch(6, "pr-6"), add("pr-6")],
                path: folder("pr-6"),
            }
        );
        // A branch name git would take as an option never reaches it as one.
        assert_eq!(
            tab.pr_worktree_plan(8, "-Bvictim", "v"),
            PrWorktree::Create {
                ops: vec![prune.clone(), fetch(8, "pr-8"), add("pr-8")],
                path: folder("pr-8"),
            }
        );
        // A `pr-N` of the user's own elsewhere: fetched into without force, which git refuses.
        assert!(matches!(
            tab.pr_worktree_plan(9, "-x", "new"),
            PrWorktree::Create { ops, .. } if ops[1] == fetch(9, "pr-9")
        ));
        // Forgejo and Gitea keep them where GitHub does; GitLab, under its own name.
        tab.hosting = Some(hosting(Forge::Forgejo));
        assert!(matches!(
            tab.pr_worktree_plan(7, "topic", "t"),
            PrWorktree::Create { ops, .. } if ops[1][3] == "refs/pull/7/head:refs/heads/topic"
        ));
        tab.hosting = Some(hosting(Forge::GitLab));
        assert!(matches!(
            tab.pr_worktree_plan(7, "topic", "t"),
            PrWorktree::Create { ops, .. } if ops[1][3] == "refs/merge-requests/7/head:refs/heads/topic"
        ));
    }

    #[test]
    fn only_plain_branch_names_are_taken_as_they_are() {
        for good in ["main", "fix/x", "feature-1", "a.b"] {
            assert!(plain_branch_name(good), "{good}");
        }
        for bad in [
            "", "-Bx", "--force", "a..b", "a b", "a:b", "a~1", "x.lock", "/a", "a/", "a//b", "@",
            "a@{1}", "a/.b", ".a", "a\\b",
        ] {
            assert!(!plain_branch_name(bad), "{bad}");
        }
    }
}
