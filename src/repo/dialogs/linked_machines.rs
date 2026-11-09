use super::{Outcome, RepoTab};
use crate::{git, tasks};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

type Sample = (PathBuf, Result<git::MachineFacts, String>);
enum Event {
    Links(Result<Vec<PathBuf>, String>),
    Facts(Vec<Sample>),
}
pub struct Form {
    repo: PathBuf,
    paths: Vec<PathBuf>,
    add: String,
    samples: Vec<Sample>,
    error: Option<String>,
    pending: bool,
    scope: tasks::Scope,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}
impl RepoTab {
    pub(in crate::repo) fn open_linked_machines(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let mut form = Form {
            repo: self.path.clone(),
            paths: Vec::new(),
            add: String::new(),
            samples: Vec::new(),
            error: None,
            pending: false,
            scope: tasks::Scope(self._jobs.0.child()),
            tx,
            rx,
        };
        let repo = form.repo.clone();
        form.submit(ctx, false, move || {
            Event::Links(git::linked_repositories(&repo))
        });
        self.dialog = Some(super::Dialog::LinkedMachines(Box::new(form)));
    }
}
impl Form {
    fn submit(
        &mut self,
        ctx: &egui::Context,
        mutation: bool,
        work: impl FnOnce() -> Event + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let key = mutation.then(|| crate::repo::messages::mutation_key(&self.repo));
        self.error = None;
        let pool = match tasks::pool() {
            Ok(pool) => pool,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        match pool.submit(
            self.scope.0.clone(),
            tasks::Priority::Interactive,
            key,
            Box::new(move || {
                let event = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                    .unwrap_or_else(|_| {
                        Event::Links(Err(crate::i18n::text("repo-core-1364").into()))
                    });
                let _ = tx.send(event);
                ctx.request_repaint();
            }),
        ) {
            Ok(()) => self.pending = true,
            Err(e) => self.error = Some(e.into()),
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Outcome {
        while let Ok(event) = self.rx.try_recv() {
            self.pending = false;
            match event {
                Event::Links(Ok(paths)) => {
                    self.paths = paths;
                    self.samples.clear();
                }
                Event::Links(Err(e)) => self.error = Some(e),
                Event::Facts(samples) => self.samples = samples,
            }
        }
        ui.set_width(800.0);
        ui.heading(crate::i18n::text("repo-core-1365"));
        ui.label(crate::i18n::text("repo-core-1366"));
        let mut remove = None;
        for path in &self.paths {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {}",
                    crate::teleport::Machine::of(path).label(),
                    path.display()
                ));
                if path != &self.repo
                    && ui
                        .add_enabled(
                            !self.pending,
                            egui::Button::new(crate::i18n::text("repo-core-1367")),
                        )
                        .clicked()
                {
                    remove = Some(path.clone());
                }
            });
        }
        if let Some(path) = remove {
            let repo = self.repo.clone();
            self.submit(ui.ctx(), true, move || {
                Event::Links(
                    git::unlink_repository(&repo, &path)
                        .and_then(|_| git::linked_repositories(&repo)),
                )
            });
        }
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.add)
                .on_hover_text(crate::i18n::text("repo-core-1368"));
            if ui
                .add_enabled(
                    !self.pending && !self.add.trim().is_empty(),
                    egui::Button::new(crate::i18n::text("repo-core-1369")),
                )
                .clicked()
            {
                let repo = self.repo.clone();
                let path = self.add.clone();
                self.submit(ui.ctx(), true, move || {
                    Event::Links(
                        git::link_repository(&repo, &path)
                            .and_then(|_| git::linked_repositories(&repo)),
                    )
                });
            }
        });
        if ui
            .add_enabled(
                !self.pending && !self.paths.is_empty(),
                egui::Button::new(crate::i18n::text("repo-core-1370")),
            )
            .clicked()
        {
            let paths = self.paths.clone();
            self.submit(ui.ctx(), false, move || {
                let mut samples = Vec::new();
                for path in paths {
                    if tasks::cancellation().is_cancelled() {
                        break;
                    }
                    let facts = git::machine_facts(&path);
                    samples.push((path, facts));
                }
                Event::Facts(samples)
            });
        }
        if self.pending {
            #[expect(
                clippy::disallowed_methods,
                reason = "Keep the existing pending-indicator repaint behavior in this lint-only unit; migrating to the throttled crate spinner requires a separate behavior change"
            )]
            ui.spinner();
            ui.label(crate::i18n::text("repo-core-1371"));
        }
        if let Some(e) = &self.error {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
        }
        let reference = self.samples.first().and_then(|(_, s)| s.as_ref().ok());
        let reference_files: std::collections::BTreeMap<_, _> = reference
            .into_iter()
            .flat_map(|f| &f.files)
            .map(|f| (f.path.as_str(), f))
            .collect();
        let mut open = None;
        egui::ScrollArea::vertical()
            .max_height(470.0)
            .show(ui, |ui| {
                for (path, sample) in &self.samples {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(path.display().to_string());
                        if ui.button(crate::i18n::text("repo-core-1372")).clicked() {
                            open = Some(path.clone());
                        }
                    });
                    match sample {
                        Err(e) => {
                            ui.label(crate::i18n::message(
                                "repo-core-1373",
                                &[("e", e.to_string().into())],
                            ));
                        }
                        Ok(f) => {
                            if let Some(base) = reference {
                                ui.label(comparison_label(base, f));
                                if base.test_command_digest != f.test_command_digest {
                                    ui.label(crate::i18n::text("repo-core-1374"));
                                }
                            } else {
                                ui.label(crate::i18n::text("repo-core-1375"));
                            }
                            ui.label(crate::i18n::message(
                                "repo-core-1376",
                                &[
                                    ("machine", f.machine.to_string().into()),
                                    ("head", f.head.to_string().into()),
                                    (
                                        "untested",
                                        f.stored_test
                                            .as_deref()
                                            .unwrap_or(crate::i18n::text("repo-core-1377"))
                                            .to_string()
                                            .into(),
                                    ),
                                ],
                            ));
                            ui.label(crate::i18n::text("repo-core-1378"));
                            ui.collapsing(crate::i18n::text("repo-core-1379"), |ui| {
                                ui.label(crate::i18n::message(
                                    "repo-core-1380",
                                    &[
                                        ("count", (f.files.len()).into()),
                                        ("tree", f.tree.to_string().into()),
                                    ],
                                ));
                                for (key, value) in &f.settings {
                                    ui.monospace(format!("{key}: {value}"));
                                }
                                for (key, value) in &f.versions {
                                    ui.monospace(format!("{key}: {value}"));
                                }
                                ui.label(crate::i18n::text("repo-core-1382"));
                                for paths in &f.case_collisions {
                                    ui.monospace(paths.join(" / "));
                                }
                                ui.label(crate::i18n::text("repo-core-1383"));
                                ui.monospace(&f.mode_changes);
                                for file in &f.files {
                                    let differs =
                                        reference_files.get(file.path.as_str()).is_none_or(|b| {
                                            b.mode != file.mode || b.endings != file.endings
                                        });
                                    if differs {
                                        ui.monospace(crate::i18n::message(
                                            "repo-core-1384",
                                            &[
                                                ("path", file.path.to_string().into()),
                                                ("mode", file.mode.to_string().into()),
                                                ("endings", file.endings.to_string().into()),
                                            ],
                                        ));
                                    }
                                }
                            });
                        }
                    }
                }
            });
        ui.label(crate::i18n::text("repo-core-1385"));
        if let Some(path) = open {
            Outcome::Done(Box::new(crate::repo::Action::OpenTab(path)))
        } else if ui.button(crate::i18n::text("repo-core-0595")).clicked() {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }
}
fn comparison_label(a: &git::MachineFacts, b: &git::MachineFacts) -> &'static str {
    match git::compare_machine_content(a, b) {
        git::ContentComparison::Match => crate::i18n::text("repo-core-1386"),
        git::ContentComparison::Different => crate::i18n::text("repo-core-1387"),
        git::ContentComparison::Changed => crate::i18n::text("repo-core-1388"),
        git::ContentComparison::Incompatible => crate::i18n::text("repo-core-1389"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_linked_copy_dialog_renders_injected_results_without_jobs() {
        let sample = |path: &str, verdict: &str| git::MachineFacts {
            repository: path.into(),
            machine: path.into(),
            head: "a".repeat(40),
            tree: "b".repeat(40),
            content_digest: "same".into(),
            clean: true,
            files: Vec::new(),
            case_collisions: Vec::new(),
            mode_changes: String::new(),
            settings: Default::default(),
            versions: Default::default(),
            test_command_digest: "command".into(),
            stored_test: Some(verdict.into()),
        };
        let a = sample("/synthetic/local", "good");
        let b = sample("ssh://synthetic/repo", "bad");
        assert!(comparison_label(&a, &b).contains("matches"));
        let (tx, rx) = mpsc::channel();
        let mut form = Form {
            repo: a.repository.clone(),
            paths: vec![a.repository.clone(), b.repository.clone()],
            add: String::new(),
            samples: vec![(a.repository.clone(), Ok(a)), (b.repository.clone(), Ok(b))],
            error: None,
            pending: false,
            scope: Default::default(),
            tx,
            rx,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1200.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(matches!(form.show(ui), Outcome::Open));
            },
        );
        output.textures_delta.clear();
        let mut text = String::new();
        fn collect(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(t) => text.push_str(&t.galley.job.text),
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        collect(s, text);
                    }
                }
                _ => {}
            }
        }
        for shape in output.shapes {
            collect(&shape.shape, &mut text);
        }
        assert!(text.contains("stored default test: good"));
        assert!(text.contains("stored default test: bad"));
        assert!(text.contains("Tracked Git content matches"));
        assert!(!form.pending);
    }
}
