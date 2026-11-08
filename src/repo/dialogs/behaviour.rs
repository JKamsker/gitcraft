use super::{Outcome, RepoTab};
use crate::parallel_bisect::observe::{self, Cache, Key, Mode, Observation, Value};
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
}
enum Event {
    Loaded(Vec<git::BehaviourCommit>),
    Frame(usize, Result<Observation, String>),
    Done(Result<(), String>),
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
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&tx)))
                    .unwrap_or_else(|_| Err(crate::i18n::text("repo-core-1160").into()));
                let _ = tx.send(Event::Done(result));
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
    fn poll(&mut self) {
        let mut changed = false;
        while let Ok(event) = self.rx.try_recv() {
            changed = true;
            match event {
                Event::Loaded(commits) => {
                    self.loaded_selection = Some((self.revision.clone(), self.count));
                    self.commits = commits;
                    self.index = 0;
                    self.refresh_cache();
                }
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
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Outcome {
        let height = (ui.ctx().content_rect().height() - 100.0).max(180.0);
        egui::ScrollArea::vertical()
            .id_salt("behaviour_dialog")
            .max_height(height)
            .show(ui, |ui| self.show_contents(ui))
            .inner
    }
    fn show_contents(&mut self, ui: &mut egui::Ui) -> Outcome {
        self.poll();
        ui.set_width(850.0);
        ui.heading(crate::i18n::text("repo-core-1162"));
        ui.label(crate::i18n::message(
            "repo-core-1163",
            &[(
                "label",
                format!("{}", crate::teleport::Machine::of(&self.repo).label()).into(),
            )],
        ));
        let idle = self.active.is_none();
        ui.add_enabled_ui(idle, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text("repo-core-1164"));
                ui.text_edit_singleline(&mut self.revision);
                ui.label(crate::i18n::text("repo-core-1165"));
                ui.add(egui::DragValue::new(&mut self.count).range(1..=32));
            });
            ui.label(crate::i18n::text("repo-core-1166"));
            if ui.button(crate::i18n::text("repo-core-1167")).clicked() {
                let repo = self.repo.clone();
                let revision = self.revision.clone();
                let count = self.count;
                self.submit(ui.ctx(), false, move |tx| {
                    let commits = git::behaviour_commits(&repo, &revision, count)?;
                    let _ = tx.send(Event::Loaded(commits));
                    Ok(())
                });
            }
            ui.label(crate::i18n::text("repo-core-1168"));
            ui.text_edit_multiline(&mut self.command);
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.mode,
                    Mode::Text,
                    crate::i18n::text("repo-core-1169"),
                );
                ui.selectable_value(
                    &mut self.mode,
                    Mode::Number,
                    crate::i18n::text("repo-core-1170"),
                );
                ui.selectable_value(
                    &mut self.mode,
                    Mode::Png,
                    crate::i18n::text("repo-core-1171"),
                );
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text("repo-core-1172"));
                ui.add(egui::DragValue::new(&mut self.timeout).range(1..=600));
            });
        });
        if self
            .loaded_selection
            .as_ref()
            .is_some_and(|s| s != &(self.revision.clone(), self.count))
        {
            self.commits.clear();
            self.frames.clear();
            self.texture = None;
            self.loaded_selection = None;
            self.first_change = None;
        }
        if self.configuration != (self.command.clone(), self.mode) {
            self.configuration = (self.command.clone(), self.mode);
            self.refresh_cache();
        }
        ui.label(crate::i18n::text("repo-core-1173"));
        if ui
            .add_enabled(
                idle && !self.commits.is_empty()
                    && !self.command.trim().is_empty()
                    && self.command.len() <= 8192,
                egui::Button::new(crate::i18n::text("repo-core-1174")),
            )
            .clicked()
        {
            let repo = self.repo.clone();
            let command = self.command.trim().to_owned();
            let mode = self.mode;
            let timeout = Duration::from_secs(self.timeout);
            let mut work = Vec::new();
            for i in 0..self.commits.len() {
                if matches!(self.frames[i], Frame::Unknown | Frame::Failed(_)) {
                    self.frames[i] = Frame::Queued;
                    work.push((i, self.commits[i].clone(), self.key(&self.commits[i].tree)));
                }
            }
            let cache = self.cache.clone();
            let repaint = ui.ctx().clone();
            self.submit(ui.ctx(), true, move |tx| {
                for (i, commit, key) in work {
                    if tasks::cancellation().is_cancelled() {
                        break;
                    }
                    let result = match cache.get(&key) {
                        Some(observation) => Ok(observation),
                        None => observe::run(&repo, &commit.hash, &command, mode, timeout),
                    };
                    if let Ok(observation) = &result {
                        cache.put(key, observation.clone());
                    }
                    // Publish concrete worker/ownership errors before checking cancellation.
                    let error = result.as_ref().err().cloned();
                    let _ = tx.send(Event::Frame(i, result));
                    repaint.request_repaint();
                    if let Some(error) = error {
                        return Err(error);
                    }
                }
                Ok(())
            });
        }
        ui.horizontal(|ui| {
            if let Some(token) = &self.active {
                ui.spinner();
                if ui.button(crate::i18n::text("repo-core-1175")).clicked() {
                    token.cancel();
                }
            }
            if ui
                .add_enabled(idle, egui::Button::new(crate::i18n::text("repo-core-1176")))
                .clicked()
            {
                self.cache.clear();
                self.refresh_cache();
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
        }
        if !self.commits.is_empty() {
            if self.mode == Mode::Number {
                self.benchmark_points(ui);
            }
            if ui
                .add(
                    egui::Slider::new(&mut self.index, 0..=self.commits.len() - 1)
                        .text(crate::i18n::text("repo-core-1177")),
                )
                .changed()
            {
                self.texture = None;
            }
            let commit = &self.commits[self.index];
            ui.label(format!("{} · {}", commit.hash, commit.summary));
            if let Some(i) = self.first_change {
                if ui
                    .button(crate::i18n::message(
                        "repo-core-1178",
                        &[("hash", format!("{}", &self.commits[i].hash[..8]).into())],
                    ))
                    .clicked()
                {
                    return Outcome::Done(Box::new(crate::repo::Action::SelectHash(
                        self.commits[i].hash.clone(),
                    )));
                }
            } else {
                ui.label(crate::i18n::text("repo-core-1179"));
            }
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .id_salt("behaviour_frames")
                .show(ui, |ui| {
                    for (i, (commit, frame)) in self.commits.iter().zip(&self.frames).enumerate() {
                        let status = match frame {
                            Frame::Unknown => crate::i18n::text("repo-core-1181"),
                            Frame::Queued => crate::i18n::text("repo-core-1182"),
                            Frame::Failed(_) => crate::i18n::text("repo-core-1183"),
                            Frame::Observed(Observation::Inconsistent) => {
                                crate::i18n::text("repo-core-1184")
                            }
                            Frame::Observed(Observation::Stable(_)) => {
                                crate::i18n::text("repo-core-1185")
                            }
                        };
                        if ui
                            .selectable_label(
                                i == self.index,
                                format!("{} · {status} · {}", &commit.hash[..8], commit.summary),
                            )
                            .clicked()
                        {
                            self.index = i;
                            self.texture = None;
                        }
                    }
                });
            match &self.frames[self.index] {
                Frame::Unknown => {
                    ui.label(crate::i18n::text("repo-core-1187"));
                }
                Frame::Queued => {
                    ui.label(crate::i18n::text("repo-core-1188"));
                }
                Frame::Failed(e) => {
                    ui.label(e);
                }
                Frame::Observed(Observation::Inconsistent) => {
                    ui.label(crate::i18n::text("repo-core-1189"));
                }
                Frame::Observed(Observation::Stable(sample)) => {
                    ui.label(crate::i18n::message(
                        "repo-core-1190",
                        &[("code", format!("{}", sample.code).into())],
                    ));
                    if !sample.stderr.is_empty() {
                        ui.collapsing(crate::i18n::text("repo-core-1191"), |ui| {
                            let preview: String = sample
                                .stderr
                                .iter()
                                .take(8192)
                                .flat_map(|b| std::ascii::escape_default(*b))
                                .map(char::from)
                                .collect();
                            ui.monospace(preview);
                            if sample.stderr.len() > 8192 {
                                ui.label(crate::i18n::text("repo-core-1192"));
                            }
                        });
                    }
                    match &sample.value {
                        Value::Text(text) => {
                            egui::ScrollArea::both()
                                .max_height(260.0)
                                .id_salt("behaviour_output")
                                .show(ui, |ui| {
                                    ui.monospace(text);
                                });
                        }
                        Value::Number(number) => {
                            ui.heading(format!("{number}"));
                            ui.label(crate::i18n::text("repo-core-1195"));
                        }
                        Value::Png {
                            width,
                            height,
                            rgba,
                        } => {
                            let texture = self.texture.get_or_insert_with(|| {
                                ui.ctx().load_texture(
                                    "behaviour-preview",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [*width as usize, *height as usize],
                                        rgba,
                                    ),
                                    Default::default(),
                                )
                            });
                            ui.add(
                                egui::Image::new((
                                    texture.id(),
                                    egui::vec2(*width as f32, *height as f32),
                                ))
                                .max_size(egui::vec2(780.0, 320.0)),
                            );
                        }
                    }
                }
            }
        }
        ui.label(crate::i18n::text("repo-core-1197"));
        if ui.button(crate::i18n::text("repo-core-0595")).clicked() {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
    fn benchmark_points(&mut self, ui: &mut egui::Ui) {
        let values: Vec<_> = self
            .frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                if let Frame::Observed(Observation::Stable(sample)) = f {
                    if let Value::Number(n) = sample.value {
                        return Some((i, n));
                    }
                }
                None
            })
            .collect();
        if values.is_empty() {
            return;
        }
        ui.label(crate::i18n::text("repo-core-1198"));
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 100.0),
            egui::Sense::click(),
        );
        let scale = values.iter().map(|(_, n)| n.abs()).fold(1.0_f64, f64::max);
        let low = values
            .iter()
            .map(|(_, n)| n / scale)
            .fold(f64::INFINITY, f64::min);
        let high = values
            .iter()
            .map(|(_, n)| n / scale)
            .fold(f64::NEG_INFINITY, f64::max);
        let range = if high > low { high - low } else { 1.0 };
        let inner = rect.shrink(8.0);
        for (i, n) in values {
            let x = inner.left() + inner.width() * i as f32 / (self.frames.len() - 1).max(1) as f32;
            let y = inner.bottom() - inner.height() * ((n / scale - low) / range) as f32;
            let point = egui::pos2(x, y);
            ui.painter().circle_filled(
                point,
                if i == self.index { 5.0 } else { 3.0 },
                ui.visuals().selection.stroke.color,
            );
            if response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| p.distance(point) <= 10.0)
            {
                self.index = i;
                self.texture = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn form() -> Form {
        let mut form = Form::new(
            "/synthetic/repo".into(),
            Default::default(),
            Default::default(),
            "HEAD".into(),
        );
        let commits = (0..4)
            .map(|i| git::BehaviourCommit {
                hash: format!("{i:040x}"),
                tree: format!("{:040x}", i + 1),
                summary: format!("frame {i}"),
            })
            .collect();
        form.tx.send(Event::Loaded(commits)).unwrap();
        form.poll();
        form
    }
    #[test]
    fn recording_timeline_dialog_scrubs_injected_stable_unknown_and_inconsistent_frames() {
        let mut form = form();
        assert!(form.frames.iter().all(|f| matches!(f, Frame::Unknown)));
        form.cache.put(
            form.key(&form.commits[0].tree),
            Observation::Stable(Value::Text("frame output".into()).into()),
        );
        form.cache.put(
            form.key(&form.commits[1].tree),
            Observation::Stable(Value::Text("changed".into()).into()),
        );
        form.cache
            .put(form.key(&form.commits[2].tree), Observation::Inconsistent);
        form.refresh_cache();
        assert_eq!(form.first_change, Some(1));
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 2400.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(matches!(form.show(ui), Outcome::Open));
            },
        );
        output.textures_delta.clear();
        let text: String = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("frame output"));
        assert!(text.contains("inconsistent repeats"));
        assert!(text.contains("not run / evicted"));
        assert!(text.contains("Open first observed adjacent change"));
        assert!(form.active.is_none());
    }
    #[test]
    fn recording_late_worker_error_survives_cancelled_token_and_completion() {
        let mut form = form();
        let token = form.scope.0.child();
        token.cancel();
        form.active = Some(token);
        form.frames[0] = Frame::Queued;
        form.tx
            .send(Event::Frame(0, Err("late concrete owner error".into())))
            .unwrap();
        form.tx
            .send(Event::Done(Err("workspace cleanup unknown".into())))
            .unwrap();
        form.poll();
        assert!(matches!(&form.frames[0], Frame::Failed(e) if e == "late concrete owner error"));
        assert_eq!(form.error.as_deref(), Some("workspace cleanup unknown"));
        assert!(form.active.is_none());
    }
}
