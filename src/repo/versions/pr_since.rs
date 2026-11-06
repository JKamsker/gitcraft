//! What changed in a pull request since it was last looked at: gitcraft remembers the version
//! of each pull request seen (see [`git::reviewed`]), and once the author pushes again (above
//! all, force-pushes a rewrite) says how the commits changed and compares the two versions.
//! Any forge: what the list of pull requests says (number and head) is enough, the page of a
//! forge that has one adds the base.

use std::path::Path;

use egui::{RichText, Ui};

use super::VersionsAct;
use crate::forge::{Forge, PullRequest};
use crate::git::{self, Counts, Credentials, Series};
use crate::pull_request::PrDetails;
use crate::repo::{Action, RepoTab, short};
use crate::theme::Tokens;

/// What changed in the pull request shown since it was last looked at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Since {
    pub number: u64,
    /// The version last looked at, and the one now.
    pub old: Series,
    pub new: Series,
    pub counts: Counts,
    /// Its commits were rewritten (a force push), not only added to.
    pub rewritten: bool,
}

/// A pull request as far as this needs it.
#[derive(Clone, Debug)]
struct PrHead {
    number: u64,
    head: String,
    /// Where the forge keeps its commits (`refs/pull/12/head`), to fetch them by.
    fetch_ref: String,
    /// The commit of its base branch, when the forge says.
    base: Option<String>,
    /// Which remote it is on.
    remote: Option<String>,
}

/// What a look at a pull request found.
#[derive(Debug, PartialEq, Eq)]
enum Found {
    /// Not looked at before: this version is remembered as seen.
    First,
    Same,
    Moved(Since),
}

/// Where a forge keeps the commits of pull request `number`.
fn pull_ref(forge: Option<Forge>, number: u64) -> String {
    match forge {
        Some(Forge::GitLab) => format!("refs/merge-requests/{number}/head"),
        _ => format!("refs/pull/{number}/head"),
    }
}

/// Whether `oid` is a whole commit id, as the forges give them.
fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `r` is a ref a fetch may be given as it is: never an option or a `src:dst` refspec.
fn is_plain_ref(r: &str) -> bool {
    r.starts_with("refs/")
        && !r.contains("..")
        && !r
            .chars()
            .any(|c| c.is_ascii_control() || " ~^:?*[\\".contains(c))
}

/// Makes sure the commits of `pr` are here, fetching them from the forge if not.
fn have_head(repo: &Path, pr: &PrHead, creds: Option<&Credentials>) -> Result<(), String> {
    if !is_oid(&pr.head) {
        return Err("the forge named a head that isn't a commit id".to_owned());
    }
    let has = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if has(&pr.head) {
        return Ok(());
    }
    let remote = pr
        .remote
        .as_deref()
        .ok_or("no remote of this repository has the pull request")?;
    let mut args = vec!["fetch", "--no-tags", "--no-write-fetch-head", remote];
    if is_plain_ref(&pr.fetch_ref) {
        args.push(&pr.fetch_ref);
    }
    // The branch may have moved on since it was read: the commit itself too.
    args.push(&pr.head);
    // Nobody waits on this: it asks nobody to sign in.
    git::run_unattended(Some(repo), &args, creds)?;
    if has(&pr.head) {
        Ok(())
    } else {
        Err(format!("the forge didn't give {}", short(&pr.head)))
    }
}

/// Where the pull request's commits start: where its head forks off its base branch (as the
/// forge names it, else the remote's default branch), else where it forks off the version
/// last looked at.
fn base_of(repo: &Path, pr: &PrHead, seen: Option<&Series>) -> String {
    let default = pr.remote.as_ref().map(|r| format!("refs/remotes/{r}/HEAD"));
    let candidates = pr
        .base
        .iter()
        .filter(|b| is_oid(b))
        .cloned()
        .chain(default)
        .chain(seen.map(|s| s.tip.clone()));
    for candidate in candidates {
        if let Ok(base) = git::merge_base(repo, &candidate, &pr.head) {
            return base;
        }
    }
    pr.head.clone()
}

/// Looks at `pr`: how it changed since the version last looked at, remembering this one as
/// seen when none was.
fn look(
    repo: &Path,
    repo_key: &str,
    pr: &PrHead,
    creds: Option<&Credentials>,
) -> Result<Found, String> {
    let seen = git::reviewed(repo, repo_key, pr.number)?;
    if seen.as_ref().is_some_and(|s| s.tip == pr.head) {
        return Ok(Found::Same);
    }
    have_head(repo, pr, creds)?;
    let new = Series::new(&base_of(repo, pr, seen.as_ref()), &pr.head);
    let Some(old) = seen else {
        git::mark_reviewed(repo, repo_key, pr.number, &new)?;
        return Ok(Found::First);
    };
    let counts = git::range_diff(repo, &old, &new)?.counts();
    let rewritten = !git::is_ancestor(repo, &old.tip, &new.tip);
    Ok(Found::Moved(Since {
        number: pr.number,
        old,
        new,
        counts,
        rewritten,
    }))
}

/// Remembers the version of `pr` now as the one seen; gives it.
fn mark_seen(
    repo: &Path,
    repo_key: &str,
    pr: &PrHead,
    creds: Option<&Credentials>,
) -> Result<Series, String> {
    have_head(repo, pr, creds)?;
    let seen = git::reviewed(repo, repo_key, pr.number)?;
    let new = Series::new(&base_of(repo, pr, seen.as_ref()), &pr.head);
    git::mark_reviewed(repo, repo_key, pr.number, &new)?;
    Ok(new)
}

impl RepoTab {
    /// The repository's key among the pull requests looked at: `host/owner/repo`.
    fn pr_repo_key(&self) -> Option<String> {
        self.hosting
            .as_ref()
            .map(|h| format!("{}/{}", h.host, h.path))
    }

    /// The pull request numbered so: from its page when it is shown (which names its base),
    /// else from the list.
    fn pr_head(&self, number: u64) -> Option<PrHead> {
        let details = self
            .pr_view
            .as_ref()
            .filter(|v| v.number == number)
            .and_then(|v| v.details.as_ref()?.as_ref().ok());
        if let Some(d) = details {
            return Some(self.pr_head_of_details(d));
        }
        let pr: &PullRequest = self.prs.iter().find(|p| p.number == number)?;
        Some(PrHead {
            number,
            head: pr.sha.clone(),
            fetch_ref: pull_ref(self.forge(), number),
            base: None,
            remote: crate::pull_request::remote_for(&self.remotes, &pr.url)
                .map(str::to_owned)
                .or_else(|| self.default_remote()),
        })
    }

    fn pr_head_of_details(&self, d: &PrDetails) -> PrHead {
        PrHead {
            number: d.number,
            head: d.head_oid.clone(),
            fetch_ref: d.head_ref.clone(),
            base: Some(d.base_oid.clone()),
            remote: crate::pull_request::remote_for(&self.remotes, &d.url)
                .map(str::to_owned)
                .or_else(|| self.default_remote()),
        }
    }

    /// Once the page of a pull request has its changes (its commits are here then): whether
    /// it changed since it was last looked at, for the banner.
    pub(in crate::repo) fn check_pr_since(&mut self, details: &PrDetails, ctx: &egui::Context) {
        let Some(key) = self.pr_repo_key() else {
            return;
        };
        if self
            .versions
            .since
            .as_ref()
            .is_some_and(|s| s.number != details.number)
        {
            self.versions.since = None;
        }
        let pr = self.pr_head_of_details(details);
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.then(
            ctx,
            move || look(&path, &key, &pr, creds.as_ref()),
            |tab, found, _| tab.pr_looked(found),
        );
    }

    fn pr_looked(&mut self, found: Result<Found, String>) {
        match found {
            Ok(Found::Moved(since)) => {
                // Only for the pull request still shown.
                if self
                    .pr_view
                    .as_ref()
                    .is_some_and(|v| v.number == since.number)
                {
                    self.versions.since = Some(since);
                }
            }
            Ok(Found::First | Found::Same) => self.versions.since = None,
            // Nothing to say then: the page works as it did.
            Err(_) => {}
        }
    }

    /// Compares pull request `number` as last looked at with how it is now, then takes this
    /// version as seen.
    pub(super) fn open_pr_since(&mut self, number: u64, ctx: &egui::Context) {
        if let Some(since) = self.versions.since.clone().filter(|s| s.number == number) {
            self.show_pr_since(since, ctx);
            return;
        }
        let (Some(key), Some(pr)) = (self.pr_repo_key(), self.pr_head(number)) else {
            return;
        };
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.then(
            ctx,
            move || look(&path, &key, &pr, creds.as_ref()),
            move |tab, found, ctx| match found {
                Ok(Found::Moved(since)) => tab.show_pr_since(since, ctx),
                Ok(Found::First | Found::Same) => {
                    tab.status = Some((
                        format!("#{number} hasn't changed since you last looked"),
                        false,
                    ));
                    tab.load_seen_heads(ctx);
                }
                Err(e) => tab.status = Some((format!("What changed in #{number}: {e}"), true)),
            },
        );
    }

    fn show_pr_since(&mut self, since: Since, ctx: &egui::Context) {
        let number = since.number;
        let title = format!("#{number} since you last looked");
        let labels = (
            format!("Last looked at ({})", short(&since.old.tip)),
            format!("Now ({})", short(&since.new.tip)),
        );
        self.open_versions(title, labels, since.old, since.new, ctx);
        self.mark_pr_seen(number, ctx);
    }

    /// Takes pull request `number` as it is now as seen: the banner goes, and the next push
    /// is compared with this version.
    pub(super) fn mark_pr_seen(&mut self, number: u64, ctx: &egui::Context) {
        if self
            .versions
            .since
            .as_ref()
            .is_some_and(|s| s.number == number)
        {
            self.versions.since = None;
        }
        let (Some(key), Some(pr)) = (self.pr_repo_key(), self.pr_head(number)) else {
            return;
        };
        self.versions.seen_heads.insert(number, pr.head.clone());
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.then(
            ctx,
            move || mark_seen(&path, &key, &pr, creds.as_ref()),
            move |tab, marked, _| {
                if let Err(e) = marked {
                    tab.versions.seen_heads.remove(&number);
                    tab.status = Some((
                        format!("Couldn't remember the version of #{number} seen: {e}"),
                        true,
                    ));
                }
            },
        );
    }

    /// Reads which version of each pull request was seen, for the list (after it is read).
    pub(in crate::repo) fn load_seen_heads(&mut self, ctx: &egui::Context) {
        let Some(key) = self.pr_repo_key() else {
            self.versions.seen_heads.clear();
            return;
        };
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::reviewed_heads(&path, &key),
            |tab, heads, _| {
                if let Ok(heads) = heads {
                    tab.versions.seen_heads = heads.into_iter().collect();
                }
            },
        );
    }

    /// Whether `pr` moved on since the version of it seen (none seen: no).
    pub(in crate::repo) fn pr_moved_since_seen(&self, pr: &PullRequest) -> bool {
        self.versions
            .seen_heads
            .get(&pr.number)
            .is_some_and(|seen| *seen != pr.sha)
    }

    /// The menu items of a pull request in the list about the version seen.
    pub(in crate::repo) fn pr_since_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        pr: &PullRequest,
    ) {
        if self.pr_moved_since_seen(pr)
            && ui
                .button("What changed since I last looked")
                .on_hover_text("Its commits as you last saw them and as they are now, paired up")
                .clicked()
        {
            acts.push(Action::Versions(VersionsAct::PrSince(pr.number)));
        }
        if !self.versions.seen_heads.contains_key(&pr.number) || self.pr_moved_since_seen(pr) {
            let r = ui
                .button("Mark as seen")
                .on_hover_text("The next push is compared with the version it has now");
            if r.clicked() {
                acts.push(Action::Versions(VersionsAct::PrSeen(pr.number)));
            }
        }
    }
}

/// The banner on the page of a pull request that changed since it was last looked at.
pub(in crate::repo) fn since_banner(
    ui: &mut Ui,
    since: Option<&Since>,
    number: u64,
    acts: &mut Vec<Action>,
) {
    let Some(since) = since.filter(|s| s.number == number) else {
        return;
    };
    let t = Tokens::get(ui.ctx());
    let what = if since.rewritten {
        "Rewritten since you last looked"
    } else {
        "New commits since you last looked"
    };
    egui::Frame::new()
        .fill(t.warning.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, t.warning.gamma_multiply(0.6)))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .outer_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                crate::icons::inline(ui, "history", 14.0, t.warning);
                let text = format!("{what}: {}", super::summary(&since.counts));
                let r = ui
                    .add(
                        egui::Label::new(RichText::new(text).color(t.text))
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Compare the two versions")
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let compare = ui.button("Compare versions");
                if r.clicked() || compare.clicked() {
                    acts.push(Action::Versions(VersionsAct::PrSince(number)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::small_button(ui, "x", "Mark as seen without comparing")
                        .clicked()
                    {
                        acts.push(Action::Versions(VersionsAct::PrSeen(number)));
                    }
                });
            });
        });
}

#[cfg(test)]
mod tests;
