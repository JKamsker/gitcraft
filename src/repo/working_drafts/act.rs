//! What the working-tree drafts are asked: switching them, a snapshot, undoing one, a
//! preview and recovering from it. The root [`Action`](crate::repo::Action) routes these
//! here in one arm.

use super::store;
use crate::repo::RepoTab;

pub(in crate::repo) enum DraftAct {
    EarlierVersions,
    Enable(bool),
    Snapshot,
    Undo,
    Preview {
        id: store::Id,
        path: String,
        index: bool,
    },
    Recover {
        preview: Box<store::RestorePreview>,
        hunks: Vec<usize>,
    },
}

impl RepoTab {
    pub(in crate::repo) fn draft_act(&mut self, act: DraftAct, ctx: &egui::Context) {
        match act {
            DraftAct::EarlierVersions => self.earlier_versions(ctx),
            DraftAct::Enable(on) => self.enable_working_drafts(on, ctx),
            DraftAct::Snapshot => self.save_working_draft(false, ctx),
            DraftAct::Undo => self.undo_working_draft(ctx),
            DraftAct::Preview { id, path, index } => {
                self.preview_working_draft(id, path, index, ctx)
            }
            DraftAct::Recover { preview, hunks } => {
                self.recover_working_draft(*preview, hunks, ctx)
            }
        }
    }
}
