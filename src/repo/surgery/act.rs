//! What history surgery is asked: the split, move and absorb editors. The root
//! [`Action`](crate::repo::Action) routes these here in one arm.

use crate::repo::actions::PaletteContribution;
use crate::repo::{Action, RepoTab};

pub(in crate::repo) enum SurgeryAct {
    /// Open the split editor on a commit of the checked-out branch.
    OpenSplit(String),
    /// Open the editor moving some changes of a commit to another one, or out of the branch.
    OpenMove(String),
    /// Prepare the split being edited and show each new commit's patch.
    CheckSplit,
    RunSplit,
    CloseSplit,
    /// Open the editor distributing the staged changes into the commits they belong to.
    OpenAbsorb,
    CheckAbsorb,
    RunAbsorb,
    /// Run the installed `git-absorb` instead.
    RunGitAbsorb,
    CloseAbsorb,
}

/// The palette's entry for distributing staged fixes, at its slot after the snapshots.
pub(in crate::repo) fn absorb_palette_item() -> PaletteContribution {
    (
        "Distribute staged fixes…".into(),
        "absorb".into(),
        Action::Surgery(SurgeryAct::OpenAbsorb),
    )
}

impl RepoTab {
    pub(in crate::repo) fn surgery_act(&mut self, act: SurgeryAct, ctx: &egui::Context) {
        match act {
            SurgeryAct::OpenSplit(hash) => self.open_split(hash, ctx),
            SurgeryAct::OpenMove(hash) => self.open_move(hash, ctx),
            SurgeryAct::CheckSplit => self.check_split(ctx),
            SurgeryAct::RunSplit => self.run_split(ctx),
            SurgeryAct::CloseSplit => self.split = None,
            SurgeryAct::OpenAbsorb => self.open_absorb(ctx),
            SurgeryAct::CheckAbsorb => self.check_absorb(ctx),
            SurgeryAct::RunAbsorb => self.run_absorb(ctx),
            SurgeryAct::RunGitAbsorb => self.run_git_absorb(ctx),
            SurgeryAct::CloseAbsorb => self.absorb = None,
        }
    }
}
