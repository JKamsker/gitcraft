//! A commit target owns its browsing/restoration intent through graph paging.
use crate::repo::{
    Action, MAX_HISTORY, RepoTab,
    bakeoff::BakeAct,
    insights::InsightsAct,
    reflog::ReflogAct,
    resolver::ResolverAct,
    review_workflow::{ReviewAct, StoryAct},
    versions::VersionsAct,
};

#[derive(Clone)]
pub(in crate::repo) enum PendingGoto {
    History(String),
    Restore(String),
}

impl PendingGoto {
    pub(in crate::repo) fn hash(&self) -> &str {
        match self {
            Self::History(hash) | Self::Restore(hash) => hash,
        }
    }

    pub(in crate::repo) fn restoring(&self) -> bool {
        matches!(self, Self::Restore(_))
    }
}

impl RepoTab {
    pub(in crate::repo) fn retire_navigation_restore(&mut self, action: &Action) {
        if self
            .pending_goto
            .as_ref()
            .is_some_and(PendingGoto::restoring)
            && matches!(
                action,
                Action::Select(_)
                    | Action::SelectBranchTip(_)
                    | Action::SelectHash(_)
                    | Action::SelectStash(_)
                    | Action::SelectPr(_)
                    | Action::FocusCommit
                    | Action::ActivityCommit(_)
                    | Action::OpenCommitChanges(_)
                    | Action::OpenDiff(_)
                    | Action::OpenDiffOfLines { .. }
                    | Action::CloseDiff
                    | Action::FileView { .. }
                    | Action::LineHistory { .. }
                    | Action::BlameAt { .. }
                    | Action::OpenPinnedFileLine(_)
                    | Action::OpenLineDossier(_)
                    | Action::OpenSymbolBiography(_)
                    | Action::OpenPr(_)
                    | Action::OpenCi { .. }
                    | Action::Insights(InsightsAct::Open(_))
                    | Action::OpenDoctor(_)
                    | Action::OpenTour
                    | Action::OpenFileComparison(_)
                    | Action::Compare { .. }
                    | Action::Story(StoryAct::OpenCode { .. })
                    | Action::Review(ReviewAct::Open(_))
                    | Action::OpenSplit(_)
                    | Action::OpenMove(_)
                    | Action::OpenAbsorb
                    | Action::InteractiveRebase { .. }
                    | Action::Versions(
                        VersionsAct::Rewrite(_)
                            | VersionsAct::Branches { .. }
                            | VersionsAct::PrSince(_)
                    )
                    | Action::Bake(BakeAct::Compare(_))
                    | Action::Resolver(ResolverAct::Review(_))
                    | Action::ToggleTimeline
                    | Action::Reflog(
                        ReflogAct::Show(_) | ReflogAct::FindLost | ReflogAct::Select(_)
                    )
                    | Action::ComparisonNormalPatch
                    | Action::Navigate(_)
            )
        {
            self.pending_goto = None;
        }
    }

    pub(in crate::repo) fn select_hash(&mut self, hash: &str, ctx: &egui::Context) {
        self.select_target(&PendingGoto::History(hash.to_owned()), ctx);
    }

    fn select_target(&mut self, target: &PendingGoto, ctx: &egui::Context) {
        let hash = target.hash();
        match self.index.get(hash).copied() {
            Some(row) => {
                if target.restoring() {
                    self.restore_commit_selection(row, ctx);
                } else {
                    self.select_history(row, ctx);
                }
                self.scroll_to = Some(row);
            }
            None => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2900",
                        &[("hash", hash.get(..8).unwrap_or(hash).to_string().into())],
                    ),
                    true,
                )));
            }
        }
    }

    /// Selects `hash`, loading more history first if it's further down.
    pub(in crate::repo) fn goto(&mut self, hash: &str, ctx: &egui::Context) {
        self.goto_target(PendingGoto::History(hash.to_owned()), ctx);
    }

    pub(in crate::repo) fn goto_target(&mut self, target: PendingGoto, ctx: &egui::Context) {
        let hash = target.hash().to_owned();
        if self.sleeping() || self.resuming {
            self.pending_goto = Some(target);
            return;
        }
        if !self.index.contains_key(&hash) && self.history.iter().any(|c| c.hash == hash) {
            self.history_filter.revealed.insert(hash.clone());
            let previous = self.selected_hash();
            self.rebuild_rows();
            self.selected = previous.and_then(|h| self.index.get(&h).copied());
        }
        if self.index.contains_key(&hash) {
            self.pending_goto = None;
            self.select_target(&target, ctx);
            self.history_filter.revealed.remove(&hash);
        } else if self.has_more && self.history.len() < MAX_HISTORY {
            self.pending_goto = Some(target);
            self.load_more(ctx);
        } else {
            self.pending_goto = None;
            self.select_target(&target, ctx);
        }
    }
}

#[cfg(test)]
mod tests;
