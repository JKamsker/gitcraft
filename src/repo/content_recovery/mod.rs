//! Remembered-content search, exact provenance preview and create-only recovery.
mod view;
use super::saved_views::reader::Reader;
use super::{Dialog, RepoTab};
use crate::git::recovery as engine;
use std::sync::Arc;
use std::time::Instant;
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
    began: Option<Instant>,
    reader: Reader<Read>,
}
enum Read {
    Results(Arc<engine::Results>),
    Preview(Arc<engine::Preview>),
    Prepared(Arc<engine::Prepared>),
}
impl State {
    fn cancel(&mut self) {
        self.reader.cancel();
        self.began = None;
    }
    fn loading(&self) -> bool {
        self.began.is_some()
    }
    fn queue(
        &mut self,
        work: impl FnOnce(&dyn Fn() -> bool) -> Result<Read, String> + Send + 'static,
    ) {
        self.reader.queue(work);
        self.began = Some(Instant::now());
        self.error = None;
        self.prepared = None;
    }
}
impl RepoTab {
    pub(super) fn content_search(&mut self, query: String, ctx: &egui::Context) {
        self.content_recovery.query = query.clone();
        self.content_recovery.results = None;
        self.content_recovery.preview = None;
        self.content_recovery.selected = None;
        let repo = self.path.clone();
        self.content_recovery.queue(move |active| {
            engine::search(&repo, &query, active).map(|r| Read::Results(Arc::new(r)))
        });
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
        let repo = self.path.clone();
        self.content_recovery.selected = Some(hit);
        self.content_recovery.queue(move |active| {
            let hit = results
                .report
                .hits
                .get(hit)
                .ok_or("Invalid content selection")?;
            engine::inspect(&repo, &results, hit, source, active)
                .map(|p| Read::Preview(Arc::new(p)))
        });
        self.dialog = Some(Dialog::ContentRecovery);
        self.poll_content_recovery(ctx);
    }
    pub(super) fn content_prepare(&mut self, target: engine::Target, ctx: &egui::Context) {
        let Some(preview) = self.content_recovery.preview.clone() else {
            return;
        };
        let repo = self.path.clone();
        self.content_recovery.queue(move |active| {
            engine::prepare(&repo, &preview, target, active).map(|p| Read::Prepared(Arc::new(p)))
        });
        self.poll_content_recovery(ctx);
    }
    pub(super) fn poll_content_recovery(&mut self, ctx: &egui::Context) {
        self.content_recovery.reader.bind(self.tx.cancel.clone());
        if !matches!(self.dialog, Some(Dialog::ContentRecovery)) {
            self.content_recovery.cancel();
            let _ = self.content_recovery.reader.poll(ctx);
            return;
        }
        if let Some(result) = self.content_recovery.reader.poll(ctx) {
            self.content_recovery.began = None;
            match result {
                Ok(Read::Results(r)) => self.content_recovery.results = Some(r),
                Ok(Read::Prepared(p)) => self.content_recovery.prepared = Some(p),
                Ok(Read::Preview(p)) => {
                    self.content_recovery.file = format!("recovered-{}.txt", &p.blob[..12]);
                    self.content_recovery.branch = format!("recovered/{}", &p.blob[..12]);
                    self.content_recovery.preview = Some(p);
                }
                Err(e) => self.content_recovery.error = Some(e),
            }
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
        let repo = self.path.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let cancel = crate::tasks::cancellation();
                engine::perform(&repo, &prepared, &|| !cancel.is_cancelled())
            },
            |tab, result, ctx| {
                tab.busy_op = None;
                tab.op_started = None;
                tab.status = Some(match result {
                    Ok(s) => (s, false),
                    Err(e) => (format!("Content recovery: {e}"), true),
                });
                tab.content_recovery.preview = None;
                tab.reload(ctx);
            },
        );
    }
}
