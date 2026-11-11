#[path = "behaviour/view.rs"]
mod view;
use super::{Outcome, RepoTab};
use crate::parallel_bisect::observe::{self, Cache, Key, Mode, Observation, Value};
use crate::parallel_bisect::presentation::{self, Presentation};
use crate::{git, tasks};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Clone, Default)]
enum Frame {
    #[default]
    Unknown,
    Queued,
    Observed(Observation),
    Failed(String),
    FailedWithPresentation(String, Presentation),
}
enum Event {
    Loaded(Vec<git::BehaviourCommit>),
    #[cfg(test)]
    Frame(usize, Result<Observation, String>),
    Done(Result<(), String>),
    FrameWithPresentation(usize, Result<Observation, String>, Option<Presentation>),
    DoneWithPresentation(Result<(), String>, Option<Presentation>),
}
pub struct Form {
    repo: PathBuf,
    scope: tasks::Scope,
    active: Option<tasks::Cancellation>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    cache: Cache,
    revision: String,
    count: usize,
    command: String,
    mode: Mode,
    timeout: u64,
    commits: Vec<git::BehaviourCommit>,
    frames: Vec<Frame>,
    index: usize,
    first_change: Option<usize>,
    configuration: (String, Mode),
    loaded_selection: Option<(String, usize)>,
    error: Option<String>,
    error_presentation: Option<Presentation>,
    texture: Option<egui::TextureHandle>,
}
impl RepoTab {
    pub(in crate::repo) fn open_behaviour(&mut self) {
        let revision = self
            .selected_hash()
            .filter(|s| !s.contains('\0'))
            .unwrap_or_else(|| "HEAD".into());
        self.dialog = Some(super::Dialog::Behaviour(Box::new(Form::new(
            self.path.clone(),
            self._jobs.0.child(),
            self.behaviour_cache.clone(),
            revision,
        ))));
    }
}
impl Form {
    fn new(repo: PathBuf, token: tasks::Cancellation, cache: Cache, revision: String) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            repo,
            scope: tasks::Scope(token),
            active: None,
            tx,
            rx,
            cache,
            revision,
            count: 16,
            command: String::new(),
            mode: Mode::Text,
            timeout: 60,
            commits: Vec::new(),
            frames: Vec::new(),
            index: 0,
            first_change: None,
            configuration: (String::new(), Mode::Text),
            loaded_selection: None,
            error: None,
            error_presentation: None,
            texture: None,
        }
    }
    fn key(&self, tree: &str) -> Key {
        Key {
            repository: self.repo.to_string_lossy().into_owned(),
            tree: tree.into(),
            command: self.command.trim().into(),
            mode: self.mode,
        }
    }
    fn refresh_cache(&mut self) {
        self.frames = self
            .commits
            .iter()
            .map(|c| {
                self.cache
                    .get(&self.key(&c.tree))
                    .map_or(Frame::Unknown, Frame::Observed)
            })
            .collect();
        self.texture = None;
        self.refresh_first_change();
    }
    fn refresh_first_change(&mut self) {
        let observations: Vec<_> = self
            .frames
            .iter()
            .map(|f| {
                if let Frame::Observed(o) = f {
                    Some(o)
                } else {
                    None
                }
            })
            .collect();
        self.first_change = observe::first_difference(&observations);
    }
    fn submit(
        &mut self,
        ctx: &egui::Context,
        mutation: bool,
        work: impl FnOnce(&Sender<Event>) -> Result<(), String> + Send + 'static,
    ) {
        self.submit_with_presentation(ctx, mutation, move |tx, _| work(tx));
    }
    fn submit_with_presentation(
        &mut self,
        ctx: &egui::Context,
        mutation: bool,
        work: impl FnOnce(&Sender<Event>, &mut Option<Presentation>) -> Result<(), String>
        + Send
        + 'static,
    ) {
        self.error_presentation = None;
        let pool = match tasks::pool() {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let token = self.scope.0.child();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let cancelled_tx = tx.clone();
        let cancelled_ctx = ctx.clone();
        let key = mutation.then(|| crate::repo::messages::mutation_key(&self.repo));
        let result = pool.submit_cancellable(
            token.clone(),
            tasks::Priority::Interactive,
            key,
            Some(Box::new(move || {
                let _ =
                    cancelled_tx.send(Event::Done(Err(crate::i18n::text("repo-core-1159").into())));
                cancelled_ctx.request_repaint();
            })),
            Box::new(move || {
                let mut presentation = None;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    work(&tx, &mut presentation)
                }))
                .unwrap_or_else(|_| {
                    presentation = None;
                    Err(crate::i18n::text("repo-core-1160").into())
                });
                presentation::retain_error(&result, &mut presentation);
                let _ = tx.send(Event::DoneWithPresentation(result, presentation));
                ctx.request_repaint();
            }),
        );
        match result {
            Ok(()) => {
                self.active = Some(token);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.into());
                for f in &mut self.frames {
                    if matches!(f, Frame::Queued) {
                        *f = Frame::Unknown;
                    }
                }
            }
        }
    }
    fn painted_error(&self, raw: &str) -> String {
        presentation::error(raw, self.error_presentation.as_ref())
    }
    fn poll(&mut self) {
        let mut changed = false;
        while let Ok(event) = self.rx.try_recv() {
            changed = true;
            let Some(event) = incoming(event, &mut self.frames, &mut self.error_presentation)
            else {
                self.texture = None;
                continue;
            };
            match event {
                Event::FrameWithPresentation(..) | Event::DoneWithPresentation(..) => {
                    unreachable!("presentation events were consumed")
                }
                Event::Loaded(commits) => {
                    self.loaded_selection = Some((self.revision.clone(), self.count));
                    self.commits = commits;
                    self.index = 0;
                    self.refresh_cache();
                }
                #[cfg(test)]
                Event::Frame(i, result) => {
                    if let Some(frame) = self.frames.get_mut(i) {
                        *frame = result.map_or_else(Frame::Failed, Frame::Observed);
                        self.texture = None;
                    }
                }
                Event::Done(result) => {
                    self.active = None;
                    if let Err(e) = result {
                        self.error = Some(e);
                    }
                    for f in &mut self.frames {
                        if matches!(f, Frame::Queued) {
                            *f = Frame::Unknown;
                        }
                    }
                }
            }
        }
        if changed {
            self.refresh_first_change();
        }
    }
}
#[cfg(test)]
#[path = "behaviour/tests.rs"]
mod tests;

// Real poll seam, pure over supplied event/state. Success caches never hold receipts.
fn incoming(
    event: Event,
    frames: &mut [Frame],
    receipt: &mut Option<Presentation>,
) -> Option<Event> {
    match event {
        Event::FrameWithPresentation(i, result, mut presentation) => {
            presentation::retain_error(&result, &mut presentation);
            if let Some(frame) = frames.get_mut(i) {
                *frame = match (result, presentation) {
                    (Ok(observation), _) => Frame::Observed(observation),
                    (Err(raw), Some(p)) => Frame::FailedWithPresentation(raw, p),
                    (Err(raw), None) => Frame::Failed(raw),
                };
            }
            None
        }
        Event::DoneWithPresentation(result, mut presentation) => {
            presentation::retain_error(&result, &mut presentation);
            *receipt = presentation;
            Some(Event::Done(result))
        }
        event => {
            // A legacy/unrelated done cannot reuse a previous operation's receipt.
            if matches!(event, Event::Done(_) | Event::Loaded(_)) {
                *receipt = None;
            }
            Some(event)
        }
    }
}

fn frame_error(raw: &str, receipt: Option<&Presentation>) -> String {
    presentation::error(raw, receipt)
}
#[cfg(test)]
#[path = "behaviour/execution_bridge_tests.rs"]
mod execution_bridge_tests;
