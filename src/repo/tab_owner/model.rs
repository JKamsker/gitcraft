//! Repository tab composition. Fields retain their original repository-only visibility.
use super::*;

pub struct RepoTab {
    pub path: PathBuf,
    pub name: String,
    pub(in crate::repo) tx: messages::JobSender,
    pub(in crate::repo) _jobs: crate::tasks::Scope,
    pub(in crate::repo) behaviour_cache: crate::parallel_bisect::observe::Cache,
    pub(in crate::repo) rx: Receiver<Msg>,

    /// When the tab was made: its opening screen shows only if the first read takes a while.
    pub(in crate::repo) opened: Instant,
    /// Whether it has been read yet, or was suspended (see `lifecycle`).
    pub(in crate::repo) life: lifecycle::Lifecycle,
    /// Numbers the reads of the repository, in the order they start: an answer that arrives
    /// after a later read's doesn't put back what that one replaced.
    pub(in crate::repo) reads: u64,
    /// The read the working tree shown comes from.
    pub(in crate::repo) work_read: u64,
    pub(in crate::repo) work_pending: u64,
    pub(in crate::repo) work_epoch: u64,
    pub(in crate::repo) keyboard: std::cell::RefCell<keyboard::State>,
    /// The read the submodules shown come from.
    pub(in crate::repo) submodules_read: u64,
    /// The watcher came while the first read ran, which may have missed changes made before
    /// the watcher was there: check again once it is in.
    pub(in crate::repo) recheck_after_load: bool,
    /// Polled as the view shown (not a parked worktree view).
    pub(in crate::repo) shown: bool,
    pub(in crate::repo) loading: bool,
    /// When the reload running started, and how long the last ones took (newest last), for
    /// the doctor.
    pub(in crate::repo) load_started: Option<Instant>,
    /// Most recent reload start, retained after completion for focus deduplication.
    pub(in crate::repo) last_load_started: Option<Instant>,
    pub(in crate::repo) load_times: std::collections::VecDeque<std::time::Duration>,
    /// A reload was asked for while one was running, which may have read too early.
    pub(in crate::repo) reload_again: bool,
    pub(in crate::repo) load_error: Option<String>,
    pub(in crate::repo) refs: Refs,
    pub(in crate::repo) graph_badge_index: graph_view::BadgeIndex,
    /// Commits as git listed them.
    pub(in crate::repo) content_recovery: content_recovery::State,
    pub(in crate::repo) working_drafts: working_drafts::State,
    pub(in crate::repo) dossier: line_dossier::State,
    pub(in crate::repo) symbols: symbol_biography::State,
    pub(in crate::repo) cochanges: cochanges::State,
    pub(in crate::repo) saved_views: saved_views::State,
    pub(in crate::repo) history: Vec<Arc<Commit>>,
    /// Operational lookup into raw history; display filtering must never break ancestry.
    pub(in crate::repo) history_index: HashMap<String, usize>,
    /// A selected pull request's tip, including one with no local or tracking ref.
    pub(in crate::repo) pr_graph_head: Option<Arc<Commit>>,
    pub(in crate::repo) has_more: bool,
    pub(in crate::repo) loading_more: bool,
    /// The branches hidden from the graph or soloed in it.
    pub(in crate::repo) visibility: branch_visibility::Visibility,
    /// Which local branches are merged or stale, and the merged pull requests that tell.
    pub(in crate::repo) branch_health: branch_health::HealthState,
    /// What `history` was logged from: `None` for every branch.
    pub(in crate::repo) log_from: Option<Vec<String>>,
    /// The order `history` was read in, and whether it follows first parents only.
    pub(in crate::repo) log_order: git::LogOrder,
    /// Counts the reloads of `history`, so that a page asked for before one is dropped.
    pub(in crate::repo) history_gen: u64,
    /// The `git log` older pages are read from, and the `history_gen` it reads for.
    pub(in crate::repo) log_stream: Arc<std::sync::Mutex<Option<(u64, git::LogStream)>>>,
    pub(in crate::repo) work: WorkStatus,
    /// Rows of the graph: the WIP pseudo-commit (when the tree is dirty) followed by `history`.
    pub(in crate::repo) commits: Vec<Arc<Commit>>,
    pub(in crate::repo) graph: Graph,
    /// Ref names for continuous lane runs, rebuilt with the graph, never while scrolling.
    pub(in crate::repo) graph_labels: HashMap<usize, String>,
    pub(in crate::repo) graph_messages: graph_view::message_tooltip::MessageTooltips,
    pub(in crate::repo) index: HashMap<String, usize>,
    pub(in crate::repo) worktrees: Vec<Worktree>,
    /// Default root shared by every worktree of this repository (empty: sibling `.wt`).
    pub(in crate::repo) worktree_root: String,
    pub(in crate::repo) stashes: Vec<Stash>,
    /// The files picked to stash, and whether git here can stash only what is staged.
    pub(in crate::repo) stash_picks: partial_stash::StashPicks,
    pub(in crate::repo) remotes: Vec<Remote>,
    pub(in crate::repo) submodules: Vec<Submodule>,
    /// It keeps files in Git LFS, and git-lfs is installed.
    pub(in crate::repo) lfs: bool,
    pub(in crate::repo) practice: Option<crate::practice::Session>,
    pub(in crate::repo) practice_checking: bool,
    pub(in crate::repo) practice_result: Option<Result<bool, String>>,
    /// Its LFS file locks, when it keeps files in LFS.
    pub(in crate::repo) lfs_locks: lfs_locks::LockState,
    /// What it keeps in LFS on disk, and what is only pointers.
    pub(in crate::repo) lfs_storage: lfs_storage::StorageState,
    /// Its Unity project, if it has one.
    pub(in crate::repo) unity: unity::UnityState,
    pub(in crate::repo) identity: Identity,
    /// Where the selected primary forge remote is hosted.
    pub(in crate::repo) hosting: Option<Hosting>,
    pub(in crate::repo) forge_selection: forge_selection::Selection,
    pub(in crate::repo) forge_remotes: Vec<(String, Hosting)>,
    pub(in crate::repo) forge_generation: u64,
    /// The selection lifetime when the actual hosting repository last changed.
    pub(in crate::repo) forge_repository_generation: u64,
    pub(in crate::repo) gerrit: Option<crate::gerrit::Server>,
    pub(in crate::repo) prs: Vec<PullRequest>,
    /// Whether the displayed list came from persistence rather than a fresh forge response.
    pub(in crate::repo) prs_from_cache: bool,
    /// A newer list, read while the pointer was on the shown one: it waits until the user is
    /// done there, or brings it in from the header (see `sidebar::pull_requests`).
    pub(in crate::repo) pending_prs: Option<Vec<PullRequest>>,
    pub(in crate::repo) pr_error: Option<String>,
    pub(in crate::repo) prs_loaded: Option<Instant>,
    /// The build status of the commits that show one.
    pub(in crate::repo) ci: ci_status::CiStatuses,
    /// The _weakens checks_ scans of the file lists shown.
    pub(in crate::repo) weakened: weakened_marks::WeakenedMarks,
    /// Whether commits are signed off (DCO), and the sign-offs of the pull request shown.
    pub(in crate::repo) dco: dco::Dco,
    /// The collision matrix window, while it is open.
    pub(in crate::repo) collisions: Option<collision_view::CollisionView>,
    /// The issues branches were made for, and the list of those assigned to you.
    pub(in crate::repo) issue_work: issue_work::IssueWork,
    /// Who owns which files (CODEOWNERS).
    pub(in crate::repo) owners: owners::Owners,
    /// The agent fence: its list, the worktrees checked against it, its editor.
    pub(in crate::repo) fence: fence_view::FenceState,
    /// The scope drift window of a worktree, while it is open.
    pub(in crate::repo) drift: Option<drift_view::DriftView>,
    /// Where the pull requests come from (forks), and how their branches are set up here.
    pub(in crate::repo) forks: forks::Forks,
    /// What the commits record of who wrote them.
    pub(in crate::repo) authorship: authorship_view::Lens,
    pub(in crate::repo) history_filter: history_filter::State,

    pub(in crate::repo) selected: Option<usize>,
    /// A stash shown in the detail pane instead of a graph row.
    pub(in crate::repo) stash_view: Option<usize>,
    pub(in crate::repo) scroll_to: Option<usize>,
    /// The commit the detail pane is about (a graph row or a stash).
    pub(in crate::repo) detail_hash: Option<String>,
    pub(in crate::repo) commit_changes: commit_changes::State,
    pub(in crate::repo) detail: Option<CommitDetail>,
    pub(in crate::repo) detail_error: Option<String>,
    /// Details read before (or ahead), so a commit clicked again shows at once.
    pub(in crate::repo) detail_cache: detail_cache::DetailCache,
    /// The files of the commits on screen, read ahead so a click shows them at once.
    pub(in crate::repo) files_cache: detail_cache::FilesCache,
    /// Ends the reads for the selected commit (and its neighbours) when the selection moves on.
    pub(in crate::repo) detail_cancel: crate::tasks::Cancellation,
    /// When the selected commit's detail was asked of git (see `slow_banner`).
    pub(in crate::repo) detail_asked: Option<Instant>,
    /// The graph shows the last snapshot of the repository (see `snapshot`) until the first read.
    pub(in crate::repo) snapshot_shown: bool,
    /// When the last snapshot was kept.
    pub(in crate::repo) snapshot_kept: Option<Instant>,
    /// The selected commit's signature, containment, notes and backlinks.
    pub(in crate::repo) extras: detail::Extras,
    pub(in crate::repo) diff: Option<DiffView>,
    /// One active syntax comparison and at most one latest replacement across diff views.
    pub(in crate::repo) structural_running: Option<(u64, u64)>,
    pub(in crate::repo) structural_pending: bool,
    pub(in crate::repo) file_comparison: Option<file_comparison::State>,
    pub(in crate::repo) compare: Option<Compare>,
    /// The pull request the big area shows.
    pub(in crate::repo) pr_view: Option<pr_view::PrView>,
    pub(in crate::repo) review_document: review_workflow::ReviewState,
    pub(in crate::repo) review_tools: review_workflow::Tools,
    /// The builds of a commit the big area shows.
    pub(in crate::repo) ci_view: Option<ci_view::CiView>,
    pub(in crate::repo) local_ci: local_ci::State,
    pub(in crate::repo) email: email::State,
    pub(in crate::repo) devcontainer: devcontainer::State,
    /// The reflog (or the lost commits) shown above the graph.
    pub(in crate::repo) reflog: Option<reflog::ReflogView>,
    /// Two versions of a series of commits compared, the rewrites kept and the pull requests'
    /// versions seen.
    pub(in crate::repo) versions: versions::Versions,
    /// The repository doctor, when the big area shows it.
    pub(in crate::repo) doctor: Option<Box<doctor::DoctorView>>,
    pub(in crate::repo) tour: Option<tour::TourView>,
    pub(in crate::repo) ssh_setup: Option<ssh_setup::State>,
    /// The insights page, when the big area shows it.
    pub(in crate::repo) insights: Option<Box<insights::InsightsView>>,
    pub(in crate::repo) pr_requests: u64,
    /// What was written on each pull request and not sent, by its number.
    pub(in crate::repo) pr_drafts: HashMap<u64, pr_review::PrDraft>,
    pub(in crate::repo) compare_requests: u64,
    /// A PR's exact endpoints are still being fetched for this comparison request.
    pub(in crate::repo) pr_selection_request: Option<u64>,
    pub(in crate::repo) file_view: Option<file_view::FileView>,
    /// The recent views, for back and forward.
    pub(in crate::repo) nav: nav_history::NavHistory<navigation::View>,
    /// The frame followed a link to a commit: a step of its own in `nav`.
    pub(in crate::repo) nav_jump: bool,
    pub(in crate::repo) rebase: Option<rebase::RebaseEditor>,
    /// What a drop offers, while its menu is open.
    pub(in crate::repo) drop_menu: Option<drag_drop::DropMenu>,
    /// A commit being split (`surgery`).
    pub(in crate::repo) split: Option<Box<surgery::SplitEditor>>,
    /// Staged fixes being distributed into the commits they belong to (`surgery`).
    pub(in crate::repo) absorb: Option<Box<surgery::AbsorbEditor>>,
    /// Stacked branches: the recorded parents and the layers they make.
    pub(in crate::repo) stacks: stack::Stacks,
    pub(in crate::repo) deferred: repository_tools::DeferredCache,
    pub(in crate::repo) prompt_session: prompt_session::State,
    pub(in crate::repo) outbox_last: Option<std::time::Instant>,
    pub(in crate::repo) outbox_reading: bool,
    /// The release being prepared or cut (its dialog).
    pub(in crate::repo) release: Option<release::ReleaseView>,
    /// The hotfix flow: the dialog that finishes one, and the push after it.
    pub(in crate::repo) hotfix: hotfix::Hotfix,
    /// The panel of what a package will publish, while open.
    pub(in crate::repo) package: Option<package::PackageView>,
    /// The selected commit's message, being edited in the right pane.
    pub(in crate::repo) reword: Option<Reword>,
    /// Push and Sync: the push running, Sync's push waiting for its pull, the banner after a
    /// refused push.
    pub(in crate::repo) push_flow: push::PushFlow,
    pub(in crate::repo) plain_rebase_request: u64,
    pub(in crate::repo) preflight_pending: Option<PendingOp>,
    pub(in crate::repo) preflight_selection: Option<git::repository_tools::routes::PreparedPush>,
    pub(in crate::repo) preflight_after: Option<(String, PendingOp)>,
    pub(in crate::repo) preflight_timeline: Option<Option<u64>>,
    /// HTTPS credentials typed in this session, used for every later git command of the tab.
    pub(in crate::repo) credentials: Option<git::Credentials>,
    pub(in crate::repo) connection_generation: u64,
    pub(in crate::repo) connection_context: crate::connections::Context,
    /// The last operation started, to run again after signing in.
    pub(in crate::repo) last_op: Option<PendingOp>,
    pub(in crate::repo) https_generation: u64,
    pub(in crate::repo) https_checking: bool,
    pub(in crate::repo) https_challenge: Option<https_recovery::Ticket>,
    /// A known git failure explained, with its fix (see [`crate::error_card`]).
    pub(in crate::repo) error_card: Option<crate::error_card::ErrorCard>,
    /// The forge has been asked for avatars.
    pub(in crate::repo) avatars_hinted: bool,
    pub(in crate::repo) diff_ignore_ws: bool,
    /// Diffs show the whole file instead of the changes.
    pub(in crate::repo) diff_as_file: bool,
    /// Markdown files show rendered in diffs (their changes, or the file).
    pub(in crate::repo) diff_rendered: bool,
    /// What is known of which files start folded (generated, lockfiles).
    pub(in crate::repo) folding: folding::Folding,
    /// Diffs show as this rich view, for the files it reads.
    pub(in crate::repo) diff_rich: Option<diff::rich::RichKind>,
    /// The file a rich view was turned on for that only its contents offer (an API document
    /// not named like one): the view applies to it, and its button shows on it.
    pub(in crate::repo) diff_rich_for: Option<String>,
    /// Whether the commit pane lists every file of the commit, and those read.
    pub(in crate::repo) all_files: detail::AllFiles,
    /// What merges resolved: the commit pane's view of one, the graph's badges.
    pub(in crate::repo) remerges: remerge::Remerges,
    pub(in crate::repo) conflict: Option<conflict::ConflictEditor>,
    /// How many times a conflict editor was opened in the tab: the one open is the latest, and
    /// what is read in the background for an earlier one is not taken for it.
    pub(in crate::repo) conflict_generation: u64,
    /// Whether merges, rebases and cherry-picks would conflict, by the commits involved.
    pub(in crate::repo) conflict_checks: conflict_check::ConflictChecks,
    /// What the toolbar's conflict badge compares the checked-out branch with.
    pub(in crate::repo) target_watch: conflict_check::TargetWatch,
    /// The Keep both dialog of a conflicted file that can't be merged.
    pub(in crate::repo) keep_both: Option<conflict::KeepBoth>,
    /// The bisect going on, and the output of `git bisect run`.
    pub(in crate::repo) bisect: bisect::Bisecting,
    /// The parallel bisect running, or the last one, and the answers by tree.
    pub(in crate::repo) parallel: parallel_bisect::Parallel,
    /// Whether conflict resolutions are remembered here (git rerere), and which it reused.
    pub(in crate::repo) rerere: conflict::rerere::Rerere,
    /// Conflict decisions taken as a file's save started, stored once it is saved.
    pub(in crate::repo) decisions_pending: Option<conflict::decisions::Pending>,
    /// The test lane's results, and the run filling it.
    pub(in crate::repo) test_lane: test_lane::TestLane,
    /// The rehearsal shown in its window.
    pub(in crate::repo) rehearsing: rehearse::Rehearsing,
    /// The search for the commits of two branches that fail together.
    pub(in crate::repo) interactions: interaction::Interactions,

    /// Saved drafts have been restored; an unopened lazy tab must not overwrite them.
    pub(in crate::repo) drafts_loaded: bool,
    pub(in crate::repo) commit_summary: String,
    pub(in crate::repo) commit_body: String,
    /// The commit box's helpers: templates, prefix picker, co-authors, completion.
    pub(in crate::repo) compose: detail::compose::Compose,
    pub(in crate::repo) amend: bool,
    /// The AI tool's commit message and branch review (see `ai`).
    pub(in crate::repo) ai: ai::AiState,
    /// The next commit, or push, runs without its hooks (`--no-verify`).
    pub(in crate::repo) skip_hooks: hooks::SkipHooks,
    /// HEAD on no branch: its banner, and checkouts that would leave commits behind.
    pub(in crate::repo) detached: detached::Detached,
    pub(in crate::repo) dialog: Option<Dialog>,
    /// The first steps, tips and first-time hints.
    pub(in crate::repo) guide: guide::Guide,
    pub(in crate::repo) search: Search,
    /// Find in the diff, file or blame shown.
    pub(in crate::repo) find: find::Find,
    /// A commit to select once enough history is loaded (search results, links).
    pub(in crate::repo) pending_goto: Option<String>,

    /// Every operation and change seen, kept across restarts; undo and redo walk it.
    pub(in crate::repo) timeline: timeline::Timeline,
    /// The timeline (undo tree) shown in the big area.
    pub(in crate::repo) timeline_view: Option<timeline::view::TimelineView>,
    /// Why the timeline isn't kept on disk, or what it couldn't do.
    pub(in crate::repo) timeline_error: Option<String>,
    pub(in crate::repo) fold: fold::FoldState,
    pub(in crate::repo) agent_calls: agent::Agents,
    /// An agent resolving the conflicts in the background, if one.
    pub(in crate::repo) resolver: resolver::ResolverState,

    pub(in crate::repo) busy_op: Option<String>,
    pub(in crate::repo) op_started: Option<Instant>,
    /// The operation running: its progress, and how to cancel it.
    pub(in crate::repo) running: Option<running::Running>,
    /// Operations asked for while another ran, to run after it.
    pub(in crate::repo) waiting: running::Waiting,
    /// The operations run, with their commands and output (the Activity panel).
    pub(in crate::repo) activity: activity::ActivityLog,
    pub(in crate::repo) graph_motion: graph_motion::State,
    pub(in crate::repo) celebration: celebration::State,
    pub(in crate::repo) topology: std::sync::Arc<graph_walk::Topology>,
    pub(in crate::repo) topology_reader:
        history_reader::Reader<std::sync::Arc<graph_walk::Topology>>,
    pub(in crate::repo) graph_narration: graph_walk::Narration,
    pub(in crate::repo) postcards: postcards::State,
    pub(in crate::repo) growth: Option<growth::State>,
    pub(in crate::repo) cleanup: Option<cleanup::State>,
    pub(in crate::repo) sparse: Option<sparse::State>,
    pub(in crate::repo) atlas: Option<atlas::State>,
    pub(in crate::repo) history_activity: Option<history_activity::Activity>,
    pub(in crate::repo) history_minimap: graph_view::minimap::State,
    pub(in crate::repo) branch_bands: branch_bands::Preferences,
    pub(in crate::repo) band_reader: history_reader::Reader<crate::graph::Graph>,
    /// A network operation failed (taken by `App`, for the taskbar button).
    pub(in crate::repo) network_failed: bool,
    /// The running operation moves the checkout: update the submodules after it, if asked to.
    pub(in crate::repo) update_submodules: bool,
    pub(in crate::repo) status: Option<Notice>,
    pub(in crate::repo) branch_filter: String,
    /// Where a clicked jump mark wants the sidebar scrolled to.
    pub(in crate::repo) sidebar_jump: Option<f32>,
    /// A sidebar section to open and scroll to, asked for from the folded sidebar.
    pub(in crate::repo) sidebar_reveal: Option<&'static str>,
    /// Put the cursor in the branch filter on the next frame.
    pub(in crate::repo) focus_branch_filter: bool,
    /// Jump to the changes row on the next rebuild (after an operation left conflicts).
    pub(in crate::repo) focus_wip: bool,
    /// Put the cursor in the commit summary on the next frame (the commit key, with no
    /// summary written yet).
    pub(in crate::repo) focus_commit_summary: bool,
    pub(in crate::repo) last_fetch: Instant,
    /// When the repository was last fetched, for the offline chip (see `offline`).
    pub(in crate::repo) fetched_at: std::sync::Arc<std::sync::Mutex<Option<std::time::SystemTime>>>,
    /// That was looked up once.
    pub(in crate::repo) fetch_time_asked: bool,
    /// Background network work was paused on the last frame (see `follow_network`).
    pub(in crate::repo) network_paused: bool,

    pub(in crate::repo) terminals: terminals::Terminals,
    /// The views shown in windows of their own.
    pub(in crate::repo) popouts: popout::Popouts,

    pub(in crate::repo) watcher: Option<notify::RecommendedWatcher>,
    /// How many times the tab set up its watching: a watcher arriving for an earlier time is
    /// not this one's (see `Msg::Watcher`).
    pub(in crate::repo) watches: u64,
    /// Suspended and shown again, not read again yet: the opening screen shows meanwhile, and
    /// nothing is laid out from the empty history (see `lifecycle`).
    pub(in crate::repo) resuming: bool,
    /// The commit selected when the tab was suspended, selected again once read.
    pub(in crate::repo) resume_select: Option<String>,
    /// The diff open when the tab was suspended, opened again once read.
    pub(in crate::repo) resume_diff: Option<DiffKey>,
    /// A remote watcher schedules one bounded look at a time from its UI timer.
    pub(in crate::repo) poller: Option<watch_remote::Poller>,
    /// The worktrees' changes are being counted (see `count_worktrees`).
    pub(in crate::repo) counting_worktrees: bool,
    /// A read came in while they were counted: they are counted again after.
    pub(in crate::repo) recount_worktrees: bool,
    /// When they were last counted (see `recount_siblings`).
    pub(in crate::repo) counted_at: Instant,
    pub(in crate::repo) fs_rx: Option<Receiver<watch::FsChange>>,
    /// A change on disk not yet reloaded: (inside .git, when).
    pub(in crate::repo) fs_pending: Option<(watch::FsChange, Instant)>,

    /// The ghost run of the operation under the pointer.
    pub(in crate::repo) ghost: ghost::Ghost,
    /// Actions queued from outside (the command palette).
    pub(in crate::repo) queued: Vec<Action>,
    /// Where a patch dropped on the window is (see `patches`).
    pub(in crate::repo) dropped_patch: patches::DroppedPatch,
    /// A folder the app should open (taken by `App`).
    pub open_request: Option<OpenRequest>,
    /// A worktree to open once the operation so labelled (which makes it) is done.
    pub(in crate::repo) open_when_done: Option<(String, PathBuf)>,
    /// The other worktrees this tab was on, oldest first, for coming back to. Boxed as in
    /// `Tab::Repo`, which they move in and out of.
    #[allow(clippy::vec_box)]
    pub(in crate::repo) parked: Vec<Box<RepoTab>>,
    /// Those of them with work going on, and what work.
    pub(in crate::repo) parked_status: Vec<(PathBuf, &'static str)>,
    /// The app has a dialog of its own open (palette, settings): leave the keys to it.
    pub keys_blocked: bool,
    /// Agent tasks, the worktree setup recipe and their dialogs.
    pub(in crate::repo) agents: agents::AgentsState,
    /// The bake-off of attempts shown in the big area, and its picker.
    pub(in crate::repo) bakeoff: bakeoff::BakeoffState,
    /// Long operations that just ended, its own and its parked worktrees': (message, failed),
    /// for a notification each (taken by `App`).
    pub finished: Vec<Notice>,
    /// The repository's own custom commands, and what the palette offers of them.
    pub(in crate::repo) custom: custom_commands::TabCommands,
}
