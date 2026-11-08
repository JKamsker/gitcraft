use super::{Act, Data, State};
use crate::{
    git::drafts::deferred as logic,
    repo::{Action, Dialog, RepoTab},
};
use std::sync::atomic::Ordering;
#[derive(Default)]
pub(in crate::repo) struct Cache {
    last: Option<std::time::Instant>,
    reading: bool,
    request: u64,
    markers: Vec<Marker>,
}
struct Marker {
    hash: String,
    branch: String,
    conflicts: Vec<String>,
}
impl Cache {
    pub(super) fn observe(&mut self, request: u64, view: &logic::View) {
        if request < self.request {
            return;
        }
        self.request = request;
        self.markers = view
            .jobs
            .jobs
            .iter()
            .filter(|j| !j.cancelled)
            .flat_map(|j| {
                j.steps.iter().filter(|s| !s.accepted).map(|s| Marker {
                    hash: s.old.clone(),
                    branch: s.branch.clone(),
                    conflicts: s.conflicts.clone(),
                })
            })
            .collect();
    }
    fn invalidate(&mut self) {
        self.request = super::REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.last = None;
    }
}
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::StartDeferred(view, title, specs) => {
            tab.deferred.invalidate();
            tab.repository_tool_mutation("Replay in isolated worktrees", false, ctx, move |r| {
                logic::start(r, &view, title, specs)
            });
        }
        Act::ContinueDeferred(view, id) => {
            tab.deferred.invalidate();
            tab.repository_tool_mutation("Continue isolated replay", false, ctx, move |r| {
                logic::resume(r, &view, &id)
            });
        }
        Act::AcceptDeferred(view, id, undo) => {
            tab.deferred.invalidate();
            tab.repository_tool_mutation(
                if undo {
                    "Undo accepted isolated replay"
                } else {
                    "Accept isolated replay"
                },
                false,
                ctx,
                move |r| logic::accept(r, &view, &id, undo),
            );
        }
        Act::CloseDeferred(view, id) => {
            tab.deferred.invalidate();
            tab.repository_tool_mutation("Close isolated proposal", false, ctx, move |r| {
                logic::close(r.repo, &view, &id, &|| !r.control.is_cancelled())
            });
        }
        Act::ReconcileDeferred(view, id) => {
            tab.deferred.invalidate();
            tab.repository_tool_mutation("Reconcile exact replay receipts", false, ctx, move |r| {
                logic::reconcile(r.repo, &view, &id, &|| !r.control.is_cancelled())
            });
        }
        Act::IsolatedStack(previews) => {
            let expected = tab.path.clone();
            let repo = tab.path.clone();
            let profile = crate::git::profile_config();
            let old = tab
                .refs
                .local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect::<std::collections::BTreeMap<_, _>>();
            tab.stacks.preview = None;
            let request = super::REQUESTS.fetch_add(1, Ordering::Relaxed);
            tab.dialog = Some(Dialog::RepositoryTools(Box::new(State {
                id: request,
                query: String::new(),
                result: None,
                filter: String::new(),
                tool: super::Tool::Deferred,
                acknowledged: false,
                compatible: false,
                parent: String::new(),
                effective: true,
            })));
            tab.then_or(
                ctx,
                false,
                Err,
                move || {
                    let view =
                        logic::read(&repo, &|| !crate::tasks::cancellation().is_cancelled())?;
                    let specs = previews
                        .into_iter()
                        .map(|p| {
                            Ok(logic::Spec {
                                expected: old
                                    .get(&p.step.branch)
                                    .ok_or(crate::i18n::text(
                                        "repo-workflows-repository_tools-act-e3e83f28",
                                    ))?
                                    .clone(),
                                branch: p.step.branch,
                                old_base: p.step.old_base,
                                onto: p.new_base,
                                parent: p.step.parent,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(Data::DeferredStack(view, specs))
                },
                move |tab, result, _| {
                    if tab.path == expected
                        && crate::git::profile_config() == profile
                        && let Some(Dialog::RepositoryTools(s)) = &mut tab.dialog
                        && s.id == request
                    {
                        s.result = Some(result);
                    }
                },
            );
        }
        other => return Some(other),
    }
    None
}
impl RepoTab {
    pub(in crate::repo) fn poll_deferred(&mut self, ctx: &egui::Context) {
        if self.sleeping()
            || self.deferred.reading
            || self.is_busy()
            || self
                .deferred
                .last
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(15))
        {
            return;
        }
        self.deferred.reading = true;
        self.deferred.last = Some(std::time::Instant::now());
        let repo = self.path.clone();
        let expected = repo.clone();
        let request = super::REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.then_or(
            ctx,
            false,
            Err,
            move || logic::read(&repo, &|| !crate::tasks::cancellation().is_cancelled()),
            move |tab, result, _| {
                tab.deferred.reading = false;
                if tab.path == expected
                    && let Ok(v) = result
                {
                    tab.deferred.observe(request, &v);
                }
            },
        );
    }
    pub(in crate::repo) fn deferred_badge(
        &self,
        ui: &mut egui::Ui,
        left: f32,
        y: f32,
        hash: &str,
        acts: &mut Vec<Action>,
    ) -> f32 {
        if self.ghost.drawing() {
            return 0.0;
        }
        let markers = self
            .deferred
            .markers
            .iter()
            .filter(|m| m.hash == hash)
            .collect::<Vec<_>>();
        if markers.is_empty() {
            return 0.0;
        }
        let conflict = markers.iter().any(|m| !m.conflicts.is_empty());
        let text = if conflict { "!" } else { "R" };
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.painter().text(
            egui::pos2(left + 7.0, y),
            egui::Align2::CENTER_CENTER,
            text,
            crate::theme::semibold(11.0),
            if conflict { t.danger } else { t.warning },
        );
        let response = ui.interact(
            egui::Rect::from_center_size(egui::pos2(left + 7.0, y), egui::vec2(16.0, 18.0)),
            ui.id().with(("deferred-replay", hash)),
            egui::Sense::click(),
        );
        if response.clicked() {
            acts.push(Action::RepositoryTools(Act::Deferred));
        }
        response.on_hover_ui(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-deferred-badge-a5eafc03",
            ));
            for m in markers {
                ui.label(&m.branch);
                for p in &m.conflicts {
                    ui.label(crate::i18n::message(
                        "repo-workflows-repository_tools-deferred-badge-8daa08a5",
                        &[("p", (p).clone().into())],
                    ));
                }
            }
        });
        16.0
    }
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-7d86e238",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-c757c39b",
    ));
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-7c52fd3d",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Deferred));
    }
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    if let Data::DeferredStack(view, specs) = data {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-412ec52f",
        ));
        for p in specs {
            ui.monospace(crate::i18n::message(
                "repo-workflows-repository_tools-show-b2f82ce9",
                &[
                    ("p-branch-1", (p.branch).clone().into()),
                    ("p-expected-2", (p.expected).clone().into()),
                    ("p-old-base-3", (p.old_base).clone().into()),
                    ("p-onto-4", (p.onto).clone().into()),
                ],
            ));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-1919101c",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::StartDeferred(
                Box::new(view.clone()),
                "Isolated restack".into(),
                specs.clone(),
            )));
        }
        return;
    }
    let Data::Deferred(view) = data else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-4829ff52",
        ));
        egui::ComboBox::from_id_salt("deferred-source")
            .selected_text(if state.query.is_empty() {
                &view.branch
            } else {
                &state.query
            })
            .show_ui(ui, |ui| {
                for (name, _) in &view.branches {
                    ui.selectable_value(&mut state.query, name.clone(), name);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-80c567ae",
        ));
        ui.text_edit_singleline(&mut state.filter);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-0d37e057",
        ));
        ui.text_edit_singleline(&mut state.parent);
    });
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-0256e5a0",
        ))
        .clicked()
    {
        let name = if state.query.is_empty() {
            view.branch.clone()
        } else {
            state.query.clone()
        };
        if let Some((_, old)) = view.branches.iter().find(|(n, _)| *n == name) {
            acts.push(Action::RepositoryTools(Act::StartDeferred(
                Box::new(view.clone()),
                format!("Rebase {name}"),
                vec![logic::Spec {
                    branch: name,
                    expected: old.clone(),
                    old_base: state.filter.clone(),
                    onto: state.parent.clone(),
                    parent: String::new(),
                }],
            )));
        }
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-3a7402e9"),
    );
    egui::ScrollArea::vertical()
        .max_height(430.0)
        .show(ui, |ui| {
            for job in &view.jobs.jobs {
                ui.separator();
                ui.strong(format!(
                    "{} · {} / {} replayed{}",
                    job.title,
                    job.cursor,
                    job.steps.len(),
                    if job.cancelled { " · closed" } else { "" }
                ));
                ui.label(&job.notice);
                if job.busy {
                    ui.colored_label(
                        crate::theme::Tokens::get(ui.ctx()).warning,
                        crate::i18n::text("repo-workflows-repository_tools-show-aa785909"),
                    );
                }
                for s in &job.steps {
                    ui.monospace(format!(
                        "{}: {} → {}{}",
                        s.branch,
                        s.old,
                        s.result.as_deref().unwrap_or(crate::i18n::text(
                            "repo-workflows-repository_tools-show-6bdc8428"
                        )),
                        if s.accepted { " · accepted" } else { "" }
                    ));
                    ui.label(crate::i18n::message(
                        "repo-workflows-repository_tools-show-616296e8",
                        &[
                            ("s-branch-1", (s.branch).clone().into()),
                            ("s-old-base-2", (s.old_base).clone().into()),
                            ("s-onto-3", (s.onto).clone().into()),
                        ],
                    ));
                    if s.made && ui.link(s.worktree.display().to_string()).clicked() {
                        acts.push(Action::OpenWorktree(s.worktree.clone()));
                    }
                    for p in &s.conflicts {
                        ui.colored_label(
                            crate::theme::Tokens::get(ui.ctx()).danger,
                            crate::i18n::message(
                                "repo-workflows-repository_tools-show-8daa08a5",
                                &[("p", (p).clone().into())],
                            ),
                        );
                    }
                    if let Some(new) = &s.result {
                        if ui
                            .button(crate::i18n::message(
                                "repo-workflows-repository_tools-show-19af9596",
                                &[("s-branch-1", (s.branch).clone().into())],
                            ))
                            .clicked()
                        {
                            acts.push(Action::Compare {
                                base: s.old.clone(),
                                base_label: crate::i18n::message(
                                    "repo-workflows-repository_tools-show-507eaa08",
                                    &[("s-branch-1", (s.branch).clone().into())],
                                ),
                                target: Some(new.clone()),
                                target_label: crate::i18n::message(
                                    "repo-workflows-repository_tools-show-7bd990ec",
                                    &[("s-branch-1", (s.branch).clone().into())],
                                ),
                                since_fork: false,
                            });
                        }
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if !job.cancelled
                        && job.cursor < job.steps.len()
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-98a12458",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::ContinueDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                    if !job.cancelled
                        && job.cursor == job.steps.len()
                        && ui
                            .add_enabled(
                                state.acknowledged && !job.busy,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-ba765b08",
                                )),
                            )
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::AcceptDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                            false,
                        )));
                    }
                    if job.steps.iter().any(|s| s.accepted)
                        && ui
                            .add_enabled(
                                state.acknowledged && !job.busy,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-440174a7",
                                )),
                            )
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::AcceptDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                            true,
                        )));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-repository_tools-show-5471de9c",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::ReconcileDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                    if !job.cancelled
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-33463eb4",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::CloseDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_deferred_panel_keeps_conflicts_reachable_and_rejects_late_marker_snapshots() {
        let view = logic::tests::view();
        let mut cache = Cache::default();
        cache.observe(20, &view);
        assert_eq!(cache.markers.len(), 1);
        let mut closed = view.clone();
        closed.jobs.jobs[0].cancelled = true;
        cache.observe(21, &closed);
        cache.observe(20, &view);
        assert!(
            cache.markers.is_empty(),
            "late reads cannot restore a closed marker"
        );
        let path = view.jobs.jobs[0].steps[0].worktree.clone();
        let mut state = State {
            id: 100,
            query: String::new(),
            result: Some(Ok(Data::Deferred(view))),
            filter: String::new(),
            tool: super::super::Tool::Deferred,
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        };
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.contains(&"Conflict waiting: src/shared.rs"));
        assert!(texts.contains(&"Continue / reconcile candidate worktree"));
        assert!(texts.contains(&"Reconcile exact acceptance receipts"));
        assert!(!texts.contains(&"Accept ready candidates"));
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == path.to_string_lossy() => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("retained candidate worktree link");
        output.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        output.textures_delta.clear();
        assert!(
            acts.iter()
                .any(|a| matches!(a,Action::OpenWorktree(p) if p==&path))
        );
    }
}
