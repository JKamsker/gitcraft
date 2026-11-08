# Resident tray and recent-repository watcher (#47)

Status: implementation WIP; pure/recording/headless checks and macOS compile validation.
Native, network and application process execution remain held. This is not native acceptance.

## Reachable behavior and source coverage

- Settings → General → Resident watcher persists disabled-by-default `keep_running_in_tray`.
  Close hides the eligible root through the existing lifecycle policy, including logic-only
  frames. Explicit Quit bypasses hiding and retains the existing save/on-exit path. Hidden
  state is not persisted. Single-instance Focus shows the root and clears its hidden marker.
- `app/resident.rs` drains typed native commands only in the coordinator. Its 250 ms logic
  heartbeat continues while hidden (including the missing-Git screen). Detached window
  membership and existing dialog protection remain governed by the established window policy.
- macOS/Windows `tray/native_desktop.rs` creates and retains a real native status item/menu
  inside the running event loop, outside the Send application state. Menu callbacks enqueue
  literal arguments into a bounded nonblocking channel: Show, recent repositories, Open,
  Clone and Quit. Status is a disabled menu row. Stale menu IDs cannot resolve a new action.
- Linux `tray/native_linux.rs` uses KSNI directly, without GTK/AppIndicator. Real
  `watcher_online`/`watcher_offline` callbacks report host availability, including changes
  after startup; a missing session bus reports the actual creation failure. Background setup
  uses the shared task lane. The fallback posts a desktop notification explaining launcher
  reopen and Quit. Both packaged/dynamic Linux launcher actions and the literal `--quit`
  handoff provide explicit exit without a tray host; cold `--quit` creates no window.
- `tray/watch.rs` owns at most twelve open/recent repository subscriptions, deduplicated by
  the existing common-Git-directory mutation key (including linked worktrees), with one active
  job in all. It creates no RepoTab for closed repositories. `repo/messages.rs` defers the automatic
  fetch of subscribed open tabs to this owner; explicit operations retain the existing shared
  mutation task key. Busy open tabs are skipped. Automatic-fetch interval zero explicitly
  pauses the watcher and reports that state.
- Each job captures the exact selected profile ID and immutable connection Context. Profile,
  connection configuration, path removal, opt-out and network pauses cancel its task token;
  stale results cannot update snapshots or notify, and a new job waits for old acknowledgement.
  Existing offline/metered policy gates submission and each network stage. Fetch/local reads
  use `run_unattended_within`, no transient Credentials, thirty-second step limits, shared
  cancellation and a 120-second coordinator deadline; retries use capped exponential backoff.
- Initial observations establish a silent baseline. Later default-branch object changes use
  a real validated `rev-list --count old..new` result; missing local remote HEAD is resolved
  from bounded `ls-remote --symref`. New review requests, newly failing own-PR checks and
  default-branch check failures produce transition notifications. Missing optional provider
  answers preserve their prior baseline. Provider PR notifications deduplicate by URL/event
  across subscriptions. Existing inbox/open-tab code does not emit these PR notifications.
  CI reads the project belonging to the selected default remote through the existing API.
  Provider capability limits (including unsupported CI forges) remain explicit.

## Shared interface dependency

The imported fb00 snapshot, also inspected at root 16a76, still routes `forge::cli::run` /
`run_with_input` through raw Command output/spawn; `run_within` uses legacy child/drain code.
Therefore the outer tray task/deadline alone does **not** establish bounded/cancellable CI
and provider CLI execution. Root confirmed this and assigned its owner a finite migration to
existing Service execution, preserving the same signatures and credential policy. Adopt that
exact correction before accepting the watcher as fully bounded. This unit does not duplicate
that shared migration or claim it has passed. HTTP providers retain their existing request
limits and cancellation capabilities; no stronger interruptibility is inferred here.

## Checks and limitations

Commands use Cargo +1.95.0, CARGO_BUILD_JOBS=2 and the prescribed normalized TMPDIR/PATH/SHELL.
The immutable receipt lives outside the repository at
`/tmp/gitcraft-interaction-resident-tray-validation.json`; only this concise text is tracked.

- `cargo test --offline --lib tray:: -- --nocapture`: eight pure/recording groups cover typed
  queue bounds/literal actions, opt-in/out, fake clock/baselines/backoff, subscription bounds,
  exact quiet argv/OID/count/default-HEAD discovery, stale profile results and owned-token
  deadline acknowledgement. No live jobs are submitted in these test builds.
- Exact headless `app::windows::tests::resident_close_hides_only_when_opted_in_and_quit_bypasses_hide`:
  default close, opt-in hide, launcher Focus and literal Quit through actual adapters.
- Exact parser `launch::args::tests::resident_quit_is_an_explicit_literal_flag_not_a_folder_or_shell_command`.
- `cargo check --offline --all-targets`: actual macOS native tray adapter compiles.
- External inert harness typechecks the byte-identical production KSNI adapter with real
  ksni 0.3.7 APIs and recording task/event stand-ins on this macOS host. It constructs no
  native object, submits no work, and executes no D-Bus call. This is not Linux target/runtime
  validation. Its adapter/log/hash records remain in /tmp.
- Qualified Clippy `-D warnings -A dead_code` remains blocked by imported shared errors owned
  by root; no qualified or strict lint success is claimed for this candidate.

Native icon/menu delivery, hidden event-loop behavior, actual Linux no-host notification
fallback, Windows/Linux target builds, real credentials/network/provider polling, cancellation
and shutdown remain unexecuted under the global hold. Required application assets are reused;
no generated binary evidence or archives are committed. Full Fluent migration remains a
separate original requirement, including all strings added by this feature.
