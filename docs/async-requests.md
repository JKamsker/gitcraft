# Async request ownership and stale-result witnesses (#195)

This page records, for each background request flow of a repository tab, which code owns
the request and which value proves that a reply is still current (its *witness*). It also
records what makes a request stale, what happens to a late reply, who clears busy and error
state, and whether superseding a request stops its work or only discards the result. It
describes the code as it is. It is not a design proposal.

Function paths are given without line numbers. `RepoTab` is
`repo::tab_owner::model::RepoTab`. Replies reach it through `RepoTab::handle`
(`src/repo/core_seams/message_dispatch.rs`) or through a feature's private reader, which is
polled in `RepoTab::poll_as`. **Uncertain** marks a claim that was not confirmed from the code.

## Shared mechanisms

| Mechanism | Where | Witness | Effect on late replies | Stops work? |
| --- | --- | --- | --- | --- |
| Tab channel | `messages::JobSender` (`sender`, `cancel`, `forge_generation`, `repo`); `RepoTab::{tx, rx}` | Channel identity | A reply sent to a receiver that has been dropped is lost. | No |
| Tab job scope | `tasks::Scope` (`RepoTab::_jobs`), whose `Drop` cancels its `tasks::Cancellation`; jobs inherit `JobSender::cancel` | `Cancellation` (an `AtomicBool` plus its parent chain) | Queued jobs are cancelled, and their failure continuation is sent. | Yes: queued jobs are not started, and running managed processes are stopped (see [process orchestration](process-orchestration.md#lifetime)) |
| Tab close | Dropping `RepoTab` drops `_jobs` and `rx` | Scope and channel | Every reply is lost. | Yes, as for the job scope |
| Tab suspend | `lifecycle::RepoTab::suspend` installs a new channel and `Scope`, calls `invalidate_repository_context` and `FileView::retire_reads`, and resets the private readers and graph caches | New channel and scope | Every reply already in flight is lost, including its failure continuation. A flag set before the suspend stays set (see [Gaps](#gaps)). | Yes, as for the job scope |
| Suspend guard | `lifecycle::RepoTab::idle` | — | A tab suspends only when it has no `loading`, `busy_op`, snapshot, `loading_more`, `counting_worktrees`, queued actions, PR view, CI view, local CI or unfinished work. | — |
| Forge generation | `forge_selection::RepoTab::forge_sender` sets `JobSender::forge_generation = Some(RepoTab::forge_generation)`. `send` wraps the reply as `Msg::ForgeResult(generation, operation, msg)` | `RepoTab::forge_generation: u64`, advanced by `bind_forge_selection` | `handle` drops the reply unless `generation == forge_generation`. A `PrPosted` for the same repository with `generation >= forge_repository_generation` is still delivered. A stale *operation* clears `busy_op` and reloads instead (see [Operations](#operations-and-busy-state)). | No |
| Context retirement | `lifecycle::RepoTab::invalidate_repository_context` / `invalidate_forge_context`, which call `invalidate_repository_insights` (or `invalidate_forge_insights`), `Search::retire`, `invalidate_repository_tool_requests` and `cochanges::State::invalidate_context`, and recurse into `parked`. The repository variant is called by `shown_now`, `suspend`, `refresh_on_focus` (window focus and F5), `invalidate_https_challenge` (connection change, or remotes changed on reload), a profile change in `app::profiles`, and worktree park and show. The forge variant is called by `bind_forge_selection`. | Per feature (below) | The feature's witness no longer matches. `invalidate_repository_tool_requests` also closes an open Repository tools dialog. | Commit search cancels its child token; after `refresh_on_focus` a search that was in flight is asked again once the reload is in (`search_after_load`). For the others, see their rows. |
| `Msg::Then` closures | `job_submission::RepoTab::{then, then_or, here_or_later}`; `forge_then` | None of its own | The closure runs against whatever state the tab has when the reply arrives. Each caller must check its own witness. | No |
| Submission failure | `job_submission::spawn_with`: queue rejection, cancellation while queued, or a panic sends `failed(error)` | — | `spawn` uses `Msg::JobPanicked`, which only sets `RepoTab::status`. `spawn_or` and `then_or` callers pass a feature-specific failure that releases their busy flag. | — |

A request follows the same path in every flow below. Each flow table names the witness that
the final check compares.

```mermaid
stateDiagram-v2
    [*] --> Issued: feature records witness (counter, id, Arc, flag)
    Issued --> Queued: spawn / spawn_or / then_or / reader.queue
    Queued --> Cancelled: tab scope or child token cancelled
    Queued --> Running: worker picks job
    Running --> Replied: job sends Msg on tx
    Cancelled --> Replied: failure continuation
    Replied --> Lost: receiver replaced (suspend) or dropped (close)
    Replied --> Checked: RepoTab::handle / reader poll
    Checked --> Applied: witness matches current state
    Checked --> Discarded: witness superseded
    Applied --> [*]
    Discarded --> [*]
    Lost --> [*]
```

## Repository reads

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| Full reload | `core_seams::load_requests::RepoTab::reload`, which sends `Msg::Loaded` through `spawn_or` | None on the reply. At most one reload runs per tab (`RepoTab::loading`); a reload asked for meanwhile sets `reload_again`. `Loaded::read` (from `RepoTab::reads`) orders only the working tree and the submodules. | Suspend and close (lost reply). A reload while sleeping calls `start_deferred` instead. | `load::RepoTab::loaded` always applies refs, history, stashes, remotes and the other fields. It applies `work` only when `newer(read, &mut work_read)`, and submodules only when `newer(read, &mut submodules_read)`. | `loaded` clears `loading` and sets or clears `load_error`. A panic or rejection comes back as `Msg::Loaded(Err)`. | Never superseded (single flight). Only tab close cancels it, because `idle()` blocks suspend while `loading`. Leaf reads use `tasks::read` and inherit the tab token. |
| Working-tree status | `load_requests::RepoTab::reload_status`, which sends `Msg::Status(read, …)` through `spawn` | `RepoTab::work_read: u64` (last applied) and `work_pending: u64` (last asked), from `RepoTab::reads` | Any newer read: a reload, another status read, or a submodule read (all share `reads`) | `handle` drops `Status(read, _)` when `read <= work_read`. `load::RepoTab::status_arrived` applies `Ok` and sets `work_read`. An `Err` only sets `status`. | `status_arrived` and `take_loaded` clear `work_pending` when `read >= work_pending`. Nothing clears it after an `Err` or a rejected job (see [Gaps](#gaps)). | Discard only |
| Submodules | `load_requests::RepoTab::reload_submodules`, which sends `Msg::Submodules(read, list)` | `RepoTab::submodules_read: u64` | A newer read | `handle` applies the list only when `newer(read, &mut submodules_read)`. | No busy flag | Discard only |
| Refs check | `load_requests::RepoTab::recheck`, which sends `Msg::RefsCheck` | None | — | `handle` reloads when the refs differ from `RepoTab::refs`. Lateness only causes an extra reload. | — | — |
| Identity | `load_requests::RepoTab::reload_identity`, which sends `Msg::Identity`; `take_loaded` also sets `identity` | None | — | The last reply to arrive wins, whether it came from `Identity` or `Loaded`. **Uncertain:** whether an out-of-order reply can show an outdated identity in practice. | — | — |
| Worktree counts | `load_requests::RepoTab::count_worktrees`, which sends `Msg::WorktreeCounts` | Single flight: `RepoTab::counting_worktrees`, with `recount_worktrees` for a follow-up | — | `worktrees_counted` applies counts by path. | `worktrees_counted` clears `counting_worktrees`. The failure continuation sends empty counts. | Discard only |
| History pages | `load_requests::RepoTab::load_more`, which sends `Msg::More(generation, excluded, page)` | `RepoTab::history_gen: u64`, advanced by `take_loaded`. `log_stream` is keyed by the same generation. | A reload (`take_loaded`). Suspend clears `log_stream`. | `handle` drops a reply whose `generation != history_gen`. A reply whose excluded namespaces no longer match only clears `loading_more` and `log_stream`. `load::RepoTab::more_loaded` skips hashes it already has. | `more_loaded` (current generation) and `take_loaded` clear `loading_more`. | Discard only. The `git log` stream is dropped. |
| Navigation to a commit not loaded yet | `navigate::target::RepoTab::goto_target` keeps `RepoTab::pending_goto: Option<PendingGoto>` while it pages | The `PendingGoto` value | `retire_navigation_restore`, on a direct selection and on other destination actions; `suspend` | A page that arrives after the user picked something else finds `pending_goto == None` and does not select anything. | — | — |

## Views

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| Commit detail | `navigate::RepoTab::fetch_detail` sends `Msg::Detail` or `Msg::DetailPrefetched` through `spawn_selection` | `RepoTab::detail_hash: Option<String>` plus `RepoTab::detail_cancel` (a child of the tab token) | A new selection, which cancels `detail_cancel` | A cancelled job replies as `DetailPrefetched`, which only fills the caches. `detail_cache::RepoTab::detail_arrived` always caches, and shows the detail or error only when `detail_hash` matches. Commit content is immutable, so an A→B→A match still shows the correct commit. | `detail_arrived` (`detail_error`) | Yes: the queued job is cancelled and the process observes `detail_cancel`. |
| Detail extras | `detail::extras::RepoTab::load_extras` | `Extras::hash` | A new commit | Dropped for another commit (tested) | — | Discard only |
| All files of a commit | `detail::all_files::AllFiles::wait_for`, which sends `Msg::TreeFiles(hash, …)` | `AllFiles::of: Option<String>` | Another commit | `AllFiles::arrived` drops other hashes. | — | Discard only |
| Files prefetch | `navigate::RepoTab::prefetch_files`, which sends `Msg::FilesPrefetched` | Single flight: `FilesCache::reading` | Suspend resets `reading`. | Only fills the cache | `files_prefetched` clears `reading`. | Discard only |
| Diff text | `diff_navigation::RepoTab::refresh_diff`, which sends `Msg::Diff(request, …)` through `spawn` | `DiffView::request: u64`, from the process-wide `state::DIFF_REQUESTS` counter. A reopened view never reuses an id. | `DiffView::next_request` (a refresh or reopen), closing the view, suspend (`diff = None`) | `DiffView::arrived` drops `request != self.request`. | No busy flag. `data` stays `None` until the current reply arrives. | Discard only |
| Diff file, rich and rendered views | `load_diff_file`, `load_diff_rich`, `load_diff_rendered` | `DiffView::request`, plus `rich_load` and `rendered_load` | A new diff request, a new rich or rendered load | `DiffView::file_arrived`, `rich_arrived` and `rendered_arrived` compare both ids. | — | Discard only |
| Structural diff | `diff::structural::RepoTab::load_diff_structural`, which sends `Msg::DiffStructural(ids, …)` | `RepoTab::structural_running: Option<(u64, u64)>`, with `structural_pending` for one follow-up | A newer diff request, or a pending follow-up | `structural_arrived` drops ids that are not running. `Structural::arrived` applies only when `ids.0 == view.request`. | `structural_arrived` clears `structural_running`. Suspend does not (see [Gaps](#gaps)). | Discard on supersede. Tab close cancels the preparation and the tool (tested). |
| Conflict editor (remote) | `diff_navigation::RepoTab::open_diff_of_lines`, which sends `Msg::ConflictOpened(generation, …)` | `RepoTab::conflict_generation: u64`, plus the open diff's path | Another diff opening | `conflict_opened` and `conflict_changed` require the generation, the path and an open conflicted diff. | `ConflictSaved` clears `saving` by path. | Discard only |
| File history, blame and text | `core_seams::file_navigation` loads through `then_or`; the replies are re-dispatched as `Msg::FileHistory`, `Msg::Blame` and `Msg::FileText` | `FileView::generation` and `blame_generation` (from `file_view::next_generation`), the `FileView::pending` slot tokens, and the probe's `ProbeState::active` and `input_check` | `invalidate_mutable_file_view` (a reload, or a status change seen by the probe), `retire_reads` (suspend), opening another file | `file_view::loading::RepoTab::finish_file_load` requires the slot token and the generation. `handle` then re-checks the generation, path and revision. `finish_file_probe` requires `probe.active == check`. | `finish_file_load` frees the slot and schedules a retry on `tasks::BUSY`. | Discard only |
| Comparison | `navigate::RepoTab::reload_compare` and `pr_view::select`, which send `Msg::Compare(request, …)` | `Compare::request`, from `RepoTab::compare_requests: u64`, and `RepoTab::pr_selection_request: Option<u64>` | A new comparison. A file-system reload does not replace a pending PR selection. | `handle` applies the reply only when `c.request == request`. | `pr_selection_arrived` clears `pr_selection_request` only for the matching request. | Discard only |
| Interactive rebase editor | `rewrite::RepoTab::interactive_rebase`, which sends `Msg::RebaseCommits` | None | — | For a non-empty `Ok`, `rewrite::RepoTab::open_rebase` closes `diff`, `conflict` and `file_view` and replaces `rebase` (see [Gaps](#gaps)). Running the rebase is guarded separately by `rebase_ready` (the editor id, the branch guard, and `HEAD` unchanged). | — | — |

## Forge views

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| PR list | `load_requests::RepoTab::load_prs`, which sends `Msg::Prs(asked, …)` through `forge_sender` | `RepoTab::prs_loaded: Option<Instant>`, plus `forge_generation` | A newer `load_prs`, `retire_pr_list` (refresh), `bind_forge_selection`, `notice_connections` | `handle` drops both `Ok` and `Err` when `prs_loaded != Some(asked)`, so a stale `Err` cannot set `pr_error`. | `prs_arrived` (current request only) | Discard only |
| PR detail and files | `pr_view::RepoTab::load_pr`, which sends `Msg::PrDetails(request, …)`; the files follow with the same id | `PrView::request`, from `RepoTab::pr_requests: u64`, plus `forge_generation` | A newer `load_pr`, `notice_connections` (`PrView::account_changed`), `close_pr`, `bind_forge_selection` | `pr_details_arrived` and `pr_files_arrived` require `v.request == request`. | Those two functions clear `PrView::loading` (current request only). A failed or refused read answers with `Err` (`spawn_or`). `PrDraft::settling` holds the request that settles it (see PR posting). | Discard only |
| PR posting | `pr_review::send`, which sends `Msg::PrPosted { hosting, … }` | No request id. `PrDraft::posting` is single flight per PR number. `hosting` is compared with `load::same_repository`. | A move to another repository | Replies for another repository are dropped in `handle`. `pr_posted` always settles the draft. | `pr_posted`; then `settling` (the id of the read `pr_posted` started, moved to a newer `load_pr` of the same page) is released by that read's answer whichever PR is shown: details that are an error or for a page no longer shown, or the files. `bind_forge_selection` releases it too. | No (it is a mutation) |
| CI status dots | `ci_status::RepoTab::load_ci`, which sends `Msg::CiStatuses(fresh, …)` through `forge_sender` | Single flight: `CiStatuses::loading`, with `superseded: bool` for a changed project set or account. `fresh` is not an identity. | `CiStatuses::account_changed` and `moved_to` keep `loading` and set `superseded`. `bind_forge_selection` resets `ci`. | `CiStatuses::arrived` drops a superseded reply and schedules a new read. | `CiStatuses::arrived` clears `loading`. Suspend does not (see [Gaps](#gaps)). | Discard only |
| CI explorer runs, jobs and log | `ci_view` (`load_ci_view`, `load_ci_jobs`, the log toggle) | `CiView::request` and `log_request`, from the process-wide counter `ci_view::REQUESTS`; jobs also match the run | `CiAct::SelectRun`, reopening, `bind_forge_selection` | `ci_runs_arrived`, `ci_jobs_arrived` and `ci_log_arrived` filter on their ids. | — | Discard only |
| CI re-run and cancel | `ci_view::RepoTab::ci_act`, which sends `Msg::CiActed(label, …)` | None. `CiView::acting` is single flight per view. | — | `ci_acted` clears `acting` on whichever `ci_view` is open (see [Gaps](#gaps)). | `ci_acted` | No (it is a mutation) |
| Issue picker | `issue_work`, via `issue_then` (`forge_sender` for forge issues, the plain `tx` for trackers) | `IssueWork::requests: u64`, the connection context, and whether the picker is open | A newer request, `forge_issues_changed`, a connection change | The continuation drops non-matching replies. | — | Discard only |
| Issue refresh | `issue_work` `refresh_issue` | Single flight: `IssueWork::refreshing: Option<String>` | `forge_issues_changed` (called by `bind_forge_selection`) clears it | The continuation clears `refreshing` without checking which refresh it belongs to (see [Gaps](#gaps)). | The continuation | Discard only |
| HTTPS sign-in retry | `https_recovery` | `https_recovery::Ticket` (`generation` from `RepoTab::https_generation`, the path, the connection context and the remotes), checked by `https_ticket_current` | `invalidate_https_challenge`, called from `notice_connections`, from `take_loaded` when the remotes change, and from `forget_https_memory` | The `sign_in` and `switch_https` continuations refuse a ticket that is not current. `last_op` is re-run only for a current ticket. | `invalidate_https_challenge` clears `https_challenge` and `https_checking`. | Discard only |

## Feature readers

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| Commit search | `commit_search::RepoTab::run_search`, which sends `Msg::Search(request, …)` | The pointer identity of `Search::pending: Option<Arc<Request>>` (`Arc::ptr_eq`), then `Request::current`, which re-checks the path, the query and mode, the roots, the order, the excluded namespaces and the connection generation (`u64::MAX` is refused) | A new `run_search`, `toggle_search`, context retirement (after a focus refresh or F5, a search in flight is asked again once the reload is in), `Drop`, suspend | `search_done` ignores a reply that is not pending. A pending reply that is no longer current only retires it. | `Search::retire` clears `running`. Only a current reply sets `problem` or calls `goto`. | Yes: `Request::cancel` is the job's cancellation token. |
| Cochange | `cochanges::State` (`poll_cochanges`, then `State::context`, then `State::read`) | `cochange_owner::requests::Requests { generation, pending: Option<Request> }`, with `checked_add` (exhausted means no new read), plus the reader's own generation | A head or repository change, `invalidate_context`, hiding or sleeping (`State::suspend`), `Drop` | `State::complete` calls `Requests::finish`. Stale success and error are both dropped. | `reader.busy()`; `State::error` is set only by an accepted reply. | Yes: `Reader::cancel` cancels the child scope. |
| Repository tools | `Dialog::RepositoryTools(Owner)` and `RepoTab::deferred: DeferredCache`; replies come as `Msg::Then` | `repository_tools::lifecycle::Ticket<S> { origin, scope }`. `Ticket::settle` requires `origin == Owner::id` (from the process-wide counter `REQUESTS`) and `scope ==` the current `owner::Scope` (path, profile, `DeferredCache` epoch, connection, choices, sign-in generation). `poll_deferred` and `poll_outbox` require `pending` or `outbox_poll == Some(request)`. | `invalidate_repository_tool_requests` (`Cache::reset_requests` advances the epoch with `checked_add(..).expect(..)`, so an exhausted epoch panics, and clears `pending`, `automatic`, `outbox_poll`, `reading` and markers; the caller also resets `outbox_reading` and `outbox_last` and closes the dialog), a scope change, closing or restarting the dialog | `Ticket::settle` returns `Completion::Ignore` for a stale ticket, and `Owner::complete` then applies nothing. Mutation completions always reach `op_done` first. | `reset_requests` clears `reading`; `invalidate_repository_tool_requests` clears `outbox_reading` and `outbox_last`. Only a current failure writes the result or a notice. | Discard only, until suspend or close drops the tab scope |
| Branch bands and graph topology | `history_reader::Reader<T>` (`RepoTab::band_reader`, `topology_reader`) | `Reader`'s private `generation: u64` (`wrapping_add`) | A new `queue` (every graph rebuild re-queues), `cancel`, suspend (the reader is replaced), `Drop` | `Reader::poll` returns only the current generation's result. | `Reader` | Yes: a child token and an `active()` check |
| Line dossier, symbol biography, saved views, content recovery, working drafts | `saved_views::reader::Reader<T>` in each feature | The reader's `generation` (saved-view pages also match `history_gen`) | Hiding, closing the feature or dialog, `cancel` | Dropped by generation. These readers are not retired by `invalidate_repository_context`. **Uncertain:** whether a late working-drafts preview can replace another dialog while `loaded` is false. | The reader | Yes: the reader cancels its scope. |
| Insights | `insights::context::Read { namespace, lifetime: Weak<()> }` | `Read::current`, which checks the `Weak` pointer and the namespace generation | `invalidate_repository_insights`, which replaces `read_lifetime`; `invalidate_forge_insights` | Dropped | Per sub-reader | Discard only |

## Tab, worktree, watcher and remote lifecycle

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| Suspend and resume | `lifecycle::RepoTab::{background_tick, suspend, shown_now, reopen_after_resume}` | New channel and `Scope`; `RepoTab::resume_select` and `resume_diff` hold the place | `Lifecycle::background` after `IDLE_AFTER` (30 min) with `idle()` true | Lost. `loaded` takes `resuming` and calls `reopen_after_resume` whether the reload succeeded or failed. The selection is restored only if its hash is in `index`. | `loaded` clears `resuming`. | Yes, the old scope is cancelled. |
| Tab close | Dropping `RepoTab` (it has no `Drop` impl of its own) | Scope and channel | `app::tabs::close_tab` | Lost. Late sends fail silently. Parked views drop with the tab. `Terminal::drop` kills its PTY child. | — | Yes, including running mutations, whose `OpControl` is a child of `tx.cancel` (built by `running::RepoTab::follow_op`) |
| Worktree park and show | `RepoTab::parked: Vec<Box<RepoTab>>`, polled by `worktree_tab::RepoTab::poll_parked`, which calls `poll_parked_view` (`shown = false`) | Each parked view keeps its own `tx`, `rx` and `_jobs` | Parking calls `invalidate_repository_context` and drops the remote `poller`. Showing calls it again, then rewatches a remote view and reloads. | Replies reach the parked view's own `rx`, so they apply only to that view. File-change reloads, fetch and CI are skipped while parked. | Each view clears its own state. | Reads and mutations keep running. A view evicted by `parked_to_drop` is dropped, which cancels its work. |
| Folder watcher | `watch::RepoTab::watch`, which sends `Msg::Watcher(watch, …)` | `RepoTab::watches: u64` | A new `watch`, suspend | `handle` accepts a watcher only when `watch == watches` and the tab is not sleeping. Otherwise it is dropped, which stops the OS watch. | — | **Uncertain:** whether `start_watcher` observes cancellation while it walks the tree |
| Remote polling (SSH/WSL) | `watch_remote::Poller` | One look at a time (`Poller::running: Arc<AtomicBool>`, reset by a guard even when a queued look is purged), its own `Scope` (a child of the tab token), and the `watches` stamp | Replacing the poller, park, suspend | A cancelled look sends nothing. A late look sends to an `fs` sender whose receiver is gone, and its `Watcher` stamp is rejected. | The `Finished` guard clears `running`. | Yes: dropping the poller cancels its scope. |
| Remote shell reads | `remote::shell::Shell` and its pool | The job's ambient cancellation (`tasks::cancellation()`) | Tab scope, ticket drop | A cancelled read returns `TimedOut`, and `begin_cleanup` closes stdin so the remote side stops the command. `output_written_late_stays_out_of_the_next_answer` covers late output. | — | Yes (cooperative). A broken network can prevent confirming the remote cleanup. |
| Connection or credential change | `load_requests::RepoTab::notice_connections` | `RepoTab::connection_generation` and `connection_context` | `connections::generation()` or `current()` changing | Clears credentials, the PR list, CI, `ci_view` and avatars; bumps `pr_requests`; sets `prs_loaded` to a fresh `Instant` (so older `Prs` replies no longer match); recurses into `parked`. Jobs already running keep the context they captured. | Same function | Discard only |
| SSH setup dialog | `ssh_setup::State` | A private channel per request plus an `active` flag | `cancel_worker`, a profile change | Discarded by `poll` | The dialog | The worker runs under a parentless `Scope`, so it stops only when the dialog state is dropped. The dialog counts as unfinished work, which blocks suspend. |
| Terminals | `terminals::Terminals`, `terminal::Terminal` | — | — | No request/reply flow: `Terminal::spawn` is synchronous and PTY I/O stays on the terminal's own threads. No stale-result risk. | — | `Drop` kills the child. |

## Operations and busy state

| Flow | Owner | Witness | Invalidated by | Late or stale reply | Busy and error cleared by | Stops work? |
| --- | --- | --- | --- | --- | --- | --- |
| Git operations | `operation_plan::start::RepoTab::run_ops_ex` and the other starters; `RepoTab::{busy_op, running, waiting}` | None on `Msg::OpDone`. `busy_op` admits one operation per tab, and the reply is delivered exactly once (success, panic, rejection and cancellation share one failure slot in `spawn_with`). The audited starters all return early while `busy_op` is set; that this covers every starter is **uncertain**. | — | `operation_plan::completion::RepoTab::op_done` always applies. | `op_done` clears `busy_op`, takes `running`, and runs `after_op`, which starts the next item in `waiting`. | `cancel_op` cancels `OpControl`. A queued job is purged and still delivers `OpDone(Err)`. A running one stops its process. |
| Forge operations (create PR, stack publish, fork approval) | `forge_operation_sender` (`forge_operation = true`) | `forge_generation` | `bind_forge_selection` (which does not clear `busy_op`) | `handle` drops the continuation, calls `stale_fork_operation_finished`, clears `busy_op`, `op_started` and `running`, sets a notice and reloads. Because `busy_op` still belongs to that operation, clearing it is correct. It then starts what waited for it, or drops it after a cancel or failure (see [Gaps](#gaps), item 3). | The stale branch in `handle` | No |

## Gaps

These were found while writing this page. They are not fixed here.

### Stale replies and busy state

1. **CI dots stop updating after a suspend that interrupts a CI read.** `ci_status::RepoTab::load_ci`
   sets `CiStatuses::loading`. `lifecycle::RepoTab::idle` does not check that flag, and
   `suspend` does not reset it, so the reply is lost with the old channel. After resume,
   `take_loaded` and `refresh_ci` reach `load_ci`, which returns early because `loading` is
   still true. `poll_ci` never fires, because `load_ci` had cleared `ci.next`. `account_changed` and `moved_to` keep the flag. Only
   `bind_forge_selection` (`self.ci = Default::default()`) clears it.
   Background tabs keep polling CI (`app::frame` polls every tab with `shown = true`), and
   `background_tick` runs right after `poll` in the same frame. A read started by
   `poll_ci` in that frame therefore lands in flight when the tab suspends.
   *Reproduce:* make the tab idle, set `ci.loading = true` (or record the `load_ci` job
   without running it), set the lifecycle's `shown_at` back by `IDLE_AFTER`, call
   `background_tick` and then `shown_now`, and check that no CI job is submitted.
2. **An old CI re-run or cancel releases a newer one's busy state.**
   `ci_view::RepoTab::ci_acted` sets `acting = false` on whichever `ci_view` is open.
   `Msg::CiActed` carries no request or run id, and `open_ci` replaces the view with
   `acting: false`.
   *Reproduce:* open CI for commit A and press Cancel. Before it answers, open CI for
   commit B and press Re-run. A's reply re-enables B's buttons while B's request is still
   running, so a second action can be sent.
3. **Fixed: a stale forge operation left queued actions stranded.** The stale-operation
   branch of `RepoTab::handle` (`Msg::ForgeResult` with `operation = true`) now goes through
   `running::release::RepoTab::stale_forge_operation_ended`. It also clears `running` and calls
   `running::release::RepoTab::busy_op_released`, which runs `running::RepoTab::after_op` once
   the slot is free, so what waits in `RepoTab::waiting` starts. As after `op_done`, the
   waiters are dropped instead (and named in the status) when the operation was cancelled or
   its `OpDone` reply failed; a fork approval's reply is a continuation, so its outcome is not
   known and counts as done. When a waiter replaces the status line, the stale operation's
   notice stays as a toast (`finished`). Like `op_done`, it starts the waiter before its reload
   lands: Fetch and Pull read what they need from Git when they run, and Push uses the branch
   checked out, which a forge mutation does not move. `direct_tests/stale_operation_queue.rs` reproduces it: Create PR, Fetch queued, the
   primary forge changed in another tab of the same repository (and, separately, a remote URL
   change), and checks that Fetch starts.
   The feature completions that set `busy_op = None` themselves call `busy_op_released` too,
   among them `forks::approval_result` (`forks/busy_release_tests.rs`), `cleanup`, `bisect`,
   `email`, `devcontainer`, `release` and `resolver`. `busy_op_released` does nothing while
   another operation holds the slot, so a preparation step that went on to start its
   operation leaves the waiters to that operation's `op_done`. Its `ok` is false only for a
   failure the status line reports, where the dropped waiters are named.
   **Remaining:** `fence_view`'s check before an operation leaves waiters queued when it opens
   its confirmation dialog, so that they do not run ahead of the operation asked first. If the
   dialog is dismissed, they wait until some later operation ends.
4. **Fixed in [#264](https://github.com/JKamsker/gitcraft/pull/264): a focus refresh dropped a running commit search and did not restart
   it.** `RepoTab::refresh_on_focus` (and F5 through `refresh`) calls
   `invalidate_repository_context`, which calls `Search::retire`. `run_search` had already
   cleared `Search::edited`, so nothing re-ran the search after the reload, and the box
   showed the new query while `searched` and `results` still belonged to the previous one.
   `refresh_on_focus` (through `refresh_search`) now marks a search that was in flight
   (`Search::rerun`, keeping `running` set), and `loaded` asks it again
   (`search_after_load`) once the reload is in and no further reload is queued, whether
   the reload succeeded or failed. Asked straight away it would capture the old `log_from`
   and order, and a reload that changes them (a hidden branch deleted elsewhere) retires it
   again in `take_visibility` or `take_order`. The old request's child token is still
   cancelled, and a closed, cleared, edited or answered search is not re-run
   (`commit_search::refresh_tests`).
   The same retirement closes an open Repository tools dialog
   (`invalidate_repository_tool_requests`). **Uncertain:** whether that is intended.
5. **Fixed in [#264](https://github.com/JKamsker/gitcraft/pull/264): a PR draft could stay in `posting`.** After a successful post,
   `pr_review::send`'s `pr_posted` sets `posting` and `settling` and calls `load_pr`. If the
   PR view was closed, another PR was opened, or the account changed before that read
   answered, `pr_details_arrived` and `pr_files_arrived` dropped the reply (wrong `request`),
   so `settling` was never released and `forge_change_blocked` stayed true. Now `settling`
   holds the id of the read that settles it, and that read's answer settles the draft
   whichever PR is shown; a newer `load_pr` of the same page takes it over. Both reads use
   `spawn_or`, so a refused or panicking read answers too. `forge_change_blocked` counts only
   drafts whose post is still on its way (`PrDraft::sending`): a draft waiting only for its
   page to be read again is released by `bind_forge_selection` anyway
   (`pr_review::settling_tests`).
6. **An old issue refresh clears a newer one's flag.** The `refresh_issue` continuation
   in `issue_work` sets `refreshing = None` before checking anything. A refresh of a tracker
   issue uses the plain `tx`, so it is not dropped by the forge generation. After a forge
   change (`forge_issues_changed` clears the flag) and a new refresh, the old continuation
   clears the new flag and a second refresh can start.
7. **A failed `spawn` leaves some busy flags set.** `spawn` turns queue rejection,
   cancellation and panics into `Msg::JobPanicked`, which only sets `status`. These callers
   have no other way to clear their flag:
   - `reload_status` leaves `work_pending` set. The same happens when the current status
     read returns `Err`. Keyboard staging (`keyboard_commands`, `partial_patch_edits`) and
     the staged test lane refuse to act while `work_pending != 0`, until the next read
     succeeds.
   - `issue_work`'s `issue_then` leaves `refreshing` set.
   - Fixed in [#264](https://github.com/JKamsker/gitcraft/pull/264): `load_pr` and the files read that follows use `spawn_or`, so they no
     longer leave `PrView::loading` or `PrDraft::settling` set.
   - `pr_review::send` leaves `PrDraft::posting` set, which keeps `forge_change_blocked`
     true. `bind_forge_selection` releases `posting` only for drafts that are `settling`.
   - `ci_view::RepoTab::ci_act` leaves `CiView::acting` set, and `load_ci_view` and
     `load_ci_jobs` leave `CiView::loading` set.
   - `refresh_diff` leaves `DiffView::request != data_request`. Until the diff is read
     again, keyboard hunk commands (`keyboard_commands`) and partial-patch edits refuse to
     act, and the view keeps showing the previous data, or nothing for a first read.
8. **Suspend keeps other single-flight flags set.** `idle()` does not check
   `structural_running`, `issue_work.refreshing` or `pr_selection_request`, and `suspend`
   resets none of them. A request still in flight when the tab suspends leaves its flag set
   after resume. For `structural_running`, `load_diff_structural` would then keep deferring
   (`structural_pending`). This needs a request outstanding at the moment of suspension,
   which is unlikely except for CI (item 1).
9. **The interactive rebase read has no witness.** `Msg::RebaseCommits` carries no request
   id. `rewrite::RepoTab::open_rebase` closes the diff, the conflict editor and the file view,
   and replaces `rebase`, whenever a non-empty `Ok` reply arrives. An empty list or an `Err`
   only sets `status`. Two `interactive_rebase` requests that
   complete out of order leave the older editor open. Running the rebase is still guarded:
   `run_rebase` and `rebase_ready` check the editor id, `same_branch` and `HEAD`.
10. **The identity has no witness.** Both `Msg::Identity` and `take_loaded` overwrite
    `RepoTab::identity`, so the last reply to arrive wins. **Uncertain:** whether an
    outdated identity can be shown in practice.
11. **A failed resume reload loses the place.** `loaded` calls `reopen_after_resume` after an
    `Err` too. `index` is empty at that point, so `resume_select` is used up without being
    restored, while `resume_diff` is reopened.

### Missing or unrun tests

- No test sends a stale `Msg::PrDetails` or `Msg::PrFiles` against a newer `pr_requests`.
- No CI explorer test covers stale `CiRuns`, `CiJobs` or `CiLog` replies. The `CiActed`
  replies in `forge_selection_tests.rs` test only the forge-generation drop, not a newer
  view's `acting`. No test covers `ci.loading` across a suspend.
- No test covers `watch_remote::Poller` cancellation or stale looks, in-flight requests
  across park and show, or the stale forge-operation path together with `waiting`.
- No test covers `cochange_owner::requests::Requests` exhaustion.
- `src/repo/repository_tools/lifetime_checks.rs` is not a module of the crate. It is
  compiled only by `src/repo/repository_tools/pure_checks.py`, which neither CI nor
  `ci/check.py` runs, so `cargo test` does not run those A→B→A and suspend checks.

### Existing coverage

Stale, late, retire and suspend behaviour is tested in these modules:

- Reload and status: `repo::tests` (`an_older_read_of_the_working_tree_does_not_replace_a_newer_one`,
  `an_older_diff_answer_does_not_replace_a_newer_one`) and `repo::tests::request_lifetimes`
  (`rapid_reloads_finish_and_coalesce`, `closing_a_tab_stops_its_background_child`,
  `feature_failure_continuation_runs_when_a_queued_job_is_cancelled`).
- Lifecycle: `repo::lifecycle::tests` (`direct_tests/lifecycle_tests.rs`: suspension of
  prefetch, file-view slots and private readers, resume, `a_watcher_from_before_is_dropped`).
- Diffs: `repo::diff::structural::tests` (`stale_answers_cannot_replace_a_newer_diff_or_read`,
  `closing_tab_cancels_preparation_and_tool_and_prevents_later_effects`) and
  `repo::diff::encrypted::tests` (`close_reload_drop_and_late_answers_discard_private_results`).
- Selection, navigation and detail: `repo::commit_changes` (`link_tests`, `selection_tests`),
  `repo::navigation::restoration_tests`, `repo::navigate::target::tests`,
  `repo::detail::extras::tests` (`answers_for_another_commit_are_dropped`),
  `repo::keyboard::tests::focus_and_targets` (`stale_status_path_diff_and_conflict_targets_never_execute`).
- File view: `repo::file_view::{tests, tests_probes, tests_reload}`.
- Conflicts and rewrites: `repo::conflict::tests` (`results_for_an_earlier_editor_are_not_taken`) and
  `repo::rewrite::tests` (`direct_tests/rewrite_tests.rs`).
- Forge: `repo::load::tests` (`an_older_list_of_pull_requests_answering_last_is_dropped`),
  `repo::forge_selection::tests` (`forge_selection_tests.rs`), `repo::ci_status` (`statuses_asked_of_the_project_before_are_dropped`),
  `repo::https_recovery`, and `repo::issue_work::tests::forge_identity`.
- Feature readers: `repo::commit_search::tests`, `repo::cochange_owner::{requests, lifecycle_tests}`,
  `repo::repository_tools::lifecycle::tests`, `repo::repository_tools::owner` tests (`owner_tests.rs`),
  `repo::repository_tools::deferred::request_tests`, `repo::history_reader::tests`
  and `repo::saved_views::reader::tests`.
- Processes and jobs: `tasks::tests`, `processes::tests`, `remote::shell::tests`,
  `repo::running::tests`, `repo::ssh_setup::tests` and `repo::terminals::tests`.
