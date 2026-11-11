//! Tab construction and its public lifecycle/query boundary.
use super::*;

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
            forge_generation: None,
            forge_operation: false,
            cancel: jobs.0.clone(),
            repo: messages::mutation_key(&path),
        };
        let branch_bands = branch_bands::Preferences::load(_ctx, &path);
        Self {
            path,
            name,
            tx,
            _jobs: jobs,
            behaviour_cache: Default::default(),
            rx,
            opened: Instant::now(),
            life: lifecycle::Lifecycle::new(Instant::now()),
            reads: 0,
            work_read: 0,
            work_pending: 0,
            work_epoch: 0,
            keyboard: Default::default(),
            submodules_read: 0,
            recheck_after_load: false,
            shown: true,
            loading: false,
            load_started: None,
            last_load_started: None,
            load_times: Default::default(),
            reload_again: false,
            load_error: None,
            refs: Refs::default(),
            graph_badge_index: graph_view::BadgeIndex::default(),
            content_recovery: Default::default(),
            working_drafts: Default::default(),
            dossier: Default::default(),
            symbols: Default::default(),
            cochanges: Default::default(),
            saved_views: Default::default(),
            history: Vec::new(),
            history_index: HashMap::new(),
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
            practice: None,
            practice_checking: false,
            practice_result: None,
            lfs_locks: Default::default(),
            lfs_storage: Default::default(),
            unity: Default::default(),
            identity: Identity::default(),
            hosting: None,
            forge_selection: Default::default(),
            forge_remotes: Vec::new(),
            forge_generation: 0,
            forge_repository_generation: 0,
            gerrit: None,
            prs: Vec::new(),
            prs_from_cache: false,
            pending_prs: None,
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
            history_filter: Default::default(),
            selected: None,
            stash_view: None,
            scroll_to: None,
            detail_hash: None,
            commit_changes: Default::default(),
            detail: None,
            detail_error: None,
            detail_cache: Default::default(),
            files_cache: Default::default(),
            detail_cancel: Default::default(),
            detail_asked: None,
            snapshot_shown: false,
            snapshot_kept: None,
            extras: Default::default(),
            diff: None,
            structural_running: None,
            structural_pending: false,
            file_comparison: None,
            compare: None,
            pr_view: None,
            review_document: Default::default(),
            review_tools: Default::default(),
            ci_view: None,
            local_ci: Default::default(),
            email: Default::default(),
            devcontainer: Default::default(),
            reflog: None,
            versions: Default::default(),
            doctor: None,
            tour: None,
            ssh_setup: None,
            insights: None,
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
            deferred: Default::default(),
            prompt_session: Default::default(),
            outbox_last: None,
            outbox_reading: false,
            release: None,
            hotfix: hotfix::Hotfix::default(),
            package: None,
            reword: None,
            push_flow: Default::default(),
            plain_rebase_request: 0,
            preflight_pending: None,
            preflight_selection: None,
            preflight_after: None,
            preflight_timeline: None,
            credentials: None,
            connection_generation: crate::connections::generation(),
            connection_context: crate::connections::current(),
            last_op: None,
            https_generation: 0,
            https_checking: false,
            https_challenge: None,
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
            resolver: Default::default(),
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
            cleanup: None,
            sparse: None,
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
            terminals: Default::default(),
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
        (!self.opening() && !self.snapshot_pending()).then(|| self.remotes.clone())
    }

    /// An editor holds work that showing something else would close: a conflict being
    /// solved, an interactive rebase being set up.
    pub(crate) fn editor_open(&self) -> bool {
        self.conflict.is_some() || self.rebase.is_some()
    }

    /// Nothing read yet: the tab shows its opening screen.
    pub fn opening(&self) -> bool {
        (self.history_gen == 0 && !self.snapshot_shown && self.load_error.is_none())
            || self.resuming
    }

    /// A startup snapshot is display-only until a successful repository read replaces it.
    pub(crate) fn snapshot_pending(&self) -> bool {
        self.snapshot_shown && self.history_gen == 0
    }

    pub fn is_busy(&self) -> bool {
        self.loading || self.busy_op.is_some() || self.snapshot_pending()
    }

    /// Queues an action from outside the tab (the command palette).
    pub(crate) fn queue(&mut self, action: Action) {
        self.queued.push(action);
    }

    /// Keep external navigation pending until fresh state can authorize its dispatch.
    pub(in crate::repo) fn take_queued_actions(&mut self) -> Vec<Action> {
        if self.snapshot_pending() {
            Vec::new()
        } else {
            std::mem::take(&mut self.queued)
        }
    }

    /// The selected primary remote's forge.
    pub(crate) fn forge(&self) -> Option<Forge> {
        self.hosting.as_ref().map(|h| h.forge)
    }

    /// Whether the tab knows its forge and can show that forge's pull requests in full.
    pub(crate) fn shows_pull_requests(&self) -> bool {
        self.hosting.as_ref().is_some_and(|h| h.forge.has_pages())
    }

    /// HTTPS credentials for this tab's git commands (from the clone that made it).
    pub fn set_credentials(&mut self, creds: git::Credentials) {
        self.invalidate_https_challenge();
        self.credentials = Some(creds);
    }

    #[cfg(test)]
    pub(crate) fn has_test_credentials(&self, username: &str, password: &str) -> bool {
        self.credentials
            .as_ref()
            .is_some_and(|c| c.username == username && c.password == password)
    }
}
