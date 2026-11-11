//! Branches hidden from the graph, or soloed: the graph then has only the commits of the
//! branches it still shows. Kept per repository with the app.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::RepoTab;
use crate::git::Refs;

/// Which branches the graph shows, by their full ref names (`refs/heads/main`,
/// `refs/remotes/origin/main`). The checked-out branch is always shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Visibility {
    /// Left out of the graph, while nothing is soloed.
    pub hidden: BTreeSet<String>,
    /// When any are: the only branches the graph shows.
    pub solo: BTreeSet<String>,
    /// Group hiding in the sidebar is independent of graph reachability.
    pub sidebar_remotes: BTreeSet<String>,
    pub graph_remotes: BTreeSet<String>,
    /// Individual Show overrides a hidden group. Solo also takes precedence over groups.
    pub shown: BTreeSet<String>,
    #[serde(skip)]
    remote_names: BTreeSet<String>,
}

/// The full ref name of the local branch `name`.
pub(crate) fn local_ref(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// The full ref name of the remote-tracking branch `name` (`origin/main`).
pub(crate) fn remote_ref(name: &str) -> String {
    format!("refs/remotes/{name}")
}

fn id(repo: &Path) -> egui::Id {
    egui::Id::new(("branch-visibility", repo))
}

impl Visibility {
    /// What is kept for the repository `repo`.
    pub fn load(ctx: &egui::Context, repo: &Path) -> Self {
        ctx.data_mut(|d| d.get_persisted::<Visibility>(id(repo)))
            .unwrap_or_default()
    }

    pub fn store(&self, ctx: &egui::Context, repo: &Path) {
        ctx.data_mut(|d| d.insert_persisted(id(repo), self.clone()));
    }

    /// Whether some branches are left out.
    pub fn is_filtering(&self) -> bool {
        !self.hidden.is_empty() || !self.solo.is_empty() || !self.graph_remotes.is_empty()
    }

    pub fn is_soloing(&self) -> bool {
        !self.solo.is_empty()
    }

    /// Whether the branch `full` is in the graph, unless it is the checked-out one: that one
    /// always is.
    pub fn shows(&self, full: &str) -> bool {
        if self.is_soloing() {
            self.solo.contains(full)
        } else {
            !self.hidden.contains(full) && (!self.group_hidden(full) || self.shown.contains(full))
        }
    }

    /// Shows `full` if it isn't shown, else leaves it out: soloed along with the others while
    /// some are soloed, hidden otherwise.
    pub fn toggle(&mut self, full: &str) {
        if self.is_soloing() {
            if !self.solo.remove(full) {
                self.solo.insert(full.to_owned());
            }
        } else if self.shows(full) {
            self.shown.remove(full);
            self.hidden.insert(full.to_owned());
        } else {
            self.hidden.remove(full);
            self.shown.insert(full.to_owned());
        }
    }

    fn remote_group(&self, full: &str) -> Option<&str> {
        let short = full.strip_prefix("refs/remotes/")?;
        self.remote_names
            .iter()
            .chain(self.graph_remotes.iter())
            .filter(|r| {
                short
                    .strip_prefix(r.as_str())
                    .is_some_and(|tail| tail.starts_with('/'))
            })
            .max_by_key(|r| r.len())
            .map(String::as_str)
    }

    fn group_hidden(&self, full: &str) -> bool {
        self.remote_group(full)
            .is_some_and(|r| self.graph_remotes.contains(r))
    }

    pub fn toggle_remote_graph(&mut self, remote: &str) {
        if !self.graph_remotes.remove(remote) {
            self.graph_remotes.insert(remote.to_owned());
            let overrides: Vec<_> = self
                .shown
                .iter()
                .filter(|r| self.remote_group(r) == Some(remote))
                .cloned()
                .collect();
            for full in overrides {
                self.shown.remove(&full);
            }
        }
    }

    fn show_all(&mut self) {
        self.hidden.clear();
        self.solo.clear();
        self.shown.clear();
        self.graph_remotes.clear();
    }

    /// Solos `full` along with the branches already soloed, or unsolos it.
    pub fn toggle_solo(&mut self, full: &str) {
        if !self.solo.remove(full) {
            self.solo.insert(full.to_owned());
        }
    }

    /// Forgets the branches that no longer exist; whether any were.
    pub fn prune(&mut self, refs: &Refs) -> bool {
        self.remote_names = refs.remotes.iter().map(|(r, _)| r.clone()).collect();
        let all: BTreeSet<String> = branch_refs(refs).map(|(full, _)| full).collect();
        let before = (self.hidden.len(), self.solo.len(), self.shown.len());
        self.hidden.retain(|r| all.contains(r));
        self.solo.retain(|r| all.contains(r));
        self.shown.retain(|r| all.contains(r));
        before != (self.hidden.len(), self.solo.len(), self.shown.len())
    }

    /// What the graph's `git log` starts from: `None` for every branch, else HEAD and the
    /// branches shown.
    pub fn revisions(&self, refs: &Refs) -> Option<Vec<String>> {
        if !self.is_filtering() {
            return None;
        }
        // A freshly deserialized preference has no cached remote names. Use the refs
        // being logged, including nested remote names, when computing background roots.
        let mut visibility = self.clone();
        visibility.remote_names = refs.remotes.iter().map(|(name, _)| name.clone()).collect();
        let head = (!refs.head.is_empty()).then(|| "HEAD".to_owned());
        Some(
            head.into_iter()
                .chain(
                    branch_refs(refs)
                        .filter(|(full, is_head)| !is_head && visibility.shows(full))
                        .map(|(full, _)| full),
                )
                .collect(),
        )
    }

    /// How many branches the graph shows, out of how many there are.
    pub fn counts(&self, refs: &Refs) -> (usize, usize) {
        branch_refs(refs).fold((0, 0), |(shown, all), (full, is_head)| {
            (shown + usize::from(is_head || self.shows(&full)), all + 1)
        })
    }
}

/// A change to which branches the graph shows.
pub(crate) enum Change {
    /// Hide the branch, or show it again.
    Toggle(String),
    /// Solo the branch along with those already soloed, or unsolo it.
    Solo(String),
    /// Show every branch.
    ShowAll,
    RemoteSidebar(String),
    RemoteGraph(String),
}

impl RepoTab {
    pub(crate) fn portable_visibility(ctx: &egui::Context, path: &Path) -> Vec<String> {
        Visibility::load(ctx, path).hidden.into_iter().collect()
    }
    pub(crate) fn restore_portable_visibility(ctx: &egui::Context, path: &Path, hidden: &[String]) {
        Visibility {
            hidden: hidden.iter().cloned().collect(),
            ..Default::default()
        }
        .store(ctx, path);
    }

    /// Changes which branches the graph shows, keeps that, and reads the graph again.
    pub(super) fn change_visibility(&mut self, change: Change, ctx: &egui::Context) {
        match change {
            Change::Toggle(full) => self.visibility.toggle(&full),
            Change::Solo(full) => self.visibility.toggle_solo(&full),
            Change::ShowAll => self.visibility.show_all(),
            Change::RemoteGraph(remote) => self.visibility.toggle_remote_graph(&remote),
            Change::RemoteSidebar(remote) => {
                if !self.visibility.sidebar_remotes.remove(&remote) {
                    self.visibility.sidebar_remotes.insert(remote);
                }
                self.visibility.store(ctx, self.repo_key());
                return;
            }
        }
        self.search.retire();
        self.visibility.store(ctx, self.repo_key());
        self.reload(ctx);
    }

    /// Takes what is kept for the repository, now that the worktrees read tell which one it is,
    /// with `log_from` what the history just read was logged from.
    pub(super) fn take_visibility(&mut self, log_from: Option<Vec<String>>, ctx: &egui::Context) {
        if self.log_from != log_from {
            self.search.retire();
        }
        let mut visibility = Visibility::load(ctx, self.repo_key());
        if visibility.prune(&self.refs) {
            visibility.store(ctx, self.repo_key());
        }
        // Changed while it was read, or read before the repository was known.
        if visibility.revisions(&self.refs) != log_from {
            self.reload_again = true;
        }
        self.visibility = visibility;
        self.log_from = log_from;
    }
}

/// Every branch, local and remote, by its full ref name, and whether it is checked out.
fn branch_refs(refs: &Refs) -> impl Iterator<Item = (String, bool)> + '_ {
    let local = refs.local.iter().map(|b| (local_ref(&b.name), b.is_head));
    let remote = refs
        .remotes
        .iter()
        .flat_map(|(_, list)| list)
        .map(|b| (remote_ref(&b.name), false));
    local.chain(remote)
}

#[cfg(test)]
mod tests;
