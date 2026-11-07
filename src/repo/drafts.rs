//! Opt-in draft lifecycle. Work is bounded/background; generations only cancel automatic
//! captures, while a confirmed single-file recovery owns its own lifetime and safety copy.
use super::messages::spawn_or;
use super::{Dialog, Msg, RepoTab};
use crate::git::drafts as store;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

pub(super) const CHECK_FAILED: &str = "Working draft safety check:";
const PAUSE: Duration = Duration::from_secs(15);
// The existing remote transport can return cancellation before its detached command
// reaches its own 15s deadline. Keep replacements coalesced through that whole bound.
const REMOTE_READ_DRAIN: Duration = Duration::from_secs(15);
type ReadJob = Box<dyn FnOnce(&dyn Fn() -> bool) -> Result<Read, String> + Send>;
struct PendingRead {
    id: u64,
    context: crate::connections::Context,
    read: ReadJob,
}

static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(super) struct State {
    pub enabled: bool,
    pub error: Option<String>,
    pub undo: Option<(store::Id, String)>,
    loaded: bool,
    config_running: bool,
    config_id: u64,
    generation: Arc<AtomicU64>,
    running: Option<u64>,
    edited: Option<Instant>,
    alive: Arc<()>,
    reader_generation: Arc<AtomicU64>,
    reader_running: Option<(u64, bool)>,
    reader_pending: Option<PendingRead>,
    reader_not_before: Option<Instant>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            error: None,
            undo: None,
            loaded: false,
            config_running: false,
            config_id: 0,
            generation: Arc::new(AtomicU64::new(0)),
            running: None,
            edited: None,
            alive: Arc::new(()),
            reader_generation: Arc::new(AtomicU64::new(0)),
            reader_running: None,
            reader_pending: None,
            reader_not_before: None,
        }
    }
}
impl State {
    pub fn observe(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        if self.enabled {
            self.edited = Some(Instant::now());
        }
    }
    pub fn cancel_pending(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.edited = None;
    }
    fn due(&self, now: Instant) -> bool {
        self.enabled
            && self.running.is_none()
            && self
                .edited
                .is_some_and(|at| now.saturating_duration_since(at) >= PAUSE)
    }
}
pub(super) enum Read {
    History {
        on: bool,
        versions: Vec<store::Draft>,
        error: Option<String>,
    },
    Recovery(Box<store::RestorePreview>),
}
#[derive(Clone)]
pub(super) enum Change {
    Setting,
    Snapshot,
    Recovery(String),
}
pub(super) enum Changed {
    Setting(bool),
    Snapshot(Option<store::Draft>),
    Recovery(store::Id),
}

impl RepoTab {
    pub(super) fn poll_drafts(&mut self, ctx: &egui::Context) {
        self.sync_draft_reader();
        self.start_draft_reader(ctx);
        if self.deferred || self.loading || self.load_error.is_some() {
            return;
        }
        if !self.drafts.loaded && !self.drafts.config_running {
            self.drafts.config_running = true;
            self.drafts.config_id += 1;
            let id = self.drafts.config_id;
            let path = self.path.clone();
            spawn_or(
                &self.tx,
                ctx,
                move |e| Msg::DraftConfigured(id, Err(e)),
                move || Msg::DraftConfigured(id, store::enabled(&path)),
            );
            return;
        }
        if !self.drafts.enabled || self.busy_op.is_some() {
            return;
        }
        if self.drafts.due(Instant::now()) {
            let id = self.drafts.generation.load(Ordering::Relaxed);
            self.drafts.running = Some(id);
            self.drafts.edited = None;
            let generation = Arc::downgrade(&self.drafts.generation);
            let repo = self.path.clone();
            spawn_or(
                &self.tx,
                ctx,
                move |e| Msg::DraftSaved(id, Err(e)),
                move || {
                    let active = || {
                        generation
                            .upgrade()
                            .is_some_and(|g| g.load(Ordering::Relaxed) == id)
                    };
                    let result = (|| {
                        if !store::enabled_while(&repo, &active)? {
                            return Ok(None);
                        }
                        store::capture(&repo, "Pause", &active)
                    })();
                    Msg::DraftSaved(id, result)
                },
            );
        } else if let Some(at) = self.drafts.edited {
            ctx.request_repaint_after(PAUSE.saturating_sub(at.elapsed()));
        }
    }
    pub(super) fn drafts_configured(&mut self, id: u64, result: Result<bool, String>) {
        if self.drafts.config_id != id {
            return;
        }
        self.drafts.config_running = false;
        self.drafts.loaded = true;
        match result {
            Ok(on) => {
                self.drafts.enabled = on;
                if on {
                    self.drafts.observe();
                }
            }
            Err(e) => {
                self.drafts.enabled = false;
                self.drafts.error = Some(e);
            }
        }
    }
    pub(super) fn draft_saved(&mut self, id: u64, result: Result<Option<store::Draft>, String>) {
        if self.drafts.running != Some(id) {
            return;
        }
        self.drafts.running = None;
        if self.drafts.generation.load(Ordering::Relaxed) != id {
            return;
        }
        self.drafts.error = result.err();
    }
    pub(super) fn earlier_versions(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.draft_reader(ctx, move |active| {
            let config = store::enabled_while(&repo, active);
            let on = config.as_ref().copied().unwrap_or(false);
            let mut error = config.err();
            let versions = match store::list(&repo, active) {
                Ok(v) => v,
                Err(e) => {
                    error = Some(e);
                    Vec::new()
                }
            };
            Ok(Read::History {
                on,
                versions,
                error,
            })
        });
    }
    pub(super) fn preview_draft(
        &mut self,
        id: store::Id,
        path: String,
        index: bool,
        ctx: &egui::Context,
    ) {
        let repo = self.path.clone();
        self.draft_reader(ctx, move |active| {
            store::preview(&repo, &id, &path, index, active).map(|v| Read::Recovery(Box::new(v)))
        });
    }
    fn draft_reader(
        &mut self,
        ctx: &egui::Context,
        read: impl FnOnce(&dyn Fn() -> bool) -> Result<Read, String> + Send + 'static,
    ) {
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.drafts.reader_generation.store(id, Ordering::Release);
        self.dialog = Some(Dialog::DraftLoading(id));
        self.drafts.reader_pending = Some(PendingRead {
            id,
            context: crate::connections::current(),
            read: Box::new(read),
        });
        self.start_draft_reader(ctx);
    }
    /// Close/replace readers immediately, retaining the running slot until its worker
    /// acknowledges cancellation. Only the latest queued request is ever retained.
    pub(super) fn cancel_draft_reads(&mut self) {
        self.drafts.reader_generation.fetch_add(1, Ordering::AcqRel);
        self.drafts.reader_pending = None;
        if matches!(self.dialog, Some(Dialog::DraftLoading(_))) {
            self.dialog = None;
        }
    }
    pub(super) fn dismiss_dialog(&mut self) {
        self.cancel_draft_reads();
        self.dialog = None;
    }
    pub(super) fn sync_draft_reader(&mut self) {
        let id = self.drafts.reader_generation.load(Ordering::Acquire);
        if !matches!(self.dialog, Some(Dialog::DraftLoading(current)) if current == id)
            && (self.drafts.reader_pending.is_some()
                || self
                    .drafts
                    .reader_running
                    .is_some_and(|(running, _)| running == id))
        {
            self.cancel_draft_reads();
        }
    }
    pub(super) fn draft_reader_waiting(&self) -> bool {
        self.drafts.reader_pending.is_some()
            && (self.drafts.reader_running.is_some()
                || self
                    .drafts
                    .reader_not_before
                    .is_some_and(|until| until > Instant::now()))
    }
    fn start_draft_reader(&mut self, ctx: &egui::Context) {
        if self.drafts.reader_running.is_some() {
            return;
        }
        if let Some(until) = self.drafts.reader_not_before {
            let left = until.saturating_duration_since(Instant::now());
            if !left.is_zero() {
                if self.drafts.reader_pending.is_some() {
                    ctx.request_repaint_after(left);
                }
                return;
            }
            self.drafts.reader_not_before = None;
        }
        let Some(PendingRead { id, context, read }) = self.drafts.reader_pending.take() else {
            return;
        };
        if !matches!(self.dialog, Some(Dialog::DraftLoading(current)) if current == id)
            || self.drafts.reader_generation.load(Ordering::Acquire) != id
        {
            return;
        }
        self.drafts.reader_running = Some((id, crate::remote::is_remote(&self.path)));
        let generation = Arc::downgrade(&self.drafts.reader_generation);
        let alive = Arc::downgrade(&self.drafts.alive);
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::DraftRead(id, Err(e)),
            move || {
                context.run(|| {
                    let began = Instant::now();
                    let active = || {
                        alive.upgrade().is_some()
                            && generation
                                .upgrade()
                                .is_some_and(|generation| generation.load(Ordering::Acquire) == id)
                            && began.elapsed() < Duration::from_secs(120)
                    };
                    let result = if active() {
                        read(&active)
                    } else {
                        Err("Draft read cancelled".into())
                    };
                    Msg::DraftRead(id, result.map(Box::new))
                })
            },
        );
    }
    pub(super) fn draft_read(
        &mut self,
        id: u64,
        result: Result<Box<Read>, String>,
        ctx: &egui::Context,
    ) {
        let Some((running, remote)) = self.drafts.reader_running else {
            return;
        };
        if running != id {
            return;
        }
        self.drafts.reader_running = None;
        if !matches!(self.dialog,Some(Dialog::DraftLoading(current))if current==id)
            || self.drafts.reader_generation.load(Ordering::Acquire) != id
        {
            if remote {
                self.drafts.reader_not_before = Some(Instant::now() + REMOTE_READ_DRAIN);
            }
            self.start_draft_reader(ctx);
            return;
        }
        match result {
            Ok(data) => match *data {
                Read::History {
                    on,
                    versions,
                    error,
                } => {
                    let was_on = self.drafts.enabled;
                    self.drafts.config_id += 1;
                    self.drafts.config_running = false;
                    self.drafts.loaded = true;
                    self.drafts.enabled = on;
                    if on && !was_on {
                        self.drafts.observe();
                    }
                    if !on {
                        self.drafts.cancel_pending();
                        self.drafts.error = None;
                    }
                    if on {
                        self.drafts.error = error.clone();
                    }
                    self.dialog = Some(Dialog::DraftHistory {
                        on,
                        versions,
                        error,
                        index: false,
                    });
                }
                Read::Recovery(preview) => {
                    let selected = (0..preview.hunks.len()).collect();
                    self.dialog = Some(Dialog::DraftRecovery { preview, selected });
                }
            },
            Err(e) => {
                self.dialog = None;
                self.status = Some((format!("Earlier versions: {e}"), true));
            }
        }
    }
    pub(super) fn enable_drafts(&mut self, on: bool, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let path = self.path.clone();
        self.drafts.config_id += 1;
        self.draft_change(Change::Setting, ctx, move |_| {
            store::set_enabled(&path, on)?;
            Ok(Changed::Setting(store::enabled(&path)?))
        });
    }
    pub(super) fn snapshot_draft(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        self.draft_change(Change::Snapshot, ctx, move |active| {
            if !store::enabled(&path)? {
                return Err("Enable automatic drafts before saving a version".into());
            }
            store::capture(&path, "Saved manually", active).map(Changed::Snapshot)
        });
    }
    pub(super) fn recover_draft(
        &mut self,
        preview: store::RestorePreview,
        hunks: Vec<usize>,
        ctx: &egui::Context,
    ) {
        let path = self.path.clone();
        self.draft_change(Change::Recovery(preview.path.clone()), ctx, move |active| {
            store::restore(&path, &preview, &hunks, active).map(Changed::Recovery)
        });
    }
    pub(super) fn undo_draft(&mut self, ctx: &egui::Context) {
        if let Some((id, path)) = self.drafts.undo.clone() {
            self.preview_draft(id, path, false, ctx);
        }
    }
    fn draft_change(
        &mut self,
        change: Change,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Fn() -> bool) -> Result<Changed, String> + Send + 'static,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        self.drafts.cancel_pending();
        self.drafts.running = None;
        self.busy_op = Some(
            match change {
                Change::Setting => "Configure working drafts",
                Change::Snapshot => "Save working draft",
                Change::Recovery(_) => "Recover earlier contents",
            }
            .into(),
        );
        self.op_started = Some(Instant::now());
        self.last_op = None;
        let failed = change.clone();
        let alive = Arc::downgrade(&self.drafts.alive);
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::DraftChanged(failed, Err(e)),
            move || {
                let began = Instant::now();
                let active =
                    || alive.upgrade().is_some() && began.elapsed() < Duration::from_secs(120);
                Msg::DraftChanged(change, work(&active))
            },
        );
    }
    pub(super) fn draft_changed(
        &mut self,
        change: Change,
        result: Result<Changed, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        match result {
            Ok(Changed::Setting(on)) => {
                self.drafts.loaded = true;
                self.drafts.config_running = false;
                self.drafts.enabled = on;
                self.drafts.error = None;
                if on {
                    self.drafts.observe();
                }
                self.status = Some((
                    if on {
                        "Working drafts enabled"
                    } else {
                        "Working drafts disabled"
                    }
                    .into(),
                    false,
                ));
            }
            Ok(Changed::Snapshot(saved)) => {
                self.drafts.error = None;
                self.status = Some((
                    if saved.is_some() {
                        "Working draft saved"
                    } else {
                        "No working changes to save"
                    }
                    .into(),
                    false,
                ));
            }
            Ok(Changed::Recovery(id)) => {
                if let Change::Recovery(path) = change {
                    self.drafts.undo = Some((id, path));
                }
                self.drafts.error = None;
                self.status=Some(("Earlier contents recovered; staged contents preserved. Undo recovery is available in Earlier versions.".into(),false));
                self.reload_status(ctx);
            }
            Err(e) => {
                self.drafts.error = Some(e.clone());
                self.status = Some((format!("Working drafts: {e}"), true));
                if matches!(change, Change::Setting) {
                    self.drafts.loaded = false;
                    self.drafts.config_running = false;
                }
                self.reload_status(ctx);
            }
        }
        self.earlier_versions(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reader_tab(ctx: &egui::Context) -> RepoTab {
        let mut tab = RepoTab::new_deferred("/nonexistent/draft-reader-fixture".into(), ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        tab.tx = tx;
        tab.rx = rx;
        tab.drafts.loaded = true;
        tab
    }
    fn history(label: &str) -> Read {
        Read::History {
            on: false,
            versions: Vec::new(),
            error: Some(label.into()),
        }
    }
    // A recording-only reader: no Git, child process, host or signals. It reports
    // the real active callback at controlled points, and stays blocked until released.
    fn delayed_reader(
        tab: &mut RepoTab,
        ctx: &egui::Context,
    ) -> (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<bool>) {
        let (step, steps) = std::sync::mpsc::channel();
        let (state, states) = std::sync::mpsc::channel();
        tab.draft_reader(ctx, move |active| {
            state.send(active()).map_err(|e| e.to_string())?;
            steps
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            state.send(active()).map_err(|e| e.to_string())?;
            steps
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            Ok(history("obsolete"))
        });
        (step, states)
    }
    fn finish_reader(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let msg = tab
                .rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| e.to_string())?;
            if let Msg::DraftRead(id, result) = msg {
                tab.draft_read(id, result, ctx);
                return Ok(());
            }
        }
    }
    #[test]
    fn modal_escape_cancels_the_owned_reader_while_the_tab_remains_alive() -> Result<(), String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = reader_tab(&ctx);
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        let _ = ctx.run_ui(Default::default(), |ui| {
            tab.dialogs(ui.ctx(), &mut Vec::new())
        });
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut Vec::new()));
        assert!(tab.dialog.is_none());
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        assert_eq!(Arc::strong_count(&tab.drafts.alive), 1);
        step.send(()).map_err(|e| e.to_string())?;
        finish_reader(&mut tab, &ctx)?;
        assert!(tab.dialog.is_none());
        assert!(tab.status.is_none());
        Ok(())
    }
    #[test]
    fn replacements_coalesce_until_the_previous_worker_finishes_and_ignore_stale_reads()
    -> Result<(), String> {
        use std::sync::atomic::AtomicUsize;
        let ctx = egui::Context::default();
        let mut tab = reader_tab(&ctx);
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        let old = tab.drafts.reader_running;
        let started = Arc::new(AtomicUsize::new(0));
        for n in 0..100 {
            let started = started.clone();
            tab.draft_reader(&ctx, move |active| {
                assert!(active());
                started.fetch_add(1, Ordering::Relaxed);
                Ok(history(&format!("newest {n}")))
            });
        }
        let latest = tab.drafts.reader_generation.load(Ordering::Acquire);
        assert_eq!(tab.drafts.reader_running, old);
        assert!(tab.draft_reader_waiting());
        assert_eq!(started.load(Ordering::Relaxed), 0);
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        step.send(()).map_err(|e| e.to_string())?;
        finish_reader(&mut tab, &ctx)?;
        assert!(matches!(tab.dialog, Some(Dialog::DraftLoading(id)) if id == latest));
        finish_reader(&mut tab, &ctx)?;
        assert_eq!(started.load(Ordering::Relaxed), 1);
        assert!(
            matches!(tab.dialog, Some(Dialog::DraftHistory { error: Some(ref label), .. }) if label == "newest 99")
        );
        Ok(())
    }
    #[test]
    fn remote_replacements_wait_through_the_existing_detached_command_bound_without_connecting()
    -> Result<(), String> {
        use std::sync::atomic::AtomicUsize;
        let ctx = egui::Context::default();
        let mut tab = reader_tab(&ctx);
        // Only the path classifier sees this host. Injected closures never run transport.
        tab.path = crate::remote::encode(
            &crate::remote::Host::Ssh("recording-only.invalid".into()),
            "/repo",
        );
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        let started = Arc::new(AtomicUsize::new(0));
        let count = started.clone();
        tab.draft_reader(&ctx, move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok(history("remote latest"))
        });
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        step.send(()).map_err(|e| e.to_string())?;
        finish_reader(&mut tab, &ctx)?;
        for _ in 0..100 {
            tab.poll_drafts(&ctx);
        }
        assert_eq!(started.load(Ordering::Relaxed), 0);
        assert!(tab.drafts.reader_running.is_none());
        assert!(tab.draft_reader_waiting());
        assert!(tab.drafts.reader_not_before.is_some_and(|until| {
            until.saturating_duration_since(Instant::now()) > Duration::from_secs(14)
        }));
        // Advance the synthetic deadline instead of sleeping or using a watchdog.
        tab.drafts.reader_not_before = Some(Instant::now());
        tab.poll_drafts(&ctx);
        finish_reader(&mut tab, &ctx)?;
        assert_eq!(started.load(Ordering::Relaxed), 1);
        Ok(())
    }
    #[test]
    fn dialog_replacement_profile_reload_and_tab_drop_cancel_reader_ownership() -> Result<(), String>
    {
        let ctx = egui::Context::default();
        let mut tab = reader_tab(&ctx);
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        // This is the normal Action::Dialog path, not a direct generation mutation.
        tab.apply(super::super::Action::Dialog(Dialog::stash()), &ctx);
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        step.send(()).map_err(|e| e.to_string())?;
        finish_reader(&mut tab, &ctx)?;
        assert!(!matches!(
            tab.dialog,
            Some(Dialog::DraftLoading(_) | Dialog::DraftHistory { .. })
        ));
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        tab.reload_profile_connections(&ctx);
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        step.send(()).map_err(|e| e.to_string())?;
        finish_reader(&mut tab, &ctx)?;
        let (step, states) = delayed_reader(&mut tab, &ctx);
        assert!(
            states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        drop(tab);
        step.send(()).map_err(|e| e.to_string())?;
        assert!(
            !states
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        step.send(()).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn a_due_pause_dispatches_a_background_snapshot_and_disabled_repos_stay_empty()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let path =
            std::env::temp_dir().join(format!("gitcraft-draft-pause-{}", std::process::id()));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        crate::git::run(Some(&path), &["init", "-q"])?;
        std::fs::write(path.join("notes"), b"a pause saves this").map_err(|e| e.to_string())?;
        store::set_enabled(&path, true)?;
        let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
        // A deferred tab also starts its independent filesystem-watcher worker.
        // Isolate this test's draft channel from that unrelated initial result.
        let (tx, rx) = std::sync::mpsc::channel();
        tab.tx = tx;
        tab.rx = rx;
        tab.deferred = false;
        tab.drafts.loaded = true;
        tab.drafts.enabled = true;
        tab.drafts.edited = Some(Instant::now() - PAUSE);
        tab.poll_drafts(&ctx);
        assert!(tab.drafts.running.is_some());
        // The message carries a completed background result; no Git result was read
        // synchronously by the UI dispatcher, which records the running generation.
        let msg = tab
            .rx
            .recv_timeout(Duration::from_secs(20))
            .map_err(|e| e.to_string())?;
        match msg {
            Msg::DraftSaved(id, result) => {
                assert!(result.as_ref().is_ok_and(Option::is_some), "{result:?}");
                tab.draft_saved(id, result);
            }
            _ => return Err("Unexpected worker result".into()),
        }
        assert!(tab.drafts.running.is_none());
        assert_eq!(store::list(&path, &|| true)?.len(), 1);
        store::set_enabled(&path, false)?;
        std::fs::write(path.join("notes"), b"not saved while disabled")
            .map_err(|e| e.to_string())?;
        tab.drafts.edited = Some(Instant::now() - PAUSE);
        tab.poll_drafts(&ctx);
        match tab
            .rx
            .recv_timeout(Duration::from_secs(20))
            .map_err(|e| e.to_string())?
        {
            Msg::DraftSaved(id, result) => {
                assert!(result.as_ref().is_ok_and(Option::is_none));
                tab.draft_saved(id, result);
            }
            _ => return Err("Unexpected worker result".into()),
        }
        assert_eq!(store::list(&path, &|| true)?.len(), 1);
        drop(tab);
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn pauses_coalesce_and_stale_or_disabled_jobs_cannot_publish_ui_results() {
        let mut state = State::default();
        assert!(!state.due(Instant::now()));
        state.enabled = true;
        state.observe();
        assert!(!state.due(Instant::now()));
        state.edited = Some(Instant::now() - PAUSE);
        assert!(state.due(Instant::now()));
        state.running = Some(state.generation.load(Ordering::Relaxed));
        assert!(!state.due(Instant::now()));
        let old = state.generation.load(Ordering::Relaxed);
        state.observe();
        assert_ne!(old, state.generation.load(Ordering::Relaxed));
        state.cancel_pending();
        state.enabled = false;
        assert!(!state.due(Instant::now()));
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/draft-ui-test".into(), &ctx);
        tab.drafts.running = Some(1);
        tab.drafts.generation.store(2, Ordering::Relaxed);
        tab.draft_saved(1, Err("stale".into()));
        assert!(tab.drafts.error.is_none());
        assert!(tab.drafts.running.is_none());
        tab.dialog = None;
        tab.draft_read(1, Err("cancelled".into()), &ctx);
        assert!(tab.status.is_none());
    }
}
