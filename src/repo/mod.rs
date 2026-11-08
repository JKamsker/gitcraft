//! One open repository: its state, background git work and the view
//! (branches on the left, commit graph or diff in the middle, commit detail / changes on the right).

mod actions;
mod activity;
pub(crate) mod agent;
mod agents;
mod ai;
mod apply;
mod atlas;
mod authorship_view;
mod bakeoff;
mod bisect;
mod branch_bands;
mod branch_cleanup;
mod branch_health;
mod branch_order;
mod branch_tree;
mod branch_visibility;
mod bundles;
mod celebration;
mod ci_status;
mod ci_view;
mod collision_view;
pub(crate) mod columns;
mod commands;
mod commit_actions;
pub(crate) mod conflict;
mod conflict_check;
#[cfg(test)]
mod cpu_benchmarks;
pub(crate) mod custom_commands;
mod dco;
mod detached;
mod detail;
mod dialogs;
mod diff;
mod diff_split;
pub(crate) mod doctor;
mod drafts;
#[cfg(test)]
mod drafts_tests;
mod drag_drop;
mod drift_view;
mod error_help;
mod external_tools;
pub(crate) mod fence_view;
pub(crate) mod file_comparison;
mod file_text;
pub(crate) mod file_view;
mod find;
mod fold;
mod folding;
mod forge_link;
pub(crate) mod forks;
mod gerrit;
mod ghost;
mod graph_motion;
mod graph_order;
pub(crate) mod graph_view;
mod graph_walk;
mod growth;
mod guide;
pub(crate) mod highlight;
mod history_activity;
mod history_reader;
mod hooks;
mod hotfix;
mod interaction;
mod issue_work;
mod keep_submodules;
mod keys;
mod layout;
mod lfs_locks;
mod lfs_storage;
mod lifecycle;
#[cfg(test)]
mod line_history_tests;
mod load;
mod messages;
mod nav_history;
mod navigate;
mod navigation;
mod offline;
pub(crate) mod operations;
mod owners;
mod package;
mod palette;
mod parallel_bisect;
mod partial_stash;
mod patches;
mod pinned_sections;
pub(crate) mod popout;
mod postcards;
mod pr_merge;
mod pr_review;
mod pr_view;
#[cfg(test)]
mod process_benchmarks;
pub(crate) mod pull;
mod push;
mod quick_edit;
mod rebase;
mod reflog;
mod rehearse;
mod release;
mod remerge;
mod revert_piece;
#[cfg(test)]
mod revert_tests;
mod rewrite;
mod running;
#[cfg(test)]
mod samples;
mod sidebar;
mod stack;
mod state;
mod surgery;
mod terminals;
mod test_lane;
#[cfg(test)]
mod tests;
mod timeline;
mod toolbar;
mod unity;
mod versions;
mod watch;
mod watch_remote;
mod weakened_marks;
pub(crate) mod widgets;
pub(crate) mod worktree_tab;
#[cfg(test)]
mod worktree_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use crate::forge::{Forge, Hosting, PullRequest};
use crate::git::{
    self, Commit, CommitDetail, Identity, Refs, Remote, Stash, Submodule, WorkStatus, Worktree,
};
use crate::graph::Graph;
use crate::terminal::Terminal;

use actions::abort_action;
pub(crate) use actions::{Action, fast_forward_action, merge_action, rebase_action};
pub use dialogs::Dialog;
pub(crate) use drafts::DraftShape;
use messages::{Msg, spawn};
pub(crate) use palette::{PaletteGeneration, PaletteItem, PaletteRun, Pick};
pub(crate) use state::{Compare, CompareData, DiffKey, DiffView, Refresh, Reword, Undo, UndoEntry};
use state::{OpKind, PendingOp, Search};
pub(crate) use test_lane::TEST_LANE_TIP;
pub(crate) use toolbar::reveal_label;
pub use widgets::primary_button;

/// Hash of the pseudo-commit that stands for uncommitted changes at the top of the graph.
pub(crate) const WIP_HASH: &str = "\u{0}wip";
/// History deeper than this isn't loaded by scrolling or searching.
pub(crate) const MAX_HISTORY: usize = 100_000;

/// How a tab asks the app to open a folder.
pub enum OpenRequest {
    NewTab(PathBuf),
    /// In the same tab or a new one, as the settings say.
    Worktree(PathBuf),
}

pub struct RepoTab {
    pub path: PathBuf,
    pub name: String,
    tx: messages::JobSender,
    _jobs: crate::tasks::Scope,
    rx: Receiver<Msg>,

    /// When the tab was made: its opening screen shows only if the first read takes a while.
    opened: Instant,
    /// Whether it has been read yet, or was suspended (see `lifecycle`).
    life: lifecycle::Lifecycle,
    /// Numbers the reads of the repository, in the order they start: an answer that arrives
    /// after a later read's doesn't put back what that one replaced.
    reads: u64,
    /// The read the working tree shown comes from.
    work_read: u64,
    /// The read the submodules shown come from.
    submodules_read: u64,
    /// The watcher came while the first read ran, which may have missed changes made before
    /// the watcher was there: check again once it is in.
    recheck_after_load: bool,
    /// Polled as the view shown (not a parked worktree view).
    shown: bool,
    loading: bool,
    /// When the reload running started, and how long the last ones took (newest last), for
    /// the doctor.
    load_started: Option<Instant>,
    load_times: std::collections::VecDeque<std::time::Duration>,
    /// A reload was asked for while one was running, which may have read too early.
    reload_again: bool,
    load_error: Option<String>,
    refs: Refs,
    graph_badge_index: graph_view::BadgeIndex,
    /// Commits as git listed them.
    history: Vec<Arc<Commit>>,
    /// A selected pull request's tip, including one with no local or tracking ref.
    pr_graph_head: Option<Arc<Commit>>,
    has_more: bool,
    loading_more: bool,
    /// The branches hidden from the graph or soloed in it.
    visibility: branch_visibility::Visibility,
    /// Which local branches are merged or stale, and the merged pull requests that tell.
    branch_health: branch_health::HealthState,
    /// What `history` was logged from: `None` for every branch.
    log_from: Option<Vec<String>>,
    /// The order `history` was read in, and whether it follows first parents only.
    log_order: git::LogOrder,
    /// Counts the reloads of `history`, so that a page asked for before one is dropped.
    history_gen: u64,
    /// The `git log` older pages are read from, and the `history_gen` it reads for.
    log_stream: Arc<std::sync::Mutex<Option<(u64, git::LogStream)>>>,
    work: WorkStatus,
    /// Rows of the graph: the WIP pseudo-commit (when the tree is dirty) followed by `history`.
    commits: Vec<Arc<Commit>>,
    graph: Graph,
    /// Ref names for continuous lane runs, rebuilt with the graph, never while scrolling.
    graph_labels: HashMap<usize, String>,
    graph_messages: graph_view::message_tooltip::MessageTooltips,
    index: HashMap<String, usize>,
    worktrees: Vec<Worktree>,
    /// Default root shared by every worktree of this repository (empty: sibling `.wt`).
    worktree_root: String,
    stashes: Vec<Stash>,
    /// The files picked to stash, and whether git here can stash only what is staged.
    stash_picks: partial_stash::StashPicks,
    remotes: Vec<Remote>,
    submodules: Vec<Submodule>,
    /// It keeps files in Git LFS, and git-lfs is installed.
    lfs: bool,
    /// Its LFS file locks, when it keeps files in LFS.
    lfs_locks: lfs_locks::LockState,
    /// What it keeps in LFS on disk, and what is only pointers.
    lfs_storage: lfs_storage::StorageState,
    /// Its Unity project, if it has one.
    unity: unity::UnityState,
    identity: Identity,
    /// Where `origin` is hosted, when it's on a forge gitcraft knows.
    hosting: Option<Hosting>,
    gerrit: Option<crate::gerrit::Server>,
    prs: Vec<PullRequest>,
    pr_error: Option<String>,
    prs_loaded: Option<Instant>,
    /// The build status of the commits that show one.
    ci: ci_status::CiStatuses,
    /// The _weakens checks_ scans of the file lists shown.
    weakened: weakened_marks::WeakenedMarks,
    /// Whether commits are signed off (DCO), and the sign-offs of the pull request shown.
    dco: dco::Dco,
    /// The collision matrix window, while it is open.
    collisions: Option<collision_view::CollisionView>,
    /// The issues branches were made for, and the list of those assigned to you.
    issue_work: issue_work::IssueWork,
    /// Who owns which files (CODEOWNERS).
    owners: owners::Owners,
    /// The agent fence: its list, the worktrees checked against it, its editor.
    fence: fence_view::FenceState,
    /// The scope drift window of a worktree, while it is open.
    drift: Option<drift_view::DriftView>,
    /// Where the pull requests come from (forks), and how their branches are set up here.
    forks: forks::Forks,
    /// What the commits record of who wrote them.
    authorship: authorship_view::Lens,

    selected: Option<usize>,
    /// A stash shown in the detail pane instead of a graph row.
    stash_view: Option<usize>,
    scroll_to: Option<usize>,
    /// The commit the detail pane is about (a graph row or a stash).
    detail_hash: Option<String>,
    detail: Option<CommitDetail>,
    detail_error: Option<String>,
    /// The selected commit's signature, containment, notes and backlinks.
    extras: detail::Extras,
    diff: Option<DiffView>,
    /// One active syntax comparison and at most one latest replacement across diff views.
    structural_running: Option<(u64, u64)>,
    structural_pending: bool,
    file_comparison: Option<file_comparison::State>,
    compare: Option<Compare>,
    /// The pull request the big area shows.
    pr_view: Option<pr_view::PrView>,
    /// The builds of a commit the big area shows.
    ci_view: Option<ci_view::CiView>,
    /// The reflog (or the lost commits) shown above the graph.
    reflog: Option<reflog::ReflogView>,
    /// Two versions of a series of commits compared, the rewrites kept and the pull requests'
    /// versions seen.
    versions: versions::Versions,
    /// The repository doctor, when the big area shows it.
    doctor: Option<Box<doctor::DoctorView>>,
    pr_requests: u64,
    /// What was written on each pull request and not sent, by its number.
    pr_drafts: HashMap<u64, pr_review::PrDraft>,
    compare_requests: u64,
    /// A PR's exact endpoints are still being fetched for this comparison request.
    pr_selection_request: Option<u64>,
    file_view: Option<file_view::FileView>,
    /// The recent views, for back and forward.
    nav: nav_history::NavHistory<navigation::View>,
    /// The frame followed a link to a commit: a step of its own in `nav`.
    nav_jump: bool,
    rebase: Option<rebase::RebaseEditor>,
    /// What a drop offers, while its menu is open.
    drop_menu: Option<drag_drop::DropMenu>,
    /// A commit being split (`surgery`).
    split: Option<Box<surgery::SplitEditor>>,
    /// Staged fixes being distributed into the commits they belong to (`surgery`).
    absorb: Option<Box<surgery::AbsorbEditor>>,
    /// Stacked branches: the recorded parents and the layers they make.
    stacks: stack::Stacks,
    /// The release being prepared or cut (its dialog).
    release: Option<release::ReleaseView>,
    /// The hotfix flow: the dialog that finishes one, and the push after it.
    hotfix: hotfix::Hotfix,
    /// The panel of what a package will publish, while open.
    package: Option<package::PackageView>,
    /// The selected commit's message, being edited in the right pane.
    reword: Option<Reword>,
    /// Push and Sync: the push running, Sync's push waiting for its pull, the banner after a
    /// refused push.
    push_flow: push::PushFlow,
    /// HTTPS credentials typed in this session, used for every later git command of the tab.
    credentials: Option<git::Credentials>,
    /// The last operation started, to run again after signing in.
    last_op: Option<PendingOp>,
    /// A known git failure explained, with its fix (see [`crate::error_card`]).
    error_card: Option<crate::error_card::ErrorCard>,
    /// The forge has been asked for avatars.
    avatars_hinted: bool,
    diff_ignore_ws: bool,
    /// Diffs show the whole file instead of the changes.
    diff_as_file: bool,
    /// Markdown files show rendered in diffs (their changes, or the file).
    diff_rendered: bool,
    /// What is known of which files start folded (generated, lockfiles).
    folding: folding::Folding,
    /// Diffs show as this rich view, for the files it reads.
    diff_rich: Option<diff::rich::RichKind>,
    /// The file a rich view was turned on for that only its contents offer (an API document
    /// not named like one): the view applies to it, and its button shows on it.
    diff_rich_for: Option<String>,
    /// Whether the commit pane lists every file of the commit, and those read.
    all_files: detail::AllFiles,
    /// What merges resolved: the commit pane's view of one, the graph's badges.
    remerges: remerge::Remerges,
    conflict: Option<conflict::ConflictEditor>,
    /// How many times a conflict editor was opened in the tab: the one open is the latest, and
    /// what is read in the background for an earlier one is not taken for it.
    conflict_generation: u64,
    /// Whether merges, rebases and cherry-picks would conflict, by the commits involved.
    conflict_checks: conflict_check::ConflictChecks,
    /// What the toolbar's conflict badge compares the checked-out branch with.
    target_watch: conflict_check::TargetWatch,
    /// The Keep both dialog of a conflicted file that can't be merged.
    keep_both: Option<conflict::KeepBoth>,
    /// The bisect going on, and the output of `git bisect run`.
    bisect: bisect::Bisecting,
    /// The parallel bisect running, or the last one, and the answers by tree.
    parallel: parallel_bisect::Parallel,
    /// Whether conflict resolutions are remembered here (git rerere), and which it reused.
    rerere: conflict::rerere::Rerere,
    /// Conflict decisions taken as a file's save started, stored once it is saved.
    decisions_pending: Option<conflict::decisions::Pending>,
    /// The test lane's results, and the run filling it.
    test_lane: test_lane::TestLane,
    /// The rehearsal shown in its window.
    rehearsing: rehearse::Rehearsing,
    /// The search for the commits of two branches that fail together.
    interactions: interaction::Interactions,

    /// Saved drafts have been restored; an unopened lazy tab must not overwrite them.
    drafts_loaded: bool,
    commit_summary: String,
    commit_body: String,
    /// The commit box's helpers: templates, prefix picker, co-authors, completion.
    compose: detail::compose::Compose,
    amend: bool,
    /// The AI tool's commit message and branch review (see `ai`).
    ai: ai::AiState,
    /// The next commit, or push, runs without its hooks (`--no-verify`).
    skip_hooks: hooks::SkipHooks,
    /// HEAD on no branch: its banner, and checkouts that would leave commits behind.
    detached: detached::Detached,
    dialog: Option<Dialog>,
    /// The first steps, tips and first-time hints.
    guide: guide::Guide,
    search: Search,
    /// Find in the diff, file or blame shown.
    find: find::Find,
    /// A commit to select once enough history is loaded (search results, links).
    pending_goto: Option<String>,

    /// Every operation and change seen, kept across restarts; undo and redo walk it.
    timeline: timeline::Timeline,
    /// The timeline (undo tree) shown in the big area.
    timeline_view: Option<timeline::view::TimelineView>,
    /// Why the timeline isn't kept on disk, or what it couldn't do.
    timeline_error: Option<String>,
    fold: fold::FoldState,
    agent_calls: agent::Agents,

    busy_op: Option<String>,
    op_started: Option<Instant>,
    /// The operation running: its progress, and how to cancel it.
    running: Option<running::Running>,
    /// Operations asked for while another ran, to run after it.
    waiting: running::Waiting,
    /// The operations run, with their commands and output (the Activity panel).
    activity: activity::ActivityLog,
    graph_motion: graph_motion::State,
    celebration: celebration::State,
    topology: std::sync::Arc<graph_walk::Topology>,
    topology_reader: history_reader::Reader<std::sync::Arc<graph_walk::Topology>>,
    graph_narration: graph_walk::Narration,
    postcards: postcards::State,
    growth: Option<growth::State>,
    atlas: Option<atlas::State>,
    history_activity: Option<history_activity::Activity>,
    history_minimap: graph_view::minimap::State,
    branch_bands: branch_bands::Preferences,
    band_reader: history_reader::Reader<crate::graph::Graph>,
    /// A network operation failed (taken by `App`, for the taskbar button).
    network_failed: bool,
    /// The running operation moves the checkout: update the submodules after it, if asked to.
    update_submodules: bool,
    status: Option<(String, bool)>,
    branch_filter: String,
    /// Where a clicked jump mark wants the sidebar scrolled to.
    sidebar_jump: Option<f32>,
    /// A sidebar section to open and scroll to, asked for from the folded sidebar.
    sidebar_reveal: Option<&'static str>,
    /// Put the cursor in the branch filter on the next frame.
    focus_branch_filter: bool,
    /// Jump to the changes row on the next rebuild (after an operation left conflicts).
    focus_wip: bool,
    /// Put the cursor in the commit summary on the next frame (the commit key, with no
    /// summary written yet).
    focus_commit_summary: bool,
    last_fetch: Instant,
    /// When the repository was last fetched, for the offline chip (see `offline`).
    fetched_at: std::sync::Arc<std::sync::Mutex<Option<std::time::SystemTime>>>,
    /// That was looked up once.
    fetch_time_asked: bool,
    /// Background network work was paused on the last frame (see `follow_network`).
    network_paused: bool,

    terminals: Vec<Terminal>,
    active_terminal: usize,
    terminal_open: bool,
    terminal_error: Option<String>,
    /// The views shown in windows of their own.
    popouts: popout::Popouts,

    watcher: Option<notify::RecommendedWatcher>,
    /// How many times the tab set up its watching: a watcher arriving for an earlier time is
    /// not this one's (see `Msg::Watcher`).
    watches: u64,
    /// Suspended and shown again, not read again yet: the opening screen shows meanwhile, and
    /// nothing is laid out from the empty history (see `lifecycle`).
    resuming: bool,
    /// The commit selected when the tab was suspended, selected again once read.
    resume_select: Option<String>,
    /// The diff open when the tab was suspended, opened again once read.
    resume_diff: Option<DiffKey>,
    /// A remote watcher schedules one bounded look at a time from its UI timer.
    poller: Option<watch_remote::Poller>,
    /// The worktrees' changes are being counted (see `count_worktrees`).
    counting_worktrees: bool,
    /// A read came in while they were counted: they are counted again after.
    recount_worktrees: bool,
    /// When they were last counted (see `recount_siblings`).
    counted_at: Instant,
    fs_rx: Option<Receiver<watch::FsChange>>,
    /// A change on disk not yet reloaded: (inside .git, when).
    fs_pending: Option<(watch::FsChange, Instant)>,

    /// The ghost run of the operation under the pointer.
    ghost: ghost::Ghost,
    /// Actions queued from outside (the command palette).
    queued: Vec<Action>,
    /// Where a patch dropped on the window is (see `patches`).
    dropped_patch: patches::DroppedPatch,
    /// A folder the app should open (taken by `App`).
    pub open_request: Option<OpenRequest>,
    /// A worktree to open once the operation so labelled (which makes it) is done.
    open_when_done: Option<(String, PathBuf)>,
    /// The other worktrees this tab was on, oldest first, for coming back to. Boxed as in
    /// `Tab::Repo`, which they move in and out of.
    #[allow(clippy::vec_box)]
    parked: Vec<Box<RepoTab>>,
    /// Those of them with work going on, and what work.
    parked_status: Vec<(PathBuf, &'static str)>,
    /// The app has a dialog of its own open (palette, settings): leave the keys to it.
    pub keys_blocked: bool,
    /// Agent tasks, the worktree setup recipe and their dialogs.
    agents: agents::AgentsState,
    /// The bake-off of attempts shown in the big area, and its picker.
    bakeoff: bakeoff::BakeoffState,
    /// Long operations that just ended, its own and its parked worktrees': (message, failed),
    /// for a notification each (taken by `App`).
    pub finished: Vec<(String, bool)>,
    /// The repository's own custom commands, and what the palette offers of them.
    custom: custom_commands::TabCommands,
}

impl RepoTab {
    pub fn new(path: PathBuf, ctx: &egui::Context) -> Self {
        let mut tab = Self::new_deferred(path, ctx);
        tab.start_deferred(ctx);
        tab
    }

    /// A tab that reads and watches nothing until [`Self::start_deferred`] (it is shown): the
    /// tabs restored on start are read only once looked at.
    pub fn new_deferred(path: PathBuf, _ctx: &egui::Context) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let (sender, rx) = channel();
        let jobs = crate::tasks::Scope::default();
        let tx = messages::JobSender {
            sender,
            cancel: jobs.0.clone(),
            repo: messages::mutation_key(&path),
        };
        let branch_bands = branch_bands::Preferences::load(_ctx, &path);
        Self {
            path,
            name,
            tx,
            _jobs: jobs,
            rx,
            opened: Instant::now(),
            life: lifecycle::Lifecycle::new(Instant::now()),
            reads: 0,
            work_read: 0,
            submodules_read: 0,
            recheck_after_load: false,
            shown: true,
            loading: false,
            load_started: None,
            load_times: Default::default(),
            reload_again: false,
            load_error: None,
            refs: Refs::default(),
            graph_badge_index: graph_view::BadgeIndex::default(),
            history: Vec::new(),
            pr_graph_head: None,
            has_more: false,
            loading_more: false,
            visibility: Default::default(),
            branch_health: Default::default(),
            log_from: None,
            log_order: Default::default(),
            history_gen: 0,
            log_stream: Default::default(),
            work: WorkStatus::default(),
            commits: Vec::new(),
            graph: Graph {
                rows: Vec::new().into(),
                width: 0,
            },
            graph_labels: HashMap::new(),
            graph_messages: Default::default(),
            index: HashMap::new(),
            worktrees: Vec::new(),
            worktree_root: String::new(),
            stashes: Vec::new(),
            stash_picks: Default::default(),
            remotes: Vec::new(),
            submodules: Vec::new(),
            lfs: false,
            lfs_locks: Default::default(),
            lfs_storage: Default::default(),
            unity: Default::default(),
            identity: Identity::default(),
            hosting: None,
            gerrit: None,
            prs: Vec::new(),
            pr_error: None,
            prs_loaded: None,
            ci: ci_status::CiStatuses::default(),
            weakened: Default::default(),
            dco: dco::Dco::default(),
            collisions: None,
            issue_work: Default::default(),
            owners: owners::Owners::default(),
            fence: Default::default(),
            drift: None,
            forks: forks::Forks::default(),
            authorship: Default::default(),
            selected: None,
            stash_view: None,
            scroll_to: None,
            detail_hash: None,
            detail: None,
            detail_error: None,
            extras: Default::default(),
            diff: None,
            structural_running: None,
            structural_pending: false,
            file_comparison: None,
            compare: None,
            pr_view: None,
            ci_view: None,
            reflog: None,
            versions: Default::default(),
            doctor: None,
            pr_requests: 0,
            pr_drafts: HashMap::new(),
            compare_requests: 0,
            pr_selection_request: None,
            file_view: None,
            nav: Default::default(),
            nav_jump: false,
            rebase: None,
            drop_menu: None,
            split: None,
            absorb: None,
            stacks: Default::default(),
            release: None,
            hotfix: hotfix::Hotfix::default(),
            package: None,
            reword: None,
            push_flow: Default::default(),
            credentials: None,
            last_op: None,
            error_card: None,
            avatars_hinted: false,
            diff_ignore_ws: false,
            diff_as_file: false,
            diff_rendered: false,
            folding: Default::default(),
            diff_rich: None,
            diff_rich_for: None,
            all_files: Default::default(),
            remerges: Default::default(),
            conflict: None,
            conflict_generation: 0,
            conflict_checks: Default::default(),
            target_watch: Default::default(),
            keep_both: None,
            bisect: Default::default(),
            parallel: Default::default(),
            rerere: Default::default(),
            decisions_pending: None,
            test_lane: Default::default(),
            rehearsing: Default::default(),
            interactions: Default::default(),
            drafts_loaded: false,
            commit_summary: String::new(),
            commit_body: String::new(),
            compose: Default::default(),
            amend: false,
            ai: Default::default(),
            skip_hooks: Default::default(),
            detached: Default::default(),
            dialog: None,
            guide: Default::default(),
            search: Search::default(),
            find: find::Find::default(),
            pending_goto: None,
            timeline: Default::default(),
            timeline_view: None,
            timeline_error: None,
            fold: Default::default(),
            agent_calls: Default::default(),
            busy_op: None,
            op_started: None,
            running: None,
            waiting: Default::default(),
            activity: Default::default(),
            graph_motion: Default::default(),
            celebration: Default::default(),
            topology: Default::default(),
            topology_reader: Default::default(),
            graph_narration: Default::default(),
            postcards: Default::default(),
            growth: None,
            atlas: None,
            history_activity: None,
            history_minimap: Default::default(),
            branch_bands,
            band_reader: Default::default(),
            network_failed: false,
            update_submodules: false,
            status: None,
            branch_filter: String::new(),
            sidebar_jump: None,
            sidebar_reveal: None,
            focus_branch_filter: false,
            focus_wip: false,
            focus_commit_summary: false,
            last_fetch: Instant::now(),
            fetched_at: Default::default(),
            fetch_time_asked: false,
            network_paused: false,
            terminals: Vec::new(),
            active_terminal: 0,
            terminal_open: false,
            terminal_error: None,
            popouts: Default::default(),
            watcher: None,
            watches: 0,
            resuming: false,
            resume_select: None,
            resume_diff: None,
            poller: None,
            counting_worktrees: false,
            recount_worktrees: false,
            counted_at: Instant::now(),
            fs_rx: None,
            fs_pending: None,
            ghost: ghost::Ghost::default(),
            queued: Vec::new(),
            dropped_patch: Default::default(),
            open_request: None,
            open_when_done: None,
            parked: Vec::new(),
            parked_status: Vec::new(),
            keys_blocked: false,
            agents: Default::default(),
            bakeoff: Default::default(),
            finished: Vec::new(),
            custom: Default::default(),
        }
    }

    /// Starts the first read of a tab made with [`Self::new_deferred`] (and its watcher), or
    /// the read after it was suspended: it is shown.
    pub fn start_deferred(&mut self, ctx: &egui::Context) {
        self.shown_now(ctx);
    }

    /// The commit selected, for tests of what opens it.
    #[cfg(test)]
    pub(crate) fn selected_commit(&self) -> Option<String> {
        self.selected_hash()
    }

    /// The file whose history, blame or contents the big area shows, if one is.
    #[cfg(test)]
    pub(crate) fn file_view_path(&self) -> Option<String> {
        self.file_view.as_ref().map(|v| v.path.clone())
    }

    /// The remotes the tab read, once it has read the repository.
    pub(crate) fn remotes_read(&self) -> Option<Vec<git::Remote>> {
        (!self.opening()).then(|| self.remotes.clone())
    }

    /// An editor holds work that showing something else would close: a conflict being
    /// solved, an interactive rebase being set up.
    pub(crate) fn editor_open(&self) -> bool {
        self.conflict.is_some() || self.rebase.is_some()
    }

    /// Nothing read yet: the tab shows its opening screen.
    pub fn opening(&self) -> bool {
        (self.history_gen == 0 && self.load_error.is_none()) || self.resuming
    }

    pub fn is_busy(&self) -> bool {
        self.loading || self.busy_op.is_some()
    }

    /// Queues an action from outside the tab (the command palette).
    pub(crate) fn queue(&mut self, action: Action) {
        self.queued.push(action);
    }

    /// The forge `origin` is on.
    pub(super) fn forge(&self) -> Option<Forge> {
        self.hosting.as_ref().map(|h| h.forge)
    }

    /// Whether the tab knows its forge and can show that forge's pull requests in full.
    pub(crate) fn shows_pull_requests(&self) -> bool {
        self.hosting.as_ref().is_some_and(|h| h.forge.has_pages())
    }

    /// HTTPS credentials for this tab's git commands (from the clone that made it).
    pub fn set_credentials(&mut self, creds: git::Credentials) {
        self.credentials = Some(creds);
    }
}

/// A hash as the UI shows it: its first 8 characters.
pub(crate) fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// `&[&["checkout", "main"]]` → owned argument lists.
pub(crate) fn ops(list: &[&[&str]]) -> Vec<Vec<String>> {
    list.iter()
        .map(|args| args.iter().map(|a| (*a).to_owned()).collect())
        .collect()
}
