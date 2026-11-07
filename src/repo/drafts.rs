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
                        if !store::enabled(&repo)? {
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
            let config = store::enabled(&repo);
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
        self.dialog = Some(Dialog::DraftLoading(id));
        let alive = Arc::downgrade(&self.drafts.alive);
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::DraftRead(id, Err(e)),
            move || {
                let began = Instant::now();
                let active =
                    || alive.upgrade().is_some() && began.elapsed() < Duration::from_secs(120);
                Msg::DraftRead(id, read(&active).map(Box::new))
            },
        );
    }
    pub(super) fn draft_read(&mut self, id: u64, result: Result<Box<Read>, String>) {
        if !matches!(self.dialog,Some(Dialog::DraftLoading(current))if current==id) {
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
        tab.draft_read(1, Err("cancelled".into()));
        assert!(tab.status.is_none());
    }
}
