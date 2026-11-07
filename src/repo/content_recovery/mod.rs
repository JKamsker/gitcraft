//! One owned reader, newest pending request, and a separate deliberate mutation.
#[cfg(test)]
mod tests;
mod view;
use super::{Dialog, Msg, RepoTab, messages::spawn_or};
use crate::git::recovery as engine;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
#[cfg(test)]
type Injection = Box<dyn FnOnce(&dyn Fn() -> bool) -> Result<Read, String> + Send>;
static REQUESTS: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
pub(super) struct State {
    pub query: String,
    pub results: Option<Arc<engine::Results>>,
    pub preview: Option<Arc<engine::Preview>>,
    pub prepared: Option<Arc<engine::Prepared>>,
    pub selected: Option<usize>,
    pub file: String,
    pub branch: String,
    pub error: Option<String>,
    generation: Arc<AtomicU64>,
    alive: Arc<()>,
    running: Option<u64>,
    pending: Option<Pending>,
    began: Option<Instant>,
}
enum Job {
    Search(String),
    Inspect(Arc<engine::Results>, usize, Option<usize>),
    Prepare(Arc<engine::Preview>, engine::Target),
    #[cfg(test)]
    Inject(Injection),
}
struct Pending {
    id: u64,
    context: crate::connections::Context,
    job: Job,
}
pub(super) enum Read {
    Results(Arc<engine::Results>),
    Preview(Arc<engine::Preview>),
    Prepared(Arc<engine::Prepared>),
}
impl State {
    pub(super) fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.pending = None;
        self.began = None;
    }
    pub(super) fn dismiss(&mut self) {
        self.cancel();
        self.results = None;
        self.preview = None;
        self.prepared = None;
        self.selected = None;
    }
    pub(super) fn loading(&self) -> bool {
        self.began.is_some()
    }
    fn enqueue(&mut self, job: Job) {
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.generation.store(id, Ordering::Release);
        self.pending = Some(Pending {
            id,
            context: crate::connections::current(),
            job,
        });
        self.began = Some(Instant::now());
        self.error = None;
        self.prepared = None;
    }
}
impl RepoTab {
    pub(super) fn open_content_recovery(&mut self) {
        self.ssh_setup = None;
        self.dismiss_dialog();
        self.discard_decrypted();
        self.dialog = Some(Dialog::ContentRecovery);
    }
    pub(super) fn content_search(&mut self, query: String, ctx: &egui::Context) {
        self.content_recovery.query = query.clone();
        self.content_recovery.results = None;
        self.content_recovery.preview = None;
        self.content_recovery.selected = None;
        self.content_recovery.enqueue(Job::Search(query));
        self.dialog = Some(Dialog::ContentRecovery);
        self.poll_content_recovery(ctx);
    }
    pub(super) fn content_inspect(
        &mut self,
        hit: usize,
        source: Option<usize>,
        ctx: &egui::Context,
    ) {
        let Some(results) = self.content_recovery.results.clone() else {
            return;
        };
        self.content_recovery.selected = Some(hit);
        self.content_recovery
            .enqueue(Job::Inspect(results, hit, source));
        self.dialog = Some(Dialog::ContentRecovery);
        self.poll_content_recovery(ctx);
    }
    pub(super) fn content_prepare(&mut self, target: engine::Target, ctx: &egui::Context) {
        let Some(preview) = self.content_recovery.preview.clone() else {
            return;
        };
        self.content_recovery.enqueue(Job::Prepare(preview, target));
        self.dialog = Some(Dialog::ContentRecovery);
        self.poll_content_recovery(ctx);
    }
    pub(super) fn poll_content_recovery(&mut self, ctx: &egui::Context) {
        if !matches!(self.dialog, Some(Dialog::ContentRecovery)) {
            self.content_recovery.dismiss();
            return;
        }
        if self.content_recovery.running.is_some() {
            return;
        }
        let Some(Pending { id, context, job }) = self.content_recovery.pending.take() else {
            return;
        };
        self.content_recovery.running = Some(id);
        let generation = Arc::downgrade(&self.content_recovery.generation);
        let repo = self.path.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |e| Msg::ContentRead(id, Err(e)),
            move || {
                context.run(|| {
                    let began = Instant::now();
                    let active = || {
                        generation
                            .upgrade()
                            .is_some_and(|g| g.load(Ordering::Acquire) == id)
                            && began.elapsed() < Duration::from_secs(120)
                    };
                    let result = (|| {
                        if !active() {
                            return Err("Content recovery cancelled".into());
                        }
                        match job {
                            Job::Search(query) => engine::search(&repo, &query, &active)
                                .map(|results| Read::Results(Arc::new(results))),
                            Job::Inspect(results, index, source) => {
                                let hit = results
                                    .report
                                    .hits
                                    .get(index)
                                    .ok_or("Invalid content selection")?;
                                engine::inspect(&repo, &results, hit, source, &active)
                                    .map(|preview| Read::Preview(Arc::new(preview)))
                            }
                            Job::Prepare(preview, target) => {
                                engine::prepare(&repo, &preview, target, &active)
                                    .map(|prepared| Read::Prepared(Arc::new(prepared)))
                            }
                            #[cfg(test)]
                            Job::Inject(read) => read(&active),
                        }
                    })();
                    Msg::ContentRead(id, result.map(Box::new))
                })
            },
        );
    }
    pub(super) fn content_read(
        &mut self,
        id: u64,
        result: Result<Box<Read>, String>,
        ctx: &egui::Context,
    ) {
        if self.content_recovery.running != Some(id) {
            return;
        }
        self.content_recovery.running = None;
        if !matches!(self.dialog, Some(Dialog::ContentRecovery))
            || self.content_recovery.generation.load(Ordering::Acquire) != id
        {
            self.poll_content_recovery(ctx);
            return;
        }
        self.content_recovery.began = None;
        match result {
            Ok(read) => match *read {
                Read::Results(results) => self.content_recovery.results = Some(results),
                Read::Preview(preview) => {
                    self.content_recovery.file = format!("recovered-{}.txt", &preview.blob[..12]);
                    self.content_recovery.branch = format!("recovered/{}", &preview.blob[..12]);
                    self.content_recovery.preview = Some(preview);
                }
                Read::Prepared(prepared) => self.content_recovery.prepared = Some(prepared),
            },
            Err(error) => self.content_recovery.error = Some(error),
        }
    }
    pub(super) fn content_perform(&mut self, prepared: Arc<engine::Prepared>, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        self.content_recovery.cancel();
        self.content_recovery.prepared = None;
        self.dialog = None;
        self.busy_op = Some("Recover content".into());
        self.op_started = Some(Instant::now());
        self.last_op = None;
        let alive = Arc::downgrade(&self.content_recovery.alive);
        let path = self.path.clone();
        spawn_or(
            &self.tx,
            ctx,
            |error| Msg::ContentRecovered(Err(error)),
            move || {
                let began = Instant::now();
                Msg::ContentRecovered(engine::perform(&path, &prepared, &|| {
                    alive.upgrade().is_some() && began.elapsed() < Duration::from_secs(120)
                }))
            },
        );
    }
    pub(super) fn content_recovered(
        &mut self,
        result: Result<String, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        self.status = Some(match result {
            Ok(message) => (message, false),
            Err(error) => (format!("Content recovery: {error}"), true),
        });
        self.content_recovery.preview = None;
        self.reload(ctx);
    }
}
