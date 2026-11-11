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
#[path = "pull_requests/tests.rs"]
mod tests;

mod row;
mod worktrees;
