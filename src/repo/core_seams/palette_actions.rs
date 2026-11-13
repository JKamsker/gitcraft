use super::*;
use crate::repo::toolbar::reveal_label;

/// A feature contributes an intent with its displayed label and searchable hint.
pub(crate) type PaletteContribution = (String, String, Action);

impl RepoTab {
    /// What the command palette offers for this repository.
    pub(crate) fn additional_palette_items(&self, ctx: &egui::Context) -> Vec<PaletteContribution> {
        let mut items = vec![(
            "DVC: Pull".into(),
            "data download remote".into(),
            Action::DvcPull,
        )];
        items.extend(crate::repo::file_comparison::palette::items());
        items.extend(crate::repo::push::palette::items(
            ctx,
            self.gerrit.is_some(),
        ));
        items.extend([
            (
                "Behaviour timeline…".into(),
                "command output benchmark screenshot".into(),
                Action::OpenBehaviour,
            ),
            (
                "Linked project copies…".into(),
                "machines".into(),
                Action::OpenLinkedMachines,
            ),
            (
                "Export Git bundle…".into(),
                "offline backup".into(),
                Action::OpenBundleExport,
            ),
            (
                "Inspect local Git bundle…".into(),
                "offline exchange".into(),
                Action::OpenBundleInspect,
            ),
        ]);
        items.extend(crate::repo::partial_stash::palette::snapshots());
        items.push((
            "Distribute staged fixes…".into(),
            "absorb".into(),
            Action::OpenAbsorb,
        ));
        items.extend(crate::repo::timeline::palette::items(ctx));
        items.extend(crate::repo::partial_stash::palette::stash(ctx));
        items.extend([
            (
                "Stage all changes".into(),
                label(ctx, Command::StageAll),
                Action::Key(Command::StageAll),
            ),
            (
                "Commit".into(),
                label(ctx, Command::Commit),
                Action::Key(Command::Commit),
            ),
            (
                "Search commits".into(),
                label(ctx, Command::Search),
                Action::ToggleSearch,
            ),
        ]);
        items.extend(crate::repo::reflog::palette::items());
        items.push(crate::repo::terminals::toggle_palette_item(ctx));
        items.push(crate::repo::offline::palette_item(
            crate::network::refused().is_some(),
        ));
        items.extend([
            (
                "Activity: operations and their output".into(),
                "log".into(),
                Action::ToggleActivity,
            ),
            crate::repo::terminals::new_palette_item(),
            ("Reload".into(), label(ctx, Command::Reload), Action::Reload),
            (
                "Select stale or merged branches…".into(),
                "branch".into(),
                Action::Cleanup(crate::repo::branch_cleanup::CleanupAct::Branches),
            ),
            (
                "Clean up worktrees of merged branches…".into(),
                "worktree".into(),
                Action::Cleanup(crate::repo::branch_cleanup::CleanupAct::Worktrees),
            ),
        ]);
        items.extend(crate::repo::repository_tools::palette::items());
        items.extend([
            (
                "Repository doctor".into(),
                "doctor".into(),
                Action::OpenDoctor(doctor::Section::ALL[0]),
            ),
            (
                "Start agent task…".into(),
                "agent".into(),
                Action::Agent(crate::repo::agents::AgentAct::OpenStart),
            ),
            (
                "Bake-off: compare attempts…".into(),
                "agent".into(),
                Action::Bake(crate::repo::bakeoff::BakeAct::OpenPicker),
            ),
            (
                "Agent turns: save each turn in this worktree".into(),
                "agent".into(),
                Action::Agent(crate::repo::agents::AgentAct::InstallHooks),
            ),
            (
                "Worktree setup…".into(),
                "agent".into(),
                Action::Agent(crate::repo::agents::AgentAct::OpenRecipe),
            ),
            (
                "What a package will publish".into(),
                "cargo / npm".into(),
                Action::Package(crate::repo::package::PackageAct::Open),
            ),
            (
                reveal_label().into(),
                "folder".into(),
                Action::OpenPath(PathBuf::new()),
            ),
            (
                "Open in editor".into(),
                "folder".into(),
                Action::OpenInEditor(PathBuf::new()),
            ),
        ]);
        items.extend(crate::repo::popout::palette_items());
        for section in doctor::Section::ALL {
            items.push((
                format!("Doctor: {}", section.title()),
                "doctor".into(),
                Action::OpenDoctor(section),
            ));
        }
        if let Some(url) = self.web_page() {
            items.push((
                "Open in browser".into(),
                "remote".into(),
                Action::OpenUrl(url),
            ));
        }
        items.extend(self.versions_palette_item());
        items.extend(self.insights_palette_items());
        if self.owners.file().is_some() {
            items.push((
                "Check CODEOWNERS".into(),
                "owners".into(),
                Action::Owners(crate::repo::owners::OwnersAct::Check),
            ));
        }
        if let Some(on) = self.rerere.enabled() {
            items.push((
                if on {
                    "Stop remembering conflict resolutions (rerere)"
                } else {
                    "Remember conflict resolutions (rerere)"
                }
                .into(),
                "conflicts".into(),
                Action::Rerere(crate::repo::conflict::rerere::RerereAct::SetEnabled(!on)),
            ));
        }
        items.extend(self.decision_palette_items());
        let unused = self.forks.unused().len();
        if unused > 0 {
            items.push((
                format!("Remove unused fork remotes ({unused})"),
                "remote".into(),
                Action::Fork(crate::repo::forks::ForkAct::Tidy),
            ));
        }
        if !self.submodules.is_empty() {
            items.push((
                "Update submodules".into(),
                "submodule".into(),
                crate::repo::keep_submodules::update_all(),
            ));
        }
        if self.unity.project().is_some() {
            items.push((
                "Unity: Set-up…".into(),
                "settings".into(),
                Action::Unity(crate::repo::unity::UnityAct::OpenPanel),
            ));
        }
        if self.lfs {
            for (label, args) in [
                ("LFS: Fetch", ["lfs", "fetch"]),
                ("LFS: Pull", ["lfs", "pull"]),
            ] {
                items.push((
                    label.into(),
                    "remote".into(),
                    Action::Ops {
                        label: label.into(),
                        ops: ops(&[&args]),
                        refresh: Refresh::All,
                    },
                ));
            }
            items.push((
                "LFS: Locks…".into(),
                "remote".into(),
                Action::LfsLocks(crate::repo::lfs_locks::LockAct::OpenPanel),
            ));
            items.push((
                "LFS: Storage…".into(),
                "remote".into(),
                Action::LfsStorage(crate::repo::lfs_storage::StorageAct::OpenPanel),
            ));
        }
        {
            items.push((
                "Start work from an issue…".into(),
                "issue".into(),
                Action::Issue(issue_work::IssueAct::OpenPicker),
            ));
        }
        if !self.refs.head.is_empty() && self.ci_project().is_some() {
            items.push((
                "Show CI for HEAD".into(),
                "builds".into(),
                Action::OpenCi {
                    sha: self.refs.head.clone(),
                    run: None,
                },
            ));
        }
        if self
            .refs
            .current
            .as_deref()
            .is_some_and(|b| b.starts_with(&self.hotfix_prefix()))
        {
            items.push((
                "Finish hotfix…".into(),
                "release".into(),
                Action::Hotfix(crate::repo::hotfix::HotfixAct::OpenFinish),
            ));
        }
        if !self.refs.head.is_empty() {
            items.push((
                "Cut release…".into(),
                "release".into(),
                Action::Release(crate::repo::release::ReleaseAct::Open(
                    self.refs.head.clone(),
                )),
            ));
            items.push((
                "Create branch".into(),
                label(ctx, Command::NewBranch),
                Action::Dialog(Dialog::create_branch(&self.refs.head, "HEAD")),
            ));
            items.push((
                "Create tag".into(),
                "tag".into(),
                Action::Dialog(Dialog::tag(&self.refs.head, "HEAD", self.default_remote())),
            ));
        }
        items.extend(self.bisect_palette_items());
        items.extend(self.parallel_palette_items());
        items.extend(self.test_palette_items());
        items.extend(self.rehearse_palette_items());
        items.extend(self.custom_palette_items());
        items
    }
}

#[cfg(test)]
#[path = "palette_contexts.rs"]
mod palette_contexts;
#[cfg(test)]
#[path = "palette_contracts.rs"]
mod palette_contracts;

#[cfg(test)]
#[path = "palette_shortcuts.rs"]
mod palette_shortcuts;
