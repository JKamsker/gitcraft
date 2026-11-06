//! The tab's side of branch cleanup: reading which branches are merged or stale when the
//! branches change, the merged pull requests that read uses, and the branches _Select stale or
//! merged branches…_ offers.

use super::branch_health::{self, Bases, Inputs, Reason};
use super::{Dialog, RepoTab};
use crate::git::{self, Branch};
use crate::settings::Settings;

/// A branch _Select stale or merged branches…_ offers, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub name: String,
    /// Its commit when it was offered: it is deleted only while it is still there.
    pub target: String,
    /// The branch it tracks, when that is still on the remote: `(remote, name there, its last
    /// fetched commit)`.
    pub remote: Option<(String, String, String)>,
    /// The steps that make it track its upstream again, for undo.
    pub track: Vec<Vec<String>>,
    pub reasons: Vec<Reason>,
}

impl Candidate {
    /// Ticked when the dialog opens: its work is in the default branch, or its upstream is gone
    /// along with being stale. Stale alone, or gone alone, may still hold work.
    pub fn preselected(&self) -> bool {
        let gone = self.reasons.contains(&Reason::UpstreamGone);
        let stale = self.reasons.iter().any(|r| matches!(r, Reason::Stale(_)));
        self.reasons.iter().any(Reason::is_merged) || (gone && stale)
    }
}

impl RepoTab {
    /// The default branch and the refs of it that exist here: on the default remote, and local.
    fn health_bases(&self) -> Bases {
        let name = self.default_base();
        let mut refs = Vec::new();
        if let Some(remote) = self.default_remote() {
            let short = format!("{remote}/{name}");
            if let Some(b) = self.remote_branches(&remote).find(|b| b.name == short) {
                refs.push((format!("refs/remotes/{short}"), b.target.clone()));
            }
        }
        if let Some(b) = self.refs.local.iter().find(|b| b.name == name) {
            refs.push((format!("refs/heads/{name}"), b.target.clone()));
        }
        Bases { name, refs }
    }

    /// Reads again, in the background, which branches are merged and when each last had a
    /// commit, if the branches, the default branch or the merged pull requests changed.
    pub(super) fn want_health(&mut self, ctx: &egui::Context) {
        let bases = self.health_bases();
        let key = branch_health::key_of(&self.refs.local, &bases, &self.branch_health.merged_prs);
        if !self.branch_health.should_read(key) {
            return;
        }
        let inputs = Inputs::new(&self.refs.local, bases, &self.branch_health.merged_prs);
        let path = self.path.clone();
        self.then(
            ctx,
            move || branch_health::read(&path, &inputs),
            move |tab, result, _| tab.branch_health.arrived(key, result),
        );
    }

    /// Lists the merged pull requests in the background, then reads the branches again with
    /// them. Kept until a list comes back: a forge out of reach changes nothing.
    pub(super) fn load_merged_prs(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let merged = hosting.forge.provider().merged(&path, &hosting);
                (hosting, merged)
            },
            |tab, (hosting, merged), ctx| {
                let same = super::load::same_repository(Some(&hosting), tab.hosting.as_ref());
                if let (true, Ok(prs)) = (same, merged)
                    && prs != tab.branch_health.merged_prs
                {
                    tab.branch_health.merged_prs = prs;
                    tab.want_health(ctx);
                }
            },
        );
    }

    /// The repository is another one on the forge now: its merged pull requests are not this
    /// one's.
    pub(super) fn forget_merged_prs(&mut self) {
        self.branch_health.merged_prs.clear();
        self.branch_health.invalidate();
    }

    /// Why `b` could go, as the badges and the cleanup show it.
    pub(super) fn branch_reasons(&self, b: &Branch, stale_days: u32) -> Vec<Reason> {
        match &self.branch_health.health {
            Some(health) => health.reasons(b, git::now(), stale_days),
            None if b.upstream_gone() => vec![Reason::UpstreamGone],
            None => Vec::new(),
        }
    }

    /// Whether `b` may be offered at all: never the checked-out branch, the default one, or
    /// one checked out in a worktree.
    fn offerable(&self, b: &Branch, default: &str) -> bool {
        !b.is_head
            && b.name != default
            && !self
                .worktrees
                .iter()
                .any(|w| w.branch.as_deref() == Some(b.name.as_str()))
    }

    /// The branches _Select stale or merged branches…_ offers, in the order of the branch list.
    pub(super) fn cleanup_candidates(&self, stale_days: u32) -> Vec<Candidate> {
        let default = self.default_base();
        self.refs
            .local
            .iter()
            .filter(|b| self.offerable(b, &default))
            .filter_map(|b| {
                let reasons = self.branch_reasons(b, stale_days);
                (!reasons.is_empty()).then(|| self.candidate(b, reasons))
            })
            .collect()
    }

    fn candidate(&self, b: &Branch, reasons: Vec<Reason>) -> Candidate {
        let tracked = (!b.upstream_gone())
            .then(|| self.split_upstream(&b.upstream))
            .flatten()
            .and_then(|(remote, name)| {
                let at = self
                    .remote_branches(&remote)
                    .find(|r| r.name == b.upstream)?
                    .target
                    .clone();
                Some((remote, name.to_owned(), at))
            });
        Candidate {
            name: b.name.clone(),
            target: b.target.clone(),
            remote: tracked,
            track: self.track_ops(&b.name, &b.upstream),
            reasons,
        }
    }

    /// _Select stale or merged branches…_ with what there is to offer now.
    pub(super) fn cleanup_dialog(&self, ctx: &egui::Context) -> Dialog {
        let days = Settings::get(ctx).stale_branch_days;
        let checking = self.branch_health.health.is_none() && self.branch_health.error.is_none();
        Dialog::cleanup(
            self.cleanup_candidates(days),
            checking,
            self.branch_health.error.clone(),
        )
    }
}

#[cfg(test)]
#[path = "branch_cleanup_tests.rs"]
mod tests;
