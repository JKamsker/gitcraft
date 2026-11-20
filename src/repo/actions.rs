//! What the views ask the tab to do, and the actions several views build.

use std::path::PathBuf;

use super::pull::PullMode;
use super::{
    Dialog, DiffKey, Refresh, RepoTab, Undo, branch_visibility, ci_view, diff, doctor, drag_drop,
    file_view, issue_work, ops, partial_stash, pr_review, quick_edit, reflog, timeline, versions,
};
use crate::git::{self, InProgress};
use crate::keymap::{Command, label};

/// Everything the views ask for; applied after drawing so views only borrow the state.
pub(crate) enum Action {
    LoadPromptSession(String),
    ShowPromptHunk(super::prompt_session::Hunk),
    Keyboard(super::keyboard::Request),
    KeyboardStep(i32),
    FocusPane(super::keyboard::Pane),
    OpenLineDossier(Option<git::investigations::dossier::Seed>),
    CloseLineDossier,
    OpenSymbolBiography(Option<git::investigations::dossier::Seed>),
    CloseSymbolBiography,
    OpenPinnedFileLine(git::investigations::dossier::Seed),
    /// Working-tree drafts: switching them, snapshots, undo, previews and recovery.
    Drafts(super::working_drafts::DraftAct),
    OpenContentRecovery,
    ContentSearch(String),
    ContentInspect(usize, Option<usize>),
    ContentPrepare(crate::git::recovery::Target),
    ContentPerform(std::sync::Arc<crate::git::recovery::Prepared>),
    RepositoryTools(super::repository_tools::Act),
    /// What the background resolving run is asked (see `resolver`).
    Resolver(super::resolver::ResolverAct),
    /// What a key of the tab does (see [`crate::keymap`]), or why it can't now.
    Key(crate::keymap::Command),
    /// A read-only clipboard payload resolved from the current selected commit.
    CopyText(String),
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
    SelectBranchTip(usize),
    DvcPull,
    /// Something done in the raw file comparison panes.
    Comparison(super::file_comparison::ComparisonAct),
    SelectHash(String),
    /// Select a branch's tip commit by hash: details only, never a file diff.
    SelectBranch(String),
    /// Follow a backlink to the commit's details and changed files (never a file diff).
    OpenCommitChanges(String),
    ActivityCommit(String),
    /// Focus a pull request's branch and compare all of its proposed changes.
    SelectPr(u64),
    SelectStash(usize),
    OpenDiff(DiffKey),
    Review(super::review_workflow::ReviewAct),
    Freshness(super::review_workflow::FreshnessAct),
    Story(super::review_workflow::StoryAct),
    ReviewTools(super::review_workflow::ToolsAct),
    /// Show a pull request in the big area.
    OpenPr(u64),
    /// Select a linked forge remote before opening its pull request.
    OpenLinkedPr {
        number: u64,
        remote: String,
    },
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
    LocalCi(super::local_ci::Act),
    Email(super::email::Act),
    Devcontainer(super::devcontainer::Act),
    /// Something done in the reflog view (opening it on a reflog, or on the lost commits).
    Reflog(reflog::ReflogAct),
    CloseReflog,
    /// Something asked of the issues assigned to you.
    Issue(issue_work::IssueAct),
    /// Shows the pull requests read while the list was in use.
    ApplyPendingPrs,
    /// Show the repository doctor in the big area, at a section.
    OpenDoctor(doctor::Section),
    CloseDoctor,
    OpenTour,
    OpenSshSetup,
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
    /// Run a refused checkout again, carrying the files that were in its way over.
    CarryCheckout(Box<super::checkout_blocked::BlockedCheckout>),
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
    Terminal(super::terminals::TerminalAct),
    /// Pop a view out into its own window, or bring it back.
    Popout(super::popout::Request),
    /// Collapse or bring back the branches panel.
    ToggleBranches,
    /// Open the branches panel at a section (by its key), or in its filter when `None`.
    OpenBranches(Option<&'static str>),
    /// Hide, show or solo branches in the graph.
    BranchVisibility(branch_visibility::Change),
    PrimaryForge(Option<String>),
    /// Read the graph's history in this order (and first parent only, or not).
    GraphOrder(git::LogOrder),
    /// Collapse or bring back the detail panel.
    ToggleDetail,
    ToggleSearch,
    FocusSearch,
    /// Open the checklist of stale or merged branches, or the worktrees of merged ones.
    Cleanup(super::dialogs::CleanupAct),
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
    /// The commit pane lists every file of the commit, or only those it changed.
    /// How the open diff shows, and moving around in it.
    Diff(diff::DiffAct),
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
    /// Go back (`-1`) or forward (`1`) through the recent views.
    /// A file's view in the central pane: its history, blame or contents, or line history.
    FileView(file_view::FileViewAct),
    Navigate(i32),
    /// Undo commit `hash`'s changes to `path` in the working tree.
    RevertFile {
        hash: String,
        path: String,
    },
    /// Have the AI tool review what `branch` adds to `base` (see `ai::review`).
    ReviewBranch {
        branch: String,
        base: String,
    },
    OpenBehaviour,
    OpenLinkedMachines,
    OpenBundleExport,
    OpenBundleInspect,
    ExportBundle {
        refs: Vec<git::BundleRef>,
        base: String,
    },
    /// Something done on stacked branches.
    /// Rewriting the checked-out branch: rewording, rebasing, folding, dropping or moving
    /// commits, and patches.
    Rewrite(super::rewrite::RewriteAct),
    /// History surgery: splitting a commit, moving changes, distributing staged fixes.
    Surgery(super::surgery::SurgeryAct),
    Stack(super::stack::StackAct),
    /// Remember these HTTPS credentials and run the operation that needed them again.
    SignIn(git::Credentials),
    /// Forget the effective HTTPS helper login, then reopen the tab sign-in dialog.
    SwitchHttps(String),
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
        captions: [Option<super::state::CompareCaption>; 2],
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
    /// Something done on the insights page (opening it at a view, in a view).
    Insights(super::insights::InsightsAct),
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

#[path = "core_seams/branch_actions.rs"]
mod branch_actions;
pub(crate) use branch_actions::*;
#[path = "core_seams/palette_actions.rs"]
mod palette_actions;
pub(crate) use palette_actions::*;
