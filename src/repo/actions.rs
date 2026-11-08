//! What the views ask the tab to do, and the actions several views build.

use super::toolbar::reveal_label;
use std::path::PathBuf;

use super::dialogs::StashChoice;
use super::pull::PullMode;
use super::{
    Dialog, DiffKey, Refresh, RepoTab, Undo, branch_visibility, ci_view, diff, doctor, drag_drop,
    file_view, issue_work, ops, partial_stash, pr_review, quick_edit, reflog, timeline, versions,
};
use crate::git::{self, InProgress};
use crate::keymap::{Command, label, written};
use crate::terminal::ShellSpec;

/// Everything the views ask for; applied after drawing so views only borrow the state.
pub(crate) enum Action {
    LoadPromptSession(String),
    ShowPromptHunk(super::prompt_session::Hunk),
    OpenLineDossier(Option<git::investigations::dossier::Seed>),
    CloseLineDossier,
    OpenSymbolBiography(Option<git::investigations::dossier::Seed>),
    CloseSymbolBiography,
    OpenPinnedFileLine(git::investigations::dossier::Seed),
    EarlierVersions,
    DraftEnable(bool),
    DraftSnapshot,
    DraftUndo,
    DraftPreview {
        id: crate::git::drafts::Id,
        path: String,
        index: bool,
    },
    DraftRecover {
        preview: Box<crate::git::drafts::RestorePreview>,
        hunks: Vec<usize>,
    },
    OpenContentRecovery,
    ContentSearch(String),
    ContentInspect(usize, Option<usize>),
    ContentPrepare(crate::git::recovery::Target),
    ContentPerform(std::sync::Arc<crate::git::recovery::Prepared>),
    RepositoryTools(super::repository_tools::Act),
    /// What a key of the tab does (see [`crate::keymap`]), or why it can't now.
    Key(crate::keymap::Command),
    /// Show the changes and put the cursor in the commit summary.
    FocusCommit,
    /// Stop the operation running (one that goes over the network).
    CancelOp,
    /// Open or close the Activity panel.
    ToggleActivity,
    /// Open the Activity panel on an operation's entry (its message clicked in the toolbar).
    ShowActivity(u64),
    /// Turn offline mode on or off.
    SetOffline(bool),
    Select(usize),
    SelectHash(String),
    ActivityCommit(String),
    /// Focus a pull request's branch and compare all of its proposed changes.
    SelectPr(u64),
    SelectStash(usize),
    OpenDiff(DiffKey),
    /// Show a pull request in the big area.
    OpenPr(u64),
    /// Open the worktree of a pull request (proposing `branch`, at `sha`), making it first
    /// when there is none.
    OpenPrWorktree {
        number: u64,
        branch: String,
        sha: String,
    },
    ClosePr,
    ReloadPr,
    /// Something done on the pull request shown.
    Pr(pr_review::PrAct),
    /// Show the builds of a commit in the big area, a run of them first if it's given.
    OpenCi {
        sha: String,
        run: Option<u64>,
    },
    CloseCi,
    /// Something done in the pipeline view.
    Ci(ci_view::CiAct),
    /// Something done in the reflog view (opening it on a reflog, or on the lost commits).
    Reflog(reflog::ReflogAct),
    CloseReflog,
    /// Something asked of the issues assigned to you.
    Issue(issue_work::IssueAct),
    /// Show the repository doctor in the big area, at a section.
    OpenDoctor(doctor::Section),
    CloseDoctor,
    /// Something done in the repository doctor.
    Doctor(doctor::DoctorAct),
    /// Agent tasks and the worktree setup recipe.
    Agent(super::agents::AgentAct),
    /// The bake-off of attempts.
    Bake(super::bakeoff::BakeAct),
    CloseDiff,
    Ops {
        label: String,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
    },
    UndoableOps {
        label: String,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        undo: Undo,
    },
    Checkout(String),
    /// What the detached HEAD's banner and dialog ask for.
    Detached(super::detached::DetachedAct),
    /// Checks out a local branch behind its upstream, then fast-forwards it.
    CheckoutPull(String),
    CheckoutRemote(String),
    Dialog(Dialog),
    Commit,
    Fetch,
    /// Pull the way the settings say.
    Pull,
    PullWith(PullMode),
    Push,
    SubmitPreflight(Box<crate::push_preflight::Plan>),
    FixPreflight(Box<crate::push_preflight::Plan>, String),
    PreflightMessages(Box<crate::push_preflight::Plan>),
    ReloadPreflight(Box<crate::push_preflight::Plan>),
    /// Pull, then push the checked-out branch.
    Sync,
    /// A button of the banner after a refused push.
    PushFlow(super::push::PushAct),
    /// Push without running the `pre-push` hook, this once.
    PushSkippingHooks,
    Reload,
    LoadMore,
    Undo,
    Redo,
    /// Show the timeline (undo tree) in the big area, or close it.
    ToggleTimeline,
    /// Something done in the timeline.
    Timeline(timeline::view::TimelineAct),
    /// Stash everything, some files or what is staged, or save a snapshot.
    Stash(partial_stash::StashRequest),
    /// Pick a working-tree file to stash, or unpick it.
    PickToStash(String),
    ClearStashPicks,
    /// Open a folder (a worktree) in its own tab.
    OpenTab(PathBuf),
    /// Open a worktree the way the settings say: in this tab, or in its own.
    OpenWorktree(PathBuf),
    ToggleTerminal,
    /// Pop a view out into its own window, or bring it back.
    Popout(super::popout::Request),
    /// Collapse or bring back the branches panel.
    ToggleBranches,
    /// Open the branches panel at a section (by its key), or in its filter when `None`.
    OpenBranches(Option<&'static str>),
    /// Hide, show or solo branches in the graph.
    BranchVisibility(branch_visibility::Change),
    /// Read the graph's history in this order (and first parent only, or not).
    GraphOrder(git::LogOrder),
    /// Collapse or bring back the detail panel.
    ToggleDetail,
    NewTerminal {
        shell: Option<ShellSpec>,
        cwd: Option<PathBuf>,
    },
    CloseTerminal(usize),
    SelectTerminal(usize),
    ToggleSearch,
    /// Open the checklist of stale or merged branches to delete.
    SelectStaleBranches,
    SearchStep(i32),
    CreatePr {
        branch: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
        /// Who is asked to review it (`ann`, `acme/payments` for a team).
        reviewers: Vec<String>,
    },
    OpenUrl(String),
    /// Copy a link to a commit, branch, file or lines on the forge, or open it there.
    ForgeLink(super::forge_link::LinkRequest),
    /// Open a path (relative to the repository) with the system's default app.
    OpenPath(PathBuf),
    OpenInEditor(PathBuf),
    /// Show the file of this diff in the external diff tool (Settings → Git).
    OpenInDiffTool(DiffKey),
    /// Resolve this conflicted file in the external merge tool (Settings → Git).
    ResolveInMergeTool(String),
    SetIdentity(String, String),
    /// Takes the repository's own `user.name` / `user.email` out (which of them it sets), so
    /// that the profile's or the global config's apply again.
    ClearIdentity {
        name: bool,
        email: bool,
    },
    /// How diffs show: side by side, without whitespace changes, the whole file.
    SetDiffOptions {
        split: bool,
        ignore_ws: bool,
        whole_file: bool,
    },
    /// The diff algorithm for the open diff's file type (`None`: git's default), kept for
    /// that type; the diff is read again where it was scrolled to.
    SetDiffAlgorithm(Option<git::DiffAlgorithm>),
    /// The open diff of a folded file (a lockfile…) shows its lines.
    UnfoldDiff,
    /// Ask git what `.gitattributes` says about folding these files.
    FoldAttributes(Vec<String>),
    /// Diffs show the whole file as it is after the change, or the changes.
    DiffAsFile(bool),
    /// Markdown files show rendered in diffs, or as text.
    DiffRendered(bool),
    /// Diffs show as this rich view (prose…), for the files it reads, or as their lines.
    DiffRich(Option<diff::rich::RichKind>),
    /// Back from a rich view to the plain diff, at the line numbered `old` before the change
    /// or `new` after it.
    DiffGoToLine {
        old: Option<u32>,
        new: Option<u32>,
    },
    /// Read syntax changes through Difftastic; staging stays in the patch.
    DiffStructural(bool),
    /// Scroll the diff to the next change (`1`) or the previous one (`-1`).
    DiffStep(i32),
    /// Show the two places of a moved block of the diff together (by its index), or stop.
    DiffMoved(Option<usize>),
    /// Scroll the diff to one of its lines (by its index).
    DiffJumpLine(usize),
    /// Show some of the unchanged lines folded in a gap of the diff (by its index).
    DiffUnfold {
        gap: usize,
        how: diff::Unfold,
    },
    /// The commit pane lists every file of the commit, or only those it changed.
    ViewAllFiles(bool),
    /// The commit pane of a merge shows what it resolved, or the files it changed.
    ShowRemerge(bool),
    SaveConflict,
    /// Something done with the Git LFS file locks.
    LfsLocks(super::lfs_locks::LockAct),
    /// The conflict editor's history of the file on each side.
    Archaeology(super::conflict::archaeology::Act),
    /// Something done with the Git LFS storage.
    LfsStorage(super::lfs_storage::StorageAct),
    /// Remembered conflict resolutions (git rerere): the switch, forgetting one.
    Rerere(super::conflict::rerere::RerereAct),
    /// Conflict decisions: reading them for the open editor, sending or fetching them.
    Decisions(super::conflict::decisions::Act),
    /// Something done for the repository's Unity project.
    Unity(super::unity::UnityAct),
    /// Strip notebooks' outputs when committing in this repository, or stop.
    StripNotebookOutputs(bool),
    /// Let Mergiraf solve what it can of the open conflict.
    SolveWithMergiraf,
    /// Preview keeping both versions of a conflicted file that can't be merged.
    KeepBoth(String),
    /// A file's history, blame or contents in the central pane (`rev`: the commit they are at).
    FileView {
        path: String,
        rev: Option<String>,
        tab: file_view::FileTab,
    },
    CloseFileView,
    /// Go back (`-1`) or forward (`1`) through the recent views.
    Navigate(i32),
    /// The history of lines `range` of `path` (numbered as they are `at`), in the file view.
    LineHistory {
        path: String,
        at: crate::git::LinesAt,
        range: crate::git::LineRange,
    },
    /// The part of commit `hash`'s diff that changed lines `lines` of `path` (as they are in
    /// the commit); its whole diff of the file when there are none.
    OpenDiffOfLines {
        hash: String,
        path: String,
        lines: Vec<crate::git::LineRange>,
    },
    /// Undo commit `hash`'s changes to `path` in the working tree.
    RevertFile {
        hash: String,
        path: String,
    },
    /// A file's blame (at `rev`, or HEAD and the working tree), scrolled to `line` (1 is the
    /// first) and marking it.
    BlameAt {
        path: String,
        rev: Option<String>,
        line: usize,
    },
    /// Open the interactive rebase editor for the commits after `onto`.
    /// Gives a commit of the checked-out branch a new message: amending the latest, rebasing
    /// for an older one.
    Reword {
        hash: String,
        message: String,
    },
    /// Have the AI tool review what `branch` adds to `base` (see `ai::review`).
    ReviewBranch {
        branch: String,
        base: String,
    },
    /// Open the editor of a commit's message in the detail pane.
    EditMessage(String),
    /// Save a commit as a patch file, asking where.
    SavePatch {
        hash: String,
        summary: String,
    },
    /// Apply a patch file, asking which.
    ApplyPatch,
    /// Open the menu of what a drop offers.
    DropMenu(drag_drop::DropMenu),
    /// Move or squash a commit of the checked-out branch onto another of it.
    CommitDrop(drag_drop::CommitDrop),
    OpenBundleExport,
    OpenBundleInspect,
    ExportBundle {
        refs: Vec<git::BundleRef>,
        base: String,
    },
    /// Drop or move a commit of the checked-out branch.
    QuickEdit {
        edit: quick_edit::Edit,
        hash: String,
    },
    InteractiveRebase {
        onto: String,
        label: String,
    },
    /// Ask before folding the staged changes into the commit `hash` (once what that rewrites
    /// is read).
    FoldInto(String),
    /// Fold as the dialog confirmed it.
    RunFold(Box<super::fold::Plan>),
    /// Abort the rebase of a fold stopped on a conflict, asked first once it is checked to be
    /// the fold's.
    AbortFold,
    RunRebase,
    ConfirmedRebase(u64),
    AskRebase {
        onto: String,
        label: String,
    },
    CloseRebase,
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
    /// Something done on stacked branches.
    Stack(super::stack::StackAct),
    /// Remember these HTTPS credentials and run the operation that needed them again.
    SignIn(git::Credentials),
    /// Ask before deleting the local tag `name` (at the commit `target`), once what it points
    /// at is known (read in the background for a repository on another machine).
    DeleteTag {
        name: String,
        target: String,
    },
    /// Add `pattern` to `.gitignore`; stop tracking `path` if git tracks it.
    Ignore {
        pattern: String,
        path: String,
    },
    /// Stage, unstage or discard these changed lines of the open diff.
    PickLines {
        pick: diff::Pick,
        lines: std::collections::BTreeSet<usize>,
    },
    /// Compare `base` with `target` (the working tree when `None`) in the right pane.
    Compare {
        base: String,
        base_label: String,
        target: Option<String>,
        target_label: String,
        since_fork: bool,
    },
    /// Show only the target's changes since the two diverged, or every difference.
    CompareSinceFork(bool),
    SwapCompare,
    CloseCompare,
    /// Search commit messages, or what commits changed in files.
    SearchCode(bool),
    /// Run a custom command, or trust a repository's.
    Custom(super::custom_commands::CustomAct),
    /// Start, move on or stop a bisect, or run a command for it.
    Bisect(super::bisect::BisectAct),
    /// Start, cancel or close a parallel bisect.
    ParallelBisect(super::parallel_bisect::ParallelAct),
    /// Something about DCO sign-offs.
    Dco(super::dco::DcoAct),
    /// Something the guide asks for: the first commit, a first-time hint.
    Guide(super::guide::GuideAct),
    /// Open the collision matrix of the worktrees' branches.
    OpenCollisions,
    /// Something about comparing two versions of a series of commits.
    Versions(versions::VersionsAct),
    /// The check of the CODEOWNERS file.
    Owners(super::owners::OwnersAct),
    /// Open the agent fence's window.
    OpenFence,
    /// Open the scope drift window of the worktree at this folder.
    OpenDrift(PathBuf),
    /// Rehearse a rebase or merge, stop it, or do it for real.
    Rehearse(super::rehearse::RehearseAct),
    /// Look for the commits of two branches that fail together, or stop looking.
    Interaction(super::interaction::InteractionAct),
    /// Test commits for the test lane, stop them, show what one printed.
    TestLane(super::test_lane::LaneAct),
    /// Something done on a pull request from a fork.
    Fork(super::forks::ForkAct),
    /// Show only the agent-written commits in the search bar's results, or stop.
    AgentFilter(bool),
    /// Work out how much of a local branch agents wrote.
    BranchAuthorship(String),
    /// Preparing or cutting a release.
    Release(super::release::ReleaseAct),
    /// Starting or finishing a hotfix.
    Hotfix(super::hotfix::HotfixAct),
    /// The panel of what a package will publish.
    Package(super::package::PackageAct),
}

/// Compares two branches, given to git by their full refs (a tag of the same name is not
/// what is meant), with their names as labels.
pub(crate) fn compare_branches_action(
    base: BranchRef<'_>,
    target: BranchRef<'_>,
    since_fork: bool,
) -> Action {
    Action::Compare {
        base: base.full(),
        base_label: base.name.to_owned(),
        target: Some(target.full()),
        target_label: target.name.to_owned(),
        since_fork,
    }
}

/// A branch an action brings in or goes onto: git is given its full ref, as a tag (or the
/// other kind of branch) of the same short name would be what `x` means to it; people see the
/// name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BranchRef<'a> {
    /// `main`, or `origin/main` for a remote-tracking branch.
    pub name: &'a str,
    pub remote: bool,
}

impl<'a> BranchRef<'a> {
    pub(crate) fn local(name: &'a str) -> Self {
        Self {
            name,
            remote: false,
        }
    }

    pub(crate) fn remote(name: &'a str) -> Self {
        Self { name, remote: true }
    }

    /// `refs/heads/main`, `refs/remotes/origin/main`.
    pub(crate) fn full(&self) -> String {
        if self.remote {
            branch_visibility::remote_ref(self.name)
        } else {
            branch_visibility::local_ref(self.name)
        }
    }
}

/// The message git gives merging `branch` into `current` by its short name; given the full
/// ref, it would say `Merge branch 'refs/heads/x'`. Like git, without `into` for `main` and
/// `master`.
pub(crate) fn merge_message(branch: BranchRef<'_>, current: &str) -> String {
    let kind = if branch.remote {
        "remote-tracking branch"
    } else {
        "branch"
    };
    let mut message = format!("Merge {kind} '{}'", branch.name);
    if !matches!(current, "main" | "master") {
        message.push_str(&format!(" into {current}"));
    }
    message
}

pub(crate) fn merge_action(branch: BranchRef<'_>, current: &str) -> Action {
    let message = merge_message(branch, current);
    Action::UndoableOps {
        label: format!("Merge {} into {current}", branch.name),
        ops: ops(&[&["merge", "--no-edit", "-m", &message, &branch.full()]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Moves the current branch forward to `branch`; fails (and says so) if they have diverged.
pub(crate) fn fast_forward_action(branch: BranchRef<'_>, current: &str) -> Action {
    Action::UndoableOps {
        label: format!("Fast-forward {current} to {}", branch.name),
        ops: ops(&[&["merge", "--ff-only", &branch.full()]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

pub(crate) fn rebase_action(onto: BranchRef<'_>, current: &str) -> Action {
    Action::AskRebase {
        onto: onto.full(),
        label: format!("Rebase {current} onto {}", onto.name),
    }
}

/// Cherry-picks the commit `hash` onto what is checked out.
pub(crate) fn cherry_pick_action(hash: &str) -> Action {
    Action::UndoableOps {
        label: format!("Cherry-pick {}", super::short(hash)),
        ops: ops(&[&["cherry-pick", hash]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Opens the interactive rebase editor for the checked-out branch onto `onto`.
pub(crate) fn interactive_rebase_action(onto: BranchRef<'_>) -> Action {
    Action::InteractiveRebase {
        onto: onto.full(),
        label: onto.name.to_owned(),
    }
}

/// Asks before aborting: the conflicts resolved so far (in the index only) are thrown away, with
/// nothing to undo it.
pub(super) fn abort_action(state: InProgress) -> Action {
    let what = state.label().to_lowercase();
    Action::Dialog(Dialog::confirm(
        &format!("Abort {what}"),
        format!(
            "Stop the {what} and go back to where it started? Conflicts resolved so far are \
             lost, and can't be brought back."
        ),
        "Abort",
        ops(&[&[state.command(), "--abort"]]),
        None,
    ))
}

impl RepoTab {
    /// What the command palette offers for this repository.
    pub(crate) fn additional_palette_items(&self) -> Vec<(String, String, Action)> {
        let mut items: Vec<(String, String, Action)> = vec![
            ("Fetch".into(), label(Command::Fetch), Action::Fetch),
            ("Pull".into(), label(Command::Pull), Action::Pull),
            (
                if self.gerrit.is_some() {
                    "Push for review"
                } else {
                    "Push"
                }
                .into(),
                label(Command::Push),
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
            ("Undo".into(), written("Cmd+Z"), Action::Undo),
            ("Redo".into(), written("Cmd+Y"), Action::Redo),
            (
                "Timeline and undo tree".into(),
                "Ctrl+Shift+Z".into(),
                Action::ToggleTimeline,
            ),
            (
                "Stash changes…".into(),
                label(Command::Stash),
                Action::Dialog(Dialog::stash()),
            ),
            (
                "Stage all changes".into(),
                label(Command::StageAll),
                Action::Key(Command::StageAll),
            ),
            (
                "Commit".into(),
                label(Command::Commit),
                Action::Key(Command::Commit),
            ),
            (
                "Search commits".into(),
                written("Cmd+F"),
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
            (
                "Toggle terminal".into(),
                label(Command::Terminal),
                Action::ToggleTerminal,
            ),
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
            (
                "New terminal".into(),
                "terminal".into(),
                Action::NewTerminal {
                    shell: None,
                    cwd: None,
                },
            ),
            ("Reload".into(), label(Command::Reload), Action::Reload),
            (
                "Select stale or merged branches…".into(),
                "branch".into(),
                Action::SelectStaleBranches,
            ),
            (
                "Conflicts that can wait: isolated rebase and restack…".into(),
                "Deferred rewrites".into(),
                Action::RepositoryTools(super::repository_tools::Act::Deferred),
            ),
            (
                "Parallel branches in this folder: hunk ownership…".into(),
                "Virtual branches".into(),
                Action::RepositoryTools(super::repository_tools::Act::Virtuals),
            ),
            (
                "Named changelists: messages, commit and shelve…".into(),
                "Working changes".into(),
                Action::RepositoryTools(super::repository_tools::Act::Lists),
            ),
            (
                "Push outbox: queue exact commit or send selected entries…".into(),
                "Offline publication".into(),
                Action::RepositoryTools(super::repository_tools::Act::Outbox),
            ),
            (
                "Shared and borrowed objects".into(),
                "Inspect shared Git directories and donors; make a clone independent".into(),
                Action::RepositoryTools(super::repository_tools::Act::Sharing),
            ),
            (
                "Publish submodule dependencies before parent…".into(),
                "submodules publish remote".into(),
                Action::RepositoryTools(super::repository_tools::Act::Dependencies),
            ),
            (
                "Pull and push routes…".into(),
                "triangular upstream ahead behind".into(),
                Action::RepositoryTools(super::repository_tools::Act::Routes(String::new())),
            ),
            (
                "Publish to several hosts…".into(),
                "publication group retry mirrors".into(),
                Action::RepositoryTools(super::repository_tools::Act::Publication(String::new())),
            ),
            (
                "Remote URLs, refspecs and mirror policy…".into(),
                "fetch push mapping prune".into(),
                Action::RepositoryTools(super::repository_tools::Act::Remote(String::new())),
            ),
            (
                "Replace refs and temporary history joins…".into(),
                "grafts original effective history".into(),
                Action::RepositoryTools(super::repository_tools::Act::Replacements),
            ),
            (
                "Ref storage and reftable migration…".into(),
                "refs compatibility".into(),
                Action::RepositoryTools(super::repository_tools::Act::Reftable),
            ),
            (
                "How an object is stored…".into(),
                "objects pack delta file version".into(),
                Action::RepositoryTools(super::repository_tools::Act::Object("HEAD".into())),
            ),
            (
                "All refs and namespace history…".into(),
                "refs internals".into(),
                Action::RepositoryTools(super::repository_tools::Act::AllRefs(String::new())),
            ),
            (
                "Repository doctor".into(),
                "doctor".into(),
                Action::OpenDoctor(doctor::Section::ALL[0]),
            ),
            (
                "Start agent task…".into(),
                "agent".into(),
                Action::Agent(super::agents::AgentAct::OpenStart),
            ),
            (
                "Bake-off: compare attempts…".into(),
                "agent".into(),
                Action::Bake(super::bakeoff::BakeAct::OpenPicker),
            ),
            (
                "Agent turns: save each turn in this worktree".into(),
                "agent".into(),
                Action::Agent(super::agents::AgentAct::InstallHooks),
            ),
            (
                "Worktree setup…".into(),
                "agent".into(),
                Action::Agent(super::agents::AgentAct::OpenRecipe),
            ),
            (
                "What a package will publish".into(),
                "cargo / npm".into(),
                Action::Package(super::package::PackageAct::Open),
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
        items.extend(super::popout::palette_items());
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
        if self.owners.file().is_some() {
            items.push((
                "Check CODEOWNERS".into(),
                "owners".into(),
                Action::Owners(super::owners::OwnersAct::Check),
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
                Action::Rerere(super::conflict::rerere::RerereAct::SetEnabled(!on)),
            ));
        }
        items.extend(self.decision_palette_items());
        let unused = self.forks.unused().len();
        if unused > 0 {
            items.push((
                format!("Remove unused fork remotes ({unused})"),
                "remote".into(),
                Action::Fork(super::forks::ForkAct::Tidy),
            ));
        }
        if !self.submodules.is_empty() {
            items.push((
                "Update submodules".into(),
                "submodule".into(),
                super::keep_submodules::update_all(),
            ));
        }
        if self.unity.project().is_some() {
            items.push((
                "Unity: Set-up…".into(),
                "settings".into(),
                Action::Unity(super::unity::UnityAct::OpenPanel),
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
                Action::LfsLocks(super::lfs_locks::LockAct::OpenPanel),
            ));
            items.push((
                "LFS: Storage…".into(),
                "remote".into(),
                Action::LfsStorage(super::lfs_storage::StorageAct::OpenPanel),
            ));
        }
        if self.hosting.is_some() {
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
                Action::Hotfix(super::hotfix::HotfixAct::OpenFinish),
            ));
        }
        if !self.refs.head.is_empty() {
            items.push((
                "Cut release…".into(),
                "release".into(),
                Action::Release(super::release::ReleaseAct::Open(self.refs.head.clone())),
            ));
            items.push((
                "Create branch".into(),
                label(Command::NewBranch),
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
