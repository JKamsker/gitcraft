//! Selecting a commit, however it is reached, shows its details and nothing more. A diff
//! opens only from an explicit click on one of the files it lists (`Action::OpenDiff`).
use super::{DiffKey, RepoTab};

impl RepoTab {
    /// Graph rows, branch tips and commit links reveal details without opening a file diff.
    pub(super) fn select_graph_row(&mut self, row: usize, branch_tip: bool, ctx: &egui::Context) {
        let Some(commit) = self.commits.get(row) else {
            return;
        };
        let selected_hash = commit.hash.clone();
        self.leave_pages();
        self.doctor = None;
        // Only branch tips discard another commit's explicit diff; ordinary rows keep its
        // place for back/forward navigation.
        if branch_tip
            && self.diff.as_ref().is_some_and(|view| {
                matches!(&view.key, DiffKey::Commit { hash, .. } | DiffKey::Remerge { hash, .. } if hash != &selected_hash)
            })
        {
            self.diff = None;
        }
        self.select(row, ctx);
        reveal_details(ctx);
        self.hint_after_select(row, ctx);
    }

    /// A backlink: shows the linked commit's details (even outside a filtered or bounded
    /// graph), where its changed files can be clicked. It never opens a diff by itself.
    pub(super) fn open_commit_changes(&mut self, hash: String, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() || self.reword.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-0392").into(),
                true,
            )));
            return;
        }
        self.leave_pages();
        self.doctor = None;
        self.nav_jump = true;
        self.pending_goto = None;
        self.diff = None;
        self.file_view = None;
        self.compare = None;
        // Works outside a filtered/bounded graph, without loading the entire history.
        self.activity_commit(hash, ctx);
        reveal_details(ctx);
    }
}

/// Shows the detail panel, with priority over the graph even where it does not fit, just as
/// an explicit reopen of the folded sidebar does.
fn reveal_details(ctx: &egui::Context) {
    crate::settings::Settings::set_show_detail(ctx, true);
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("repo_detail").with("reopen-small"), true);
    });
}

#[cfg(test)]
#[path = "commit_changes/link_tests.rs"]
mod link_tests;

#[cfg(test)]
#[path = "commit_changes/selection_tests.rs"]
mod selection_tests;
