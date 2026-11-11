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
impl RepoTab {
    /// The canonical web instance and repository among the pull requests looked at.
    fn pr_repo_key(&self) -> Option<String> {
        self.hosting
            .as_ref()
            .and_then(|h| h.review_repository_key())
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
            remote: self
                .hosting
                .as_ref()
                .and_then(|h| crate::pull_request::remote_for(&self.remotes, &pr.url, h))
                .map(str::to_owned),
        })
    }

    fn pr_head_of_details(&self, d: &PrDetails) -> PrHead {
        PrHead {
            number: d.number,
            head: d.head_oid.clone(),
            fetch_ref: d.head_ref.clone(),
            base: Some(d.base_oid.clone()),
            remote: self
                .hosting
                .as_ref()
                .and_then(|h| crate::pull_request::remote_for(&self.remotes, &d.url, h))
                .map(str::to_owned),
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
        // Looked at already for this head (the page read again, a review sent): nothing new.
        if self
            .versions
            .since
            .as_ref()
            .is_some_and(|s| s.number == details.number && s.new.tip == details.head_oid)
        {
            return;
        }
        let pr = self.pr_head_of_details(details);
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.forge_then(
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
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "What changed in #{number}: it isn't in the list of this repository's pull requests"
                ),
                true,
            )));
            return;
        };
        // It may fetch from the forge first: say so meanwhile.
        self.status = Some(crate::repo::Notice::plain((
            format!("Reading #{number}…"),
            false,
        )));
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.forge_then(
            ctx,
            move || look(&path, &key, &pr, creds.as_ref()),
            move |tab, found, ctx| match found {
                Ok(Found::Moved(since)) => tab.show_pr_since(since, ctx),
                Ok(Found::First | Found::Same) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("#{number} hasn't changed since you last looked"),
                        false,
                    )));
                    tab.load_seen_heads(ctx);
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("What changed in #{number}: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    pub(super) fn show_pr_since(&mut self, since: Since, ctx: &egui::Context) {
        let number = since.number;
        let title = format!("#{number} since you last looked");
        let labels = (
            format!("Last looked at ({})", short(&since.old.tip)),
            format!("Now ({})", short(&since.new.tip)),
        );
        let seen = since.new.clone();
        self.open_versions(title, labels, since.old, since.new, ctx);
        // Seen once it is shown: a comparison that fails leaves the banner and the version
        // last looked at as they were.
        if let Some(view) = &mut self.versions.view {
            view.seen_when_shown = Some((number, seen));
            view.heading = Some(super::VersionHeading::Since(number));
        }
        if self
            .status
            .as_deref()
            .is_some_and(|(s, _)| s.starts_with("Reading #"))
        {
            self.status = None;
        }
    }

    /// Takes the version `seen` of pull request `number` (the one just compared) as seen.
    pub(super) fn mark_version_seen(&mut self, number: u64, seen: Series, ctx: &egui::Context) {
        let Some(key) = self.pr_repo_key() else {
            return;
        };
        if self
            .versions
            .since
            .as_ref()
            .is_some_and(|s| s.number == number)
        {
            self.versions.since = None;
        }
        self.versions.seen_heads.insert(number, seen.tip.clone());
        let path = self.path.clone();
        self.forge_then(
            ctx,
            move || git::mark_reviewed(&path, &key, number, &seen),
            move |tab, marked, _| tab.seen_marked(number, marked),
        );
    }

    fn seen_marked(&mut self, number: u64, marked: Result<(), String>) {
        if let Err(e) = marked {
            self.versions.seen_heads.remove(&number);
            self.status = Some(crate::repo::Notice::plain((
                format!("Couldn't remember the version of #{number} seen: {e}"),
                true,
            )));
        }
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
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "Mark as seen: #{number} isn't in the list of this repository's pull requests"
                ),
                true,
            )));
            return;
        };
        self.versions.seen_heads.insert(number, pr.head.clone());
        let (path, creds) = (self.path.clone(), self.credentials.clone());
        self.forge_then(
            ctx,
            move || mark_seen(&path, &key, &pr, creds.as_ref()).map(|_| ()),
            move |tab, marked, _| tab.seen_marked(number, marked),
        );
    }

    /// Reads which version of each pull request was seen, for the list (after it is read).
    pub(in crate::repo) fn load_seen_heads(&mut self, ctx: &egui::Context) {
        let Some(key) = self.pr_repo_key() else {
            self.versions.seen_heads.clear();
            return;
        };
        let path = self.path.clone();
        // The list holds at most 100: only a shorter one names every open pull request.
        let open: Option<Vec<u64>> =
            (self.prs.len() < 100).then(|| self.prs.iter().map(|p| p.number).collect());
        self.forge_then(
            ctx,
            move || {
                if let Some(open) = open {
                    // What can't be pruned (a read-only repository) is only kept longer.
                    let _ = git::prune_reviewed(&path, &key, &open);
                }
                git::reviewed_heads(&path, &key)
            },
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
                .button(crate::i18n::text(
                    "repo-workflows-versions-pr-since-items-49804fd7",
                ))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-versions-pr-since-items-528089c6",
                ))
                .clicked()
        {
            acts.push(Action::Versions(VersionsAct::PrSince(pr.number)));
        }
        if !self.versions.seen_heads.contains_key(&pr.number) || self.pr_moved_since_seen(pr) {
            let r = ui
                .button(crate::i18n::text(
                    "repo-workflows-versions-pr-since-items-20383a22",
                ))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-versions-pr-since-items-aeced7e0",
                ));
            if r.clicked() {
                acts.push(Action::Versions(VersionsAct::PrSeen(pr.number)));
            }
        }
    }
}

/// What the banner says, and whether something in the commits changed (else it only moved
/// onto a newer base, every commit the same).
pub(in crate::repo) fn banner_text(since: &Since) -> (String, bool) {
    let c = &since.counts;
    if since.rewritten && c.changed == 0 && c.added == 0 && c.dropped == 0 {
        return (
            crate::i18n::message(
                "repo-workflows-versions-rebased-banner",
                &[(
                    "moved",
                    (if since.counts.moved > 0 { "yes" } else { "no" }).into(),
                )],
            ),
            c.moved > 0,
        );
    }
    let what = if since.rewritten {
        crate::i18n::text("repo-workflows-versions-banner-text-c600fba5")
    } else {
        crate::i18n::text("repo-workflows-versions-banner-text-1b763c24")
    };
    (format!("{what}: {}", super::summary(c)), true)
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
    let (text, loud) = banner_text(since);
    let tint = if loud { t.warning } else { t.text_dim };
    egui::Frame::new()
        .fill(tint.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, tint.gamma_multiply(0.6)))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .outer_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                crate::icons::inline(ui, "history", 14.0, tint);
                let r = ui
                    .add(
                        egui::Label::new(RichText::new(text).color(t.text))
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-versions-since-banner-7b518252",
                    ))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let compare = ui.button(crate::i18n::text(
                    "repo-workflows-versions-since-banner-9e053092",
                ));
                if r.clicked() || compare.clicked() {
                    acts.push(Action::Versions(VersionsAct::PrSince(number)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::small_button(
                        ui,
                        "x",
                        crate::i18n::text("repo-workflows-versions-since-banner-a9306697"),
                    )
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

mod lookup;
#[cfg(test)]
use lookup::is_plain_ref;
use lookup::{look, mark_seen, pull_ref};
