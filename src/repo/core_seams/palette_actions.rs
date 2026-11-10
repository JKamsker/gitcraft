use super::*;
use crate::repo::toolbar::reveal_label;

impl RepoTab {
    /// What the command palette offers for this repository.
    pub(crate) fn additional_palette_items(
        &self,
        ctx: &egui::Context,
    ) -> Vec<(String, String, Action)> {
        let mut items: Vec<(String, String, Action)> = vec![
            (
                "DVC: Pull".into(),
                "data download remote".into(),
                Action::DvcPull,
            ),
            (
                "Code moved across files".into(),
                "refactor moved block".into(),
                Action::OpenFileComparison(crate::repo::file_comparison::Page::CodeMoves),
            ),
            (
                "File migration map".into(),
                "files".into(),
                Action::OpenFileComparison(crate::repo::file_comparison::Page::Migration),
            ),
            (
                "Three-state staging lens".into(),
                "staging".into(),
                Action::OpenFileComparison(crate::repo::file_comparison::Page::Lens),
            ),
            (
                "File versions across branches".into(),
                "files".into(),
                Action::OpenFileComparison(crate::repo::file_comparison::Page::Matrix),
            ),
            ("Fetch".into(), label(ctx, Command::Fetch), Action::Fetch),
            ("Pull".into(), label(ctx, Command::Pull), Action::Pull),
            (
                if self.gerrit.is_some() {
                    "Push for review"
                } else {
                    "Push"
                }
                .into(),
                label(ctx, Command::Push),
                Action::Push,
            ),
            (
                "Sync (pull, then push)".into(),
                "remote".into(),
                Action::Sync,
            ),
            (
                if self.gerrit.is_some() {
                    "Push for review, skipping hooks"
                } else {
                    "Push, skipping hooks"
                }
                .into(),
                "--no-verify".into(),
                Action::PushSkippingHooks,
            ),
        ];
        if self.gerrit.is_some() {
            items.retain(|(_, _, action)| !matches!(action, Action::Sync));
        }
        items.extend(
            PullMode::ALL.map(|m| (m.label().into(), "remote".into(), Action::PullWith(m))),
        );
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
            (
                "Stash only what is staged…".into(),
                "stash".into(),
                Action::Dialog(Dialog::stash_as(StashChoice::Staged)),
            ),
            (
                "Save snapshot (stash, keep the changes)…".into(),
                "stash".into(),
                Action::Dialog(Dialog::stash_as(StashChoice::Snapshot)),
            ),
            (
                "Distribute staged fixes…".into(),
                "absorb".into(),
                Action::OpenAbsorb,
            ),
            ("Undo".into(), label(ctx, Command::Undo), Action::Undo),
            ("Redo".into(), label(ctx, Command::Redo), Action::Redo),
            (
                "Timeline and undo tree".into(),
                label(ctx, Command::Timeline),
                Action::ToggleTimeline,
            ),
            (
                "Stash changes…".into(),
                label(ctx, Command::Stash),
                Action::Dialog(Dialog::stash()),
            ),
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
            (
                "Reflog".into(),
                "recovery".into(),
                Action::Reflog(reflog::ReflogAct::Show(reflog::Source::head())),
            ),
            (
                "Find lost commits".into(),
                "recovery".into(),
                Action::Reflog(reflog::ReflogAct::FindLost),
            ),
            crate::repo::terminals::toggle_palette_item(ctx),
            if crate::network::refused().is_some() {
                (
                    "Go online".into(),
                    "network offline".into(),
                    Action::SetOffline(false),
                )
            } else {
                (
                    "Go offline".into(),
                    "network offline metered".into(),
                    Action::SetOffline(true),
                )
            },
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
                Action::SelectStaleBranches,
            ),
            (
                "Conflicts that can wait: isolated rebase and restack…".into(),
                "Deferred rewrites".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Deferred),
            ),
            (
                "Parallel branches in this folder: hunk ownership…".into(),
                "Virtual branches".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Virtuals),
            ),
            (
                "Inspect state publication receipts…".into(),
                "Working-state transaction recovery".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::StateReceipts),
            ),
            (
                "Named changelists: messages, commit and shelve…".into(),
                "Working changes".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Lists),
            ),
            (
                "Push outbox: queue exact commit or send selected entries…".into(),
                "Offline publication".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Outbox),
            ),
            (
                "Subtrees: register, update, inspect and publish vendored folders…".into(),
                "Vendored source history".into(),
                Action::RepositoryTools(
                    crate::repo::repository_tools::Act::Subtrees(String::new()),
                ),
            ),
            (
                "Shared and borrowed objects".into(),
                "Inspect shared Git directories and donors; make a clone independent".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Sharing),
            ),
            (
                "Publish submodule dependencies before parent…".into(),
                "submodules publish remote".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Dependencies),
            ),
            (
                "Pull and push routes…".into(),
                "triangular upstream ahead behind".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Routes(String::new())),
            ),
            (
                "Publish to several hosts…".into(),
                "publication group retry mirrors".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Publication(
                    String::new(),
                )),
            ),
            (
                "Remote URLs, refspecs and mirror policy…".into(),
                "fetch push mapping prune".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Remote(String::new())),
            ),
            (
                "Replace refs and temporary history joins…".into(),
                "grafts original effective history".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Replacements),
            ),
            (
                "Ref storage and reftable migration…".into(),
                "refs compatibility".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Reftable),
            ),
            (
                "How an object is stored…".into(),
                "objects pack delta file version".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::Object("HEAD".into())),
            ),
            (
                "All refs and namespace history…".into(),
                "refs internals".into(),
                Action::RepositoryTools(crate::repo::repository_tools::Act::AllRefs(String::new())),
            ),
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
