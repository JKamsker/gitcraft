//! Behaviour timeline controls and observation presentation.

use super::*;

impl Form {
    pub(in crate::repo::dialogs) fn show(&mut self, ui: &mut egui::Ui) -> Outcome {
        let height = (ui.ctx().content_rect().height() - 100.0).max(180.0);
        egui::ScrollArea::vertical()
            .id_salt("behaviour_dialog")
            .max_height(height)
            .show(ui, |ui| self.show_contents(ui))
            .inner
    }
    pub(super) fn show_contents(&mut self, ui: &mut egui::Ui) -> Outcome {
        self.poll();
        ui.set_width(850.0);
        ui.heading(crate::i18n::text("repo-core-1162"));
        ui.label(crate::i18n::message(
            "repo-core-1163",
            &[(
                "label",
                crate::teleport::Machine::of(&self.repo)
                    .label()
                    .to_string()
                    .into(),
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
                if matches!(
                    self.frames[i],
                    Frame::Unknown | Frame::Failed(_) | Frame::FailedWithPresentation(..)
                ) {
                    self.frames[i] = Frame::Queued;
                    work.push((i, self.commits[i].clone(), self.key(&self.commits[i].tree)));
                }
            }
            let cache = self.cache.clone();
            let repaint = ui.ctx().clone();
            self.submit_with_presentation(ui.ctx(), true, move |tx, done_presentation| {
                for (i, commit, key) in work {
                    if tasks::cancellation().is_cancelled() {
                        break;
                    }
                    let mut presentation = None;
                    let result = match cache.get(&key) {
                        Some(observation) => Ok(observation),
                        None => observe::run_with_presentation(
                            &repo,
                            &commit.hash,
                            &command,
                            mode,
                            timeout,
                            &mut presentation,
                        ),
                    };
                    if let Ok(observation) = &result {
                        cache.put(key, observation.clone());
                    }
                    // Publish concrete worker/ownership errors before checking cancellation.
                    let error = result.as_ref().err().cloned();
                    let _ = tx.send(Event::FrameWithPresentation(
                        i,
                        result,
                        presentation.clone(),
                    ));
                    repaint.request_repaint();
                    if let Some(error) = error {
                        *done_presentation = presentation;
                        return Err(error);
                    }
                }
                Ok(())
            });
        }
        ui.horizontal(|ui| {
            if let Some(token) = &self.active {
                #[expect(clippy::disallowed_methods, reason = "Keep the existing pending-indicator repaint behavior in this lint-only unit; migrating to the throttled crate spinner requires a separate behavior change")]
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
            ui.colored_label(
                crate::theme::Tokens::get(ui.ctx()).danger,
                self.painted_error(e),
            );
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
                        &[("hash", self.commits[i].hash[..8].to_string().into())],
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
                            Frame::Failed(_) | Frame::FailedWithPresentation(..) => {
                                crate::i18n::text("repo-core-1183")
                            }
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
                Frame::FailedWithPresentation(e, receipt) => {
                    ui.label(frame_error(e, Some(receipt)));
                }
                Frame::Failed(e) => {
                    ui.label(frame_error(e, None));
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
    pub(super) fn benchmark_points(&mut self, ui: &mut egui::Ui) {
        let values: Vec<_> = self
            .frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                if let Frame::Observed(Observation::Stable(sample)) = f
                    && let Value::Number(n) = sample.value
                {
                    return Some((i, n));
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
