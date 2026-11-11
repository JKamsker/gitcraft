//! Opt-in, bounded version-specific package metadata. No guessed changelog URLs.
mod archive;
#[cfg(test)]
mod tests;

use super::deps::{DepChange, Ecosystem, Registry};
use crate::{connections, network, tasks};
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Metadata {
    pub license: Option<String>,
    pub changelogs: BTreeMap<String, String>,
    pub archive_url: String,
    pub license_file: Option<(String, Result<String, String>)>,
    pub license_note: Option<String>,
    pub skipped: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    ecosystem: Ecosystem,
    old: Registry,
    new: Registry,
}
type Side = Result<Option<Metadata>, String>;
type Answer = (Side, Side);
fn read_both(
    eco: Ecosystem,
    old: &Registry,
    new: &Registry,
    mut read: impl FnMut(Ecosystem, &Registry) -> Side,
) -> Answer {
    // A missing/broken predecessor must never prevent reading the updated release.
    (read(eco, old), read(eco, new))
}
struct Pending {
    rx: Receiver<Answer>,
    _scope: tasks::Scope,
    context: connections::Context,
}
#[derive(Default)]
pub(super) struct State {
    selected: Option<(String, Request)>,
    pending: Option<Pending>,
    cache: VecDeque<(Request, Answer)>,
    result: Option<Answer>,
    generation: u64,
}

impl State {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn select(&mut self, eco: Ecosystem, change: &DepChange, ctx: &egui::Context) {
        self.start(
            change.name.clone(),
            Request {
                ecosystem: eco,
                old: change.metadata_registry.0.clone(),
                new: change.metadata_registry.1.clone(),
            },
            ctx,
            false,
        );
    }
    fn start(&mut self, name: String, request: Request, ctx: &egui::Context, retry: bool) {
        self.start_using(name, request, ctx, retry, fetch);
    }
    fn start_using(
        &mut self,
        name: String,
        request: Request,
        ctx: &egui::Context,
        retry: bool,
        mut read: impl FnMut(Ecosystem, &Registry) -> Side + Send + 'static,
    ) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.selected = Some((name, request.clone()));
        if retry {
            self.cache.retain(|(r, _)| r != &request);
        }
        self.result = self
            .cache
            .iter()
            .find(|(r, _)| *r == request)
            .map(|(_, a)| a.clone());
        if request.old.is_none() && request.new.is_none() {
            let error = crate::i18n::text("repo-core-4002").to_owned();
            self.result = Some((Err(error.clone()), Err(error)));
            return;
        }
        if self.result.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let scope = tasks::Scope::default();
        let repaint = ctx.clone();
        let work = request.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit(
                scope.0.clone(),
                tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let answer = read_both(work.ecosystem, &work.old, &work.new, &mut read);
                    let _ = tx.send(answer);
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submitted {
            Ok(()) => {
                self.pending = Some(Pending {
                    rx,
                    _scope: scope,
                    context: connections::current(),
                })
            }
            Err(e) => self.result = Some((Err(e.clone()), Err(e))),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.context != connections::current())
        {
            self.pending = None;
            self.result = Some((
                Err(crate::i18n::text("repo-core-4003").into()),
                Err(crate::i18n::text("repo-core-4003").into()),
            ));
            self.generation = self.generation.wrapping_add(1);
        }
        if let Some(pending) = &self.pending {
            match pending.rx.try_recv() {
                Ok(answer) => {
                    self.generation = self.generation.wrapping_add(1);
                    self.pending = None;
                    // Offline, cancelled and transient responses remain retryable. Partial
                    // successes stay visible but never poison the complete-result cache.
                    if answer.0.is_ok()
                        && answer.1.is_ok()
                        && let Some((_, request)) = &self.selected
                    {
                        self.cache.retain(|(r, _)| r != request);
                        self.cache.push_back((request.clone(), answer.clone()));
                        if self.cache.len() > 8 {
                            self.cache.pop_front();
                        }
                    }
                    self.result = Some(answer);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.generation = self.generation.wrapping_add(1);
                    self.result = Some((
                        Err(crate::i18n::text("repo-core-4004").into()),
                        Err(crate::i18n::text("repo-core-4004").into()),
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
    }
    fn close(&mut self) {
        self.selected = None;
        self.pending = None;
        self.result = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn append_document(
        &self,
        doc: &mut crate::repo::find::rendered::Document,
        t: &crate::theme::Tokens,
    ) {
        let Some((name, _)) = &self.selected else {
            return;
        };
        doc.text(
            &crate::i18n::message("repo-core-4005", &[("name", name.to_string().into())]),
            t.text,
        );
        if self.pending.is_some() {
            doc.text(crate::i18n::text("repo-core-4006"), t.text_dim);
        }
        if let Some(answer) = &self.result {
            doc.text(
                &crate::i18n::message(
                    "repo-core-4007",
                    &[
                        (
                            "else",
                            (if changed(answer) {
                                crate::i18n::text("repo-core-4008")
                            } else {
                                ""
                            })
                            .to_string()
                            .into(),
                        ),
                        ("answer", side_summary(&answer.0).to_string().into()),
                        ("answer2", side_summary(&answer.1).to_string().into()),
                    ],
                ),
                t.warning,
            );
            for (side, result) in [
                (crate::i18n::text("repo-core-1564"), &answer.0),
                (crate::i18n::text("repo-core-1565"), &answer.1),
            ] {
                match result {
                    Ok(Some(m)) => m.append_document(doc, side, t),
                    Ok(None) => doc.text(
                        &crate::i18n::message(
                            "repo-core-4009",
                            &[("side", side.to_string().into())],
                        ),
                        t.text_dim,
                    ),
                    Err(error) => doc.text(
                        &crate::i18n::message(
                            "repo-core-4010",
                            &[
                                ("side", side.to_string().into()),
                                ("error", error.to_string().into()),
                            ],
                        ),
                        t.warning,
                    ),
                }
            }
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        let Some((name, request)) = self.selected.clone() else {
            return;
        };
        let mut retry = false;
        ui.horizontal(|ui| {
            ui.strong(crate::i18n::message(
                "repo-core-4005",
                &[("name", name.to_string().into())],
            ));
            if ui.button(crate::i18n::text("repo-core-4011")).clicked() {
                self.close();
            }
            if self.pending.is_none()
                && self
                    .result
                    .as_ref()
                    .is_some_and(|(a, b)| a.is_err() || b.is_err())
                && ui.button(crate::i18n::text("repo-core-4012")).clicked()
            {
                retry = true;
            }
        });
        if retry {
            self.start(name, request, ui.ctx(), true);
        }
        if self.pending.is_some() {
            ui.label(crate::i18n::text("repo-core-4013"));
        }
        if let Some(answer) = &self.result {
            ui.label(crate::i18n::message(
                "repo-core-4007",
                &[
                    (
                        "else",
                        (if changed(answer) {
                            crate::i18n::text("repo-core-4008")
                        } else {
                            ""
                        })
                        .to_string()
                        .into(),
                    ),
                    ("answer", side_summary(&answer.0).to_string().into()),
                    ("answer2", side_summary(&answer.1).to_string().into()),
                ],
            ));
            egui::ScrollArea::vertical()
                .id_salt("dependency_metadata")
                .max_height(240.0)
                .show(ui, |ui| {
                    for (side, result) in [
                        (crate::i18n::text("repo-core-1564"), &answer.0),
                        (crate::i18n::text("repo-core-1565"), &answer.1),
                    ] {
                        let metadata = match result {
                            Ok(Some(m)) => m,
                            Ok(None) => {
                                ui.label(crate::i18n::message(
                                    "repo-core-4009",
                                    &[("side", side.to_string().into())],
                                ));
                                continue;
                            }
                            Err(error) => {
                                ui.label(crate::i18n::message(
                                    "repo-core-4010",
                                    &[
                                        ("side", side.to_string().into()),
                                        ("error", error.to_string().into()),
                                    ],
                                ));
                                continue;
                            }
                        };
                        ui.hyperlink_to(
                            crate::i18n::message(
                                "repo-core-4015",
                                &[("side", side.to_string().into())],
                            ),
                            &metadata.archive_url,
                        );
                        if let Some((path, content)) = &metadata.license_file {
                            egui::CollapsingHeader::new(crate::i18n::message(
                                "repo-core-3996",
                                &[
                                    ("side", side.to_string().into()),
                                    ("path", path.to_string().into()),
                                ],
                            ))
                            .id_salt((side, "licence-file"))
                            .show(ui, |ui| match content {
                                Ok(text) => {
                                    ui.label(text);
                                }
                                Err(error) => {
                                    ui.label(error);
                                }
                            });
                        }
                        if metadata.changelogs.is_empty() {
                            ui.label(crate::i18n::message(
                                "repo-core-4017",
                                &[("side", side.to_string().into())],
                            ));
                        }
                        for (path, reason) in &metadata.skipped {
                            ui.label(crate::i18n::message(
                                "repo-core-3999",
                                &[
                                    ("side", side.to_string().into()),
                                    ("path", path.to_string().into()),
                                    ("reason", reason.to_string().into()),
                                ],
                            ));
                        }
                        for (path, text) in &metadata.changelogs {
                            egui::CollapsingHeader::new(crate::i18n::message(
                                "repo-core-3998",
                                &[
                                    ("side", side.to_string().into()),
                                    ("path", path.to_string().into()),
                                ],
                            ))
                            .id_salt((side, path))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(text).monospace()).wrap(),
                                );
                            });
                        }
                    }
                });
        }
        ui.separator();
    }
}

mod registry;
use registry::fetch;
#[cfg(test)]
use registry::{get, parse};

#[allow(unused_imports)] // Compatibility re-export.
pub(super) use registry::urls;

mod document;
use document::{changed, side_summary};
