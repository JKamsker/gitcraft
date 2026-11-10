//! The forge source is a UI preference, independent of Git's transport/upstream settings.
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use super::{RepoTab, messages::JobSender};
use crate::{forge::Hosting, git::Remote};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Selection {
    pub name: String,
    /// Used to follow an unambiguous rename. A URL change under the same name follows the name.
    url: String,
}

impl Selection {
    fn id(repo: &Path) -> egui::Id {
        egui::Id::new(("primary-forge-remote", repo))
    }

    fn load(ctx: &egui::Context, repo: &Path) -> Self {
        ctx.data_mut(|d| d.get_persisted(Self::id(repo)))
            .unwrap_or_default()
    }

    fn store(&self, ctx: &egui::Context, repo: &Path) {
        ctx.data_mut(|d| {
            if d.get_persisted::<Self>(Self::id(repo)).as_ref() != Some(self) {
                d.get_temp_mut_or_default::<Arc<AtomicU64>>(lifetime_id(repo))
                    .fetch_add(1, Ordering::SeqCst);
            }
            d.insert_persisted(Self::id(repo), self.clone());
        });
    }

    fn reconcile(&mut self, remotes: &[Remote]) {
        if self.name.is_empty() {
            return;
        }
        if let Some(remote) = remotes.iter().find(|r| r.name == self.name) {
            self.url.clone_from(&remote.url);
            return;
        }
        let mut renamed = remotes.iter().filter(|r| r.url == self.url);
        match (renamed.next(), renamed.next()) {
            (Some(remote), None) => self.name.clone_from(&remote.name),
            _ => *self = Self::default(),
        }
    }

    /// A preference another tab just saved can be newer than our remote catalog.
    /// Follow an unambiguous old name for the same URL, otherwise wait for a fresh
    /// catalog rather than briefly targeting origin or a previous URL's repository.
    fn resolve_current<'a>(
        &self,
        hosts: &'a [(String, Hosting)],
        remotes: &[Remote],
    ) -> Option<&'a (String, Hosting)> {
        if self.name.is_empty()
            || remotes
                .iter()
                .any(|r| r.name == self.name && r.url == self.url)
        {
            return self.resolve(hosts);
        }
        let mut matches = remotes.iter().filter(|r| r.url == self.url);
        match (matches.next(), matches.next()) {
            (Some(remote), None) => hosts.iter().find(|(name, _)| name == &remote.name),
            _ => None,
        }
    }

    fn resolve<'a>(&self, hosts: &'a [(String, Hosting)]) -> Option<&'a (String, Hosting)> {
        hosts
            .iter()
            .find(|(name, _)| name == &self.name)
            .or_else(|| hosts.iter().find(|(name, _)| name == "origin"))
            .or_else(|| hosts.first())
    }
}

fn lifetime_id(repo: &Path) -> egui::Id {
    egui::Id::new(("primary-forge-lifetime", repo))
}

/// Shared by tabs of the same repository. Workers can notice a preference change
/// before their tab next polls, including switching away and back between polls.
pub(super) fn lifetime(ctx: &egui::Context, repo: &Path) -> (Arc<AtomicU64>, u64) {
    ctx.data_mut(|d| {
        let clock = d
            .get_temp_mut_or_default::<Arc<AtomicU64>>(lifetime_id(repo))
            .clone();
        let read = clock.load(Ordering::SeqCst);
        (clock, read)
    })
}

impl RepoTab {
    /// A deep link must never fall back to another repository's PR with the same number.
    pub(super) fn select_linked_forge(&mut self, remote: &str, ctx: &egui::Context) -> bool {
        let available = self.forge_remotes.iter().any(|(name, _)| name == remote);
        if available && self.primary_forge_remote() == Some(remote) {
            return true;
        }
        if self.forge_change_blocked() || !available {
            self.status = Some(super::Notice::plain((
                format!(
                    "Cannot open the linked pull request on remote {remote}: finish any running operation and refresh the repository, then try again"
                ),
                true,
            )));
            return false;
        }
        self.set_primary_forge(Some(remote.to_owned()), ctx);
        self.primary_forge_remote() == Some(remote)
    }

    pub(super) fn primary_forge_remote(&self) -> Option<&str> {
        self.forge_selection
            .resolve_current(&self.forge_remotes, &self.remotes)
            .map(|(name, _)| name.as_str())
    }

    pub(super) fn set_primary_forge(&mut self, name: Option<String>, ctx: &egui::Context) {
        // Do not discard a posting editor or change the target beneath a running mutation.
        if self.forge_change_blocked() {
            return;
        }
        let selection = match name {
            Some(name) => {
                let Some(remote) = self.remotes.iter().find(|r| r.name == name) else {
                    return;
                };
                if !self.forge_remotes.iter().any(|(n, _)| n == &name) {
                    return;
                }
                Selection {
                    name,
                    url: remote.url.clone(),
                }
            }
            None => Selection::default(),
        };
        selection.store(ctx, self.repo_key());
        self.take_forge_selection(false, ctx);
        if crate::network::background_may_run() {
            self.load_prs(ctx);
            self.refresh_ci(ctx);
        }
        self.rebuild(ctx);
    }

    pub(super) fn forge_change_blocked(&self) -> bool {
        self.busy_op.is_some() || self.pr_drafts.values().any(|draft| draft.posting)
    }

    /// Choose using the *current* preference, never the preference a slow reload captured.
    pub(super) fn take_forge_selection(&mut self, remotes_changed: bool, ctx: &egui::Context) {
        let mut selection = Selection::load(ctx, self.repo_key());
        selection.reconcile(&self.remotes);
        selection.store(ctx, self.repo_key());
        self.bind_forge_selection(selection, remotes_changed);
    }

    pub(super) fn take_forge_catalog(
        &mut self,
        remotes_changed: bool,
        read_generation: u64,
        ctx: &egui::Context,
    ) {
        let selection = Selection::load(ctx, self.repo_key());
        let matches = selection.name.is_empty()
            || self
                .remotes
                .iter()
                .any(|r| r.name == selection.name && r.url == selection.url);
        if read_generation != self.forge_generation && !matches {
            // This reload began before a newer preference/catalog was seen in another tab.
            // It cannot reconcile that newer URL back to its captured, previous value.
            self.bind_forge_selection(selection, remotes_changed);
            self.reload_again = true;
        } else {
            self.take_forge_selection(remotes_changed, ctx);
        }
    }

    /// Other worktree/tab views share the same persisted preference. Rebind before draining
    /// their result channel, without reconciling a newer remote name against an older catalog.
    pub(super) fn sync_forge_selection(&mut self, ctx: &egui::Context) {
        if self.forge_generation == 0 {
            return;
        }
        let selection = Selection::load(ctx, self.repo_key());
        if selection == self.forge_selection {
            return;
        }
        let stale_catalog = !selection.name.is_empty()
            && !self
                .remotes
                .iter()
                .any(|r| r.name == selection.name && r.url == selection.url);
        self.bind_forge_selection(selection, false);
        if stale_catalog {
            self.reload(ctx);
        }
        if crate::network::background_may_run() {
            self.load_prs(ctx);
            self.refresh_ci(ctx);
        }
    }

    fn bind_forge_selection(&mut self, selection: Selection, remotes_changed: bool) {
        let hosting = selection
            .resolve_current(&self.forge_remotes, &self.remotes)
            .map(|(_, h)| h.clone());
        let changed =
            selection != self.forge_selection || hosting != self.hosting || remotes_changed;
        self.forge_selection = selection;
        if !changed {
            return;
        }
        self.clear_forge_forks();
        let moved = !super::load::same_repository(self.hosting.as_ref(), hosting.as_ref());
        self.forge_generation += 1;
        if moved || self.forge_repository_generation == 0 {
            self.forge_repository_generation = self.forge_generation;
        }
        self.hosting = hosting;
        let pr_comparison = self.compare.as_ref().is_some_and(|c| {
            self.prs
                .iter()
                .any(|p| c.target.as_deref() == Some(p.sha.as_str()))
        });
        let had_pr_view = self.pr_view.is_some();
        // The same repository's list stays shown until it is read again; another's goes at once.
        if moved {
            self.prs.clear();
        }
        self.pending_prs = None;
        self.pr_error = None;
        self.prs_loaded = None;
        self.close_pr();
        self.ci_view = None;
        self.ci = Default::default();
        if moved {
            self.pr_drafts.clear();
        } else {
            // Completed posts are only waiting on the details/files reads invalidated
            // below. Pending mutations have settling=false and must remain blocked.
            for draft in self.pr_drafts.values_mut() {
                if std::mem::take(&mut draft.settling) {
                    draft.posting = false;
                }
            }
        }
        self.pr_graph_head = None;
        self.versions.seen_heads.clear();
        self.versions.since = None;
        self.versions.view = None;
        self.clear_forge_release();
        self.hotfix.finish = None;
        self.clear_forge_dco();
        let was_pr_comparison = self.pr_selection_request.take().is_some() || pr_comparison;
        if was_pr_comparison {
            self.compare = None;
        }
        if was_pr_comparison || had_pr_view {
            self.diff = None;
        }
        self.forget_merged_prs();
        self.branch_health = Default::default();
        self.forge_issues_changed();
        self.clear_forge_drift();
        self.owners = Default::default();
        self.stacks.publish = None;
        self.avatars_hinted = false;
        if matches!(self.dialog, Some(super::Dialog::CreatePr { .. })) {
            self.dialog = None;
        }
    }

    /// Hosting reads and their continuations carry a selection lifetime, so A→B→A also
    /// rejects A's old results. Generic Git jobs continue to deliver their completion.
    pub(super) fn forge_sender(&self) -> JobSender {
        JobSender {
            forge_generation: Some(self.forge_generation),
            ..self.tx.clone()
        }
    }

    pub(super) fn forge_operation_sender(&self) -> JobSender {
        JobSender {
            forge_operation: true,
            ..self.forge_sender()
        }
    }

    pub(super) fn forge_then<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        super::spawn(&self.forge_sender(), ctx, move || {
            let result = work();
            super::Msg::Then(Box::new(move |tab, ctx| then(tab, result, ctx)))
        });
    }
}

#[cfg(test)]
#[path = "forge_selection_tests.rs"]
mod tests;
