//! Insights read ownership follows both the forge preference and the repository namespace.
use super::{Kind, RepoTab};
#[cfg(test)]
pub(super) mod namespace_tests;

#[derive(PartialEq, Eq)]
pub(super) struct Namespace {
    path: std::path::PathBuf,
    connection: crate::connections::Context,
    generation: u64,
}

impl Namespace {
    pub(super) fn capture(tab: &RepoTab) -> Self {
        Self {
            path: tab.path.clone(),
            connection: crate::connections::current(),
            generation: crate::connections::generation(),
        }
    }

    pub(super) fn current(&self, tab: &RepoTab) -> bool {
        self.generation != u64::MAX && *self == Self::capture(tab)
    }
}

/// Only this page's current repository lifetime may run a read continuation.
pub(super) struct Read {
    namespace: Namespace,
    lifetime: std::sync::Weak<()>,
}

impl Read {
    pub(super) fn capture(tab: &RepoTab) -> Self {
        Self {
            namespace: Namespace::capture(tab),
            lifetime: tab
                .insights
                .as_ref()
                .map_or_else(std::sync::Weak::new, |view| {
                    std::sync::Arc::downgrade(&view.read_lifetime)
                }),
        }
    }

    pub(super) fn current(&self, tab: &RepoTab) -> bool {
        self.namespace.current(tab)
            && self
                .lifetime
                .upgrade()
                .zip(tab.insights.as_ref())
                .is_some_and(|(old, view)| std::sync::Arc::ptr_eq(&old, &view.read_lifetime))
    }
}

impl RepoTab {
    pub(in crate::repo) fn invalidate_forge_insights(&mut self) {
        if let Some(view) = &mut self.insights {
            view.ci_path = Default::default();
            view.deploy = Default::default();
            view.review_heat = Default::default();
            view.pr = None;
            view.loaded.remove(&Kind::CiPath);
            view.loaded.remove(&Kind::Deploy);
            view.loaded.remove(&Kind::ReviewHeat);
            view.constellation.invalidate_forge();
            if view.impact.invalidate_forge() {
                view.loaded.remove(&Kind::Impact);
            }
            if view.transform.invalidate_forge() {
                view.loaded.remove(&Kind::Transform);
            }
        }
    }

    /// Account/transport, worktree and suspend transitions retire every repository read.
    pub(in crate::repo) fn invalidate_repository_insights(&mut self) {
        self.invalidate_forge_insights();
        if let Some(view) = &mut self.insights {
            view.read_lifetime = std::sync::Arc::new(());
            view.loaded.clear();
            view.constellation = Default::default();
            view.conflict_radar = Default::default();
            view.impact.retire_repository();
            view.transform.retire_repository();
            view.evolution.retire_repository();
            view.churn.retire_repository();
            view.coupling.retire_repository();
            view.coverage.retire_repository();
            view.ribbons.retire_repository();
        }
    }

    /// Observe shared A→B→A preferences and route changes before reads, replies or painting.
    pub(in crate::repo) fn sync_insights_context(&mut self, ctx: &egui::Context) {
        let (_, epoch) = super::super::forge_selection::lifetime(ctx, self.repo_key());
        let namespace = Namespace::capture(self);
        let Some(view) = &self.insights else { return };
        let changed_namespace = view.namespace.as_ref().is_some_and(|old| *old != namespace);
        let changed_forge = view.forge_epoch.is_some_and(|old| old != epoch);
        if changed_namespace {
            self.invalidate_repository_insights();
        } else if changed_forge {
            self.invalidate_forge_insights();
        }
        if let Some(view) = &mut self.insights {
            view.forge_epoch = Some(epoch);
            view.namespace = Some(namespace);
        }
    }
}
