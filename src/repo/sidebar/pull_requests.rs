//! The pull (or merge) requests of the repository's forge.

use std::path::{Path, PathBuf};

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use super::remote_short;
use crate::a11y::labels;
use crate::forge::{Forge, Hosting, PullRequest};
use crate::git::Branch;
use crate::icons;
use crate::menu_item::MenuItem;
use crate::repo::issue_work::IssueAct;
use crate::repo::pinned_sections::{Alert, Button, Header, Sections};
use crate::repo::{Action, RepoTab, ci_status, widgets};
use crate::theme::Tokens;

/// How long (in seconds) after the list was last in use a newer one waits before it replaces it.
const IN_USE_GRACE: f64 = 3.0;

/// Where the time the tab's list was last in use is kept (egui's clock).
fn in_use_id(tab: &Path) -> egui::Id {
    egui::Id::new(("pr-list-in-use", tab))
}

/// Where the last list read is kept, per repository, hosting and account (a profile's), across
/// restarts.
fn cache_id(repo: &Path, hosting: &Hosting) -> egui::Id {
    let account = crate::connections::current();
    egui::Id::new((
        "pr-list-cache",
        repo,
        &hosting.host,
        &hosting.path,
        &account.git_config,
        &account.choices,
    ))
}

/// Whether a list read anew replaces the `shown` one now: when nothing would move, or the list
/// was last in use `since` seconds ago, long enough (`None`: not at all).
fn apply_now(shown: &[PullRequest], fresh: &[PullRequest], since: Option<f64>) -> bool {
    shown == fresh || shown.is_empty() || since.is_none_or(|since| since >= IN_USE_GRACE)
}

/// The orange of a header whose newer list waits: between the warning yellow and the danger red.
fn attention(t: &Tokens) -> egui::Color32 {
    let mix = |a: u8, b: u8| ((u16::from(a) * 3 + u16::from(b) * 2) / 5) as u8;
    let (w, d) = (t.warning, t.danger);
    egui::Color32::from_rgb(mix(w.r(), d.r()), mix(w.g(), d.g()), mix(w.b(), d.b()))
}

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
        let top = ui.cursor().top();
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
                    tooltip: crate::i18n::text("repo-core-3447"),
                    action: Some(Action::Issue(IssueAct::OpenPicker)),
                }),
            },
        );
        if self.pending_prs.is_some() {
            sections.alert(Alert {
                tint: attention(&t),
                button: Button {
                    icon: "refresh-cw",
                    tooltip: crate::i18n::text("repo-core-pr-list-newer"),
                    action: Some(Action::ApplyPendingPrs),
                },
            });
        }
        if open {
            // A failed read leaves the rows of the last one, with why under them.
            for pr in &self.prs {
                self.pr_row(ui, acts, forge, pr, idle);
            }
            if let Some(raw_error) = &self.pr_error {
                let err = crate::forge::inbox::display_error(raw_error);
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
            }
        }
        let area = Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        let in_use = ui.rect_contains_pointer(area)
            || (open
                && self
                    .prs
                    .iter()
                    .any(|pr| widgets::keyed_row_menu_open(ui, ("pr-row", pr.number))));
        self.note_pr_list_use(ui.ctx(), in_use, acts);
    }

    /// Notes when the list was last in use (the pointer on it, a row's menu open). A newer list
    /// waiting is let in once it has been left alone for [`IN_USE_GRACE`].
    fn note_pr_list_use(&self, ctx: &egui::Context, in_use: bool, acts: &mut Vec<Action>) {
        let id = in_use_id(&self.path);
        let now = ctx.input(|i| i.time);
        if in_use {
            ctx.data_mut(|d| d.insert_temp(id, now));
            return;
        }
        if self.pending_prs.is_none() {
            return;
        }
        let since = ctx.data(|d| d.get_temp::<f64>(id)).map(|at| now - at);
        match since {
            Some(since) if since < IN_USE_GRACE => {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(IN_USE_GRACE - since))
            }
            _ => acts.push(Action::ApplyPendingPrs),
        }
    }

    /// Takes in a list read anew: at once, unless it would move the rows under the user, who
    /// then gets it from the header (orange, with a refresh button) or once done with the list.
    pub(in crate::repo) fn prs_arrived(&mut self, prs: Vec<PullRequest>, ctx: &egui::Context) {
        self.pr_error = None;
        if let Some(hosting) = &self.hosting {
            let id = cache_id(self.repo_key(), hosting);
            ctx.data_mut(|d| d.insert_persisted(id, prs.clone()));
        }
        let since = ctx
            .data(|d| d.get_temp::<f64>(in_use_id(&self.path)))
            .map(|at| ctx.input(|i| i.time) - at);
        if apply_now(&self.prs, &prs, since) {
            self.take_prs(prs, ctx);
        } else {
            self.pending_prs = Some(prs);
            ctx.request_repaint();
        }
    }

    /// Shows the list read while the shown one was in use.
    pub(in crate::repo) fn apply_pending_prs(&mut self, ctx: &egui::Context) {
        if let Some(prs) = self.pending_prs.take() {
            self.take_prs(prs, ctx);
        }
    }

    fn take_prs(&mut self, prs: Vec<PullRequest>, ctx: &egui::Context) {
        self.pending_prs = None;
        self.prs = prs;
        self.prs_from_cache = false;
        if crate::network::background_may_run() {
            self.refresh_ci(ctx);
        }
        self.load_seen_heads(ctx);
    }

    /// Before the forge is asked, the list it gave last time for `hosting` (kept across
    /// restarts), so that the section isn't empty meanwhile.
    pub(in crate::repo) fn seed_prs(&mut self, hosting: &Hosting, ctx: &egui::Context) {
        if !self.prs.is_empty() || self.pending_prs.is_some() {
            return;
        }
        let id = cache_id(self.repo_key(), hosting);
        if let Some(prs) = ctx.data_mut(|d| d.get_persisted::<Vec<PullRequest>>(id)) {
            self.prs = prs;
            self.prs_from_cache = true;
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
                crate::i18n::text("repo-core-3449")
            } else {
                ""
            },
            pr.branch,
            pr.author,
            if pr.draft {
                crate::i18n::text("repo-core-3450")
            } else {
                ""
            }
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
            crate::i18n::message("repo-core-3451", &[("url", pr.url.to_string().into())])
        } else {
            crate::i18n::message("repo-core-3452", &[("url", pr.url.to_string().into())])
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
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| {
                if has_view
                    && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3453"))
                        .icon("eye")
                        .show(ui)
                        .clicked()
                {
                    acts.push(Action::OpenPr(pr.number));
                }
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3454"))
                    .icon("external-link")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                    if !has_view && !moved {
                        acts.push(Action::Versions(
                            crate::repo::versions::VersionsAct::PrSeen(pr.number),
                        ));
                    }
                }
                self.pr_since_items(ui, acts, pr);
                self.pr_worktree_button(ui, acts, pr.number, &pr.branch, &pr.sha, idle, true);
                if crate::menu_item::MenuItem::new(crate::i18n::message(
                    "repo-core-3455",
                    &[("branch", pr.branch.to_string().into())],
                ))
                .icon("log-in")
                .enabled(idle)
                .show(ui)
                .clicked()
                {
                    if self.refs.local.iter().any(|b| b.name == pr.branch) {
                        acts.push(Action::Checkout(pr.branch.clone()));
                    } else if let Some(b) = self.pr_remote_branch(pr) {
                        acts.push(Action::CheckoutRemote(b.name.clone()));
                    } else {
                        let Some(remote) =
                            self.primary_forge_remote().map(str::to_owned).or_else(|| {
                                self.hosting
                                    .is_none()
                                    .then(|| self.default_remote())
                                    .flatten()
                            })
                        else {
                            return;
                        };
                        acts.push(Action::CheckoutRemote(format!("{remote}/{}", pr.branch)));
                    }
                }
                self.fork_items(ui, acts, pr.number, idle, false);
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3456"))
                    .icon("copy")
                    .show(ui)
                    .clicked()
                {
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
        let default = self.primary_forge_remote().map(str::to_owned).or_else(|| {
            self.hosting
                .is_none()
                .then(|| self.default_remote())
                .flatten()
        });
        let found: Vec<(&str, &Branch)> = self
            .remotes
            .iter()
            .flat_map(|r| {
                self.remote_branches(&r.name)
                    .filter(|b| remote_short(&b.name, &r.name) == Some(branch))
                    .filter(|b| {
                        self.hosting
                            .as_ref()
                            .is_none_or(|h| h.matches_git_remote(&r.url))
                            || (!sha.is_empty() && b.target == sha)
                    })
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
            PrWorktree::Here => (false, crate::i18n::text("repo-core-3457").to_owned()),
            PrWorktree::Open(path) => (
                true,
                crate::i18n::message(
                    "repo-core-3458",
                    &[("path", crate::remote::display(path).to_string().into())],
                ),
            ),
            PrWorktree::Create { path, .. } => (
                idle,
                crate::i18n::message(
                    "repo-core-3459",
                    &[("path", crate::remote::display(path).to_string().into())],
                ),
            ),
        };
        let r = if in_menu {
            // Why it can't open, under it; what it would do, on hover.
            let here = matches!(plan, PrWorktree::Here);
            MenuItem::new(crate::i18n::text("repo-core-3460"))
                .icon("folder-git-2")
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
                crate::i18n::text("repo-core-3460"),
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
        if self.hosting.is_some() && self.primary_forge_remote().is_none() {
            self.status = Some(crate::repo::Notice::plain((
                "The primary forge remote is unavailable; refresh before opening this pull request"
                    .into(),
                true,
            )));
            return;
        }
        match self.pr_worktree_plan(number, branch, sha) {
            PrWorktree::Here => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-3461",
                        &[("number", format!("{}", number).into())],
                    ),
                    false,
                )));
            }
            PrWorktree::Open(path) => {
                self.open_request = Some(crate::repo::OpenRequest::Worktree(path));
            }
            PrWorktree::Create { ops, path } => {
                if self.busy_op.is_some() {
                    return;
                }
                let label = crate::i18n::message(
                    "repo-core-3462",
                    &[("number", format!("{}", number).into())],
                );
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
                let from = self
                    .primary_forge_remote()
                    .map(str::to_owned)
                    .or_else(|| {
                        self.hosting
                            .is_none()
                            .then(|| self.default_remote())
                            .flatten()
                    })
                    .unwrap_or_else(|| "origin".to_owned());
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

    fn numbered(number: u64) -> PullRequest {
        PullRequest {
            number,
            title: format!("Change {number}"),
            branch: format!("change-{number}"),
            base: "main".into(),
            base_oid: String::new(),
            from_fork: false,
            sha: format!("{number:040x}"),
            author: "ann".into(),
            url: String::new(),
            draft: false,
        }
    }

    #[test]
    fn a_newer_list_waits_while_the_shown_one_is_in_use() {
        let shown = [numbered(1), numbered(2)];
        let fresh = [numbered(3), numbered(1), numbered(2)];
        assert!(!apply_now(&shown, &fresh, Some(0.5)));
        assert!(apply_now(&shown, &fresh, Some(IN_USE_GRACE)));
        assert!(apply_now(&shown, &fresh, None));
        // Nothing would move: the same list, or none shown yet.
        assert!(apply_now(&shown, &shown, Some(0.0)));
        assert!(apply_now(&[], &fresh, Some(0.0)));
    }

    #[test]
    fn a_list_read_while_in_use_is_kept_aside_until_asked_for() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-pending".into(), &ctx);
        tab.prs = vec![numbered(1)];
        tab.prs_from_cache = true;
        let now = ctx.input(|i| i.time);
        ctx.data_mut(|d| d.insert_temp(in_use_id(&tab.path), now));
        tab.prs_arrived(vec![numbered(2), numbered(1)], &ctx);
        assert_eq!(tab.prs, vec![numbered(1)]);
        assert_eq!(tab.pending_prs, Some(vec![numbered(2), numbered(1)]));
        assert!(tab.prs_from_cache); // A pending response does not refresh displayed rows.
        tab.apply_pending_prs(&ctx);
        assert_eq!(tab.prs, vec![numbered(2), numbered(1)]);
        assert!(tab.pending_prs.is_none());
        assert!(!tab.prs_from_cache);
    }

    #[test]
    fn seeded_pr_list_keeps_its_provenance_until_a_fresh_response_is_displayed() {
        let ctx = egui::Context::default();
        let hosting = Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            path: "o/r".into(),
            web: "https://github.com".into(),
        };
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-seeded".into(), &ctx);
        let id = cache_id(tab.repo_key(), &hosting);
        ctx.data_mut(|d| d.insert_persisted(id, vec![numbered(7)]));
        tab.seed_prs(&hosting, &ctx);
        assert_eq!(tab.prs, vec![numbered(7)]);
        assert!(tab.prs_from_cache);
        // Even an identical fresh response makes the displayed endpoints trustworthy.
        tab.prs_arrived(vec![numbered(7)], &ctx);
        assert!(!tab.prs_from_cache);
    }

    #[test]
    fn a_read_after_a_failed_one_clears_its_error() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pr-list-error".into(), &ctx);
        tab.prs = vec![numbered(1)];
        tab.pr_error = Some("timeout".into());
        tab.prs_arrived(vec![numbered(1)], &ctx);
        assert!(tab.pr_error.is_none());
        assert_eq!(tab.prs, vec![numbered(1)]);
    }

    #[test]
    fn review14_pr_branch_lookup_does_not_fall_back_to_another_instance() {
        use crate::forge::Hosting;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/review14-pr-lookup".into(), &ctx);
        let h = Hosting {
            forge: Forge::GitLab,
            host: "forge.example".into(),
            web: "https://forge.example:4000/proxy".into(),
            path: "Group/Project".into(),
        };
        tab.remotes = [
            ("origin", "https://forge.example:3000/proxy/Group/Project"),
            ("case", "https://forge.example:4000/proxy/group/project"),
            ("selected", "https://forge.example:4000/proxy/Group/Project"),
        ]
        .map(|(name, url)| Remote {
            name: name.into(),
            url: url.into(),
        })
        .into();
        tab.forge_remotes = vec![("selected".into(), h)];
        tab.take_forge_selection(false, &ctx);
        tab.refs.remotes = tab
            .remotes
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    vec![branch(&format!("{}/fix", r.name), &r.name)],
                )
            })
            .collect();
        assert_eq!(
            tab.remote_branch_of("fix", "").map(|b| b.name.as_str()),
            Some("selected/fix")
        );
        tab.refs.remotes.retain(|(r, _)| r != "selected");
        assert!(tab.remote_branch_of("fix", "").is_none());
        // A known immutable commit can safely be read from another local object source.
        assert_eq!(
            tab.remote_branch_of("fix", "origin")
                .map(|b| b.name.as_str()),
            Some("origin/fix")
        );
    }

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
            base_oid: String::new(),
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

    /// A saved root takes precedence over the default, including from linked checkouts.
    #[test]
    fn a_pull_request_worktree_uses_the_saved_root() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("ssh://build/elsewhere/linked".into(), &ctx);
        tab.worktrees = vec![worktree("ssh://build/home/me/app", "main", false)];
        tab.worktree_root = "custom.worktrees".to_owned();
        tab.refs.local = vec![branch("feat/awesome-stuff", "f")];
        let path = "/home/me/app/custom.worktrees/feat/awesome-stuff";
        assert_eq!(
            tab.pr_worktree_plan(3, "feat/awesome-stuff", "f"),
            PrWorktree::Create {
                ops: vec![
                    strings(&["worktree", "prune"]),
                    strings(&["worktree", "add", "--", path, "feat/awesome-stuff"])
                ],
                path: format!("ssh://build{path}").into(),
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
