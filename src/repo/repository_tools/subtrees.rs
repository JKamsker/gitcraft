use super::{Act, Data, State, Tool};
use crate::{
    git::repository_tools::subtrees as logic,
    repo::{Action, Dialog, RepoTab},
};
#[derive(Clone)]
pub(crate) enum Job {
    Register(String, String, String, logic::History),
    Fetch(String),
    Prepare(String, logic::Kind, bool),
    Continue(String),
    Accept(String),
    Reconcile(String),
    Close(String),
}
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::SubtreeJob(view, job) => {
            let network = matches!(job, Job::Fetch(_));
            tab.repository_tool_mutation("Subtree operation", network, ctx, move |r| match job {
                Job::Register(path, remote, branch, history) => {
                    logic::register(r, &view, path, remote, branch, history)
                }
                Job::Fetch(id) => logic::fetch(r, &view, &id),
                Job::Prepare(id, kind, sign_commits) => {
                    logic::prepare(r, &view, &id, kind, sign_commits)
                }
                Job::Continue(id) => logic::resume(r, &view, &id),
                Job::Accept(id) => logic::accept(r, &view, &id),
                Job::Reconcile(id) => logic::reconcile(r, &view, &id),
                Job::Close(id) => logic::close(r, &view, &id),
            });
        }
        Act::SubtreePreparePush(view, id, renewed) => {
            if tab.is_busy() {
                return None;
            }
            let repo = tab.path.clone();
            let expected = repo.clone();
            let profile = crate::settings::Settings::get(ctx).profile();
            let expected_profile = profile.clone();
            let context = crate::connections::current();
            let expected_context = context.clone();
            let generation = tab.https_generation;
            let credentials = tab.credentials.clone();
            if !selection(
                &context,
                &profile,
                generation,
                &tab.connection_context,
                &profile,
                generation,
            ) {
                return None;
            }
            let request = super::REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tab.dialog = Some(Dialog::RepositoryTools(Box::new(State {
                id: request,
                query: String::new(),
                result: None,
                filter: String::new(),
                tool: Tool::Subtrees,
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
                    context
                        .run(|| {
                            logic::prepare_publish(
                                &repo,
                                &view,
                                &id,
                                profile,
                                credentials.as_ref(),
                                generation,
                                renewed,
                            )
                        })
                        .map(|p| Data::SubtreePush(Box::new(p)))
                },
                move |tab, result, ctx| {
                    let selected = crate::settings::Settings::get(ctx).profile();
                    let result = if crate::connections::current() == expected_context
                        && selection(
                            &expected_context,
                            &expected_profile,
                            generation,
                            &tab.connection_context,
                            &selected,
                            tab.https_generation,
                        ) {
                        result
                    } else {
                        Err(
                            crate::i18n::text("repo-workflows-repository_tools-act-27cc6eb8")
                                .into(),
                        )
                    };
                    if tab.path == expected
                        && let Some(Dialog::RepositoryTools(s)) = &mut tab.dialog
                        && s.id == request
                    {
                        s.result = Some(result);
                    }
                },
            );
        }
        Act::SubtreeSend(p) => {
            let profile = crate::settings::Settings::get(ctx).profile();
            if p.context != crate::connections::current()
                || !selection(
                    &p.context,
                    &p.plan.profile,
                    p.generation,
                    &tab.connection_context,
                    &profile,
                    tab.https_generation,
                )
            {
                tab.status = Some(crate::repo::Notice::plain((
                    "Prepared subtree profile/sign-in changed; prepare and approve again".into(),
                    true,
                )));
                return None;
            }
            let plan = p.plan.clone();
            tab.reviewed_send(&plan, ctx, move |tab, reviewed, ctx| {
                tab.repository_tool_mutation("Publish exact subtree split", true, ctx, move |r| {
                    p.context.run(|| logic::send(r, &p, &reviewed))
                });
            });
        }
        other => return Some(other),
    }
    None
}
fn selection(
    context: &crate::connections::Context,
    profile: &crate::settings::profiles::UserProfile,
    generation: u64,
    actual: &crate::connections::Context,
    selected: &crate::settings::profiles::UserProfile,
    actual_generation: u64,
) -> bool {
    context == actual
        && profile.id == selected.id
        && profile.connections == selected.connections
        && context.choices == profile.connections
        && generation == actual_generation
}
fn job(acts: &mut Vec<Action>, v: &logic::View, j: Job) {
    acts.push(Action::RepositoryTools(Act::SubtreeJob(
        Box::new(v.clone()),
        j,
    )));
}
fn patch(ui: &mut egui::Ui, id: &str, label: &str, s: &str) {
    egui::CollapsingHeader::new(label)
        .id_salt(id)
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::both().max_height(210.).show(ui, |ui| {
                if s.is_empty() {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-patch-eef91a52",
                    ));
                } else {
                    ui.monospace(s);
                }
            });
        });
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-e949fe01",
    ));
    ui.label(logic::history_explanation());
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-16d60e39",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-a23601d6",
    ));
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-2ddef7c7",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Subtrees(String::new())));
    }
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    match result {
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
        }
        Ok(Data::Subtrees(v)) => {
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-ad16979e",
                &[
                    ("v-branch-1", (v.branch).clone().into()),
                    ("v-head-2", (v.head).clone().into()),
                ],
            ));
            ui.label(&v.helper);
            let signing_id = egui::Id::new(("subtree-sign-commits", state.id));
            let mut sign_commits = ui
                .ctx()
                .data_mut(|data| *data.get_temp_mut_or_insert_with(signing_id, || v.sign_commits));
            if ui
                .checkbox(
                    &mut sign_commits,
                    crate::i18n::text("repo-workflows-repository_tools-subtrees-sign-commits"),
                )
                .changed()
            {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(signing_id, sign_commits));
            }
            egui::CollapsingHeader::new(crate::i18n::text(
                "repo-workflows-repository_tools-show-43a8ef7d",
            ))
            .id_salt("subtree-discovery")
            .show(ui, |ui| {
                ui.label(crate::i18n::text(
                    "repo-workflows-repository_tools-show-ea03ec13",
                ));
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-show-a6b3283e",
                    ));
                    ui.text_edit_singleline(&mut state.filter);
                });
                if state.query.is_empty() {
                    state.query = v.remotes.first().cloned().unwrap_or_default();
                }
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-show-666eca6d",
                    ));
                    egui::ComboBox::from_id_salt("subtree-remote")
                        .selected_text(&state.query)
                        .show_ui(ui, |ui| {
                            for remote in &v.remotes {
                                ui.selectable_value(&mut state.query, remote.clone(), remote);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-show-4829ff52",
                    ));
                    ui.text_edit_singleline(&mut state.parent);
                });
                ui.radio_value(
                    &mut state.compatible,
                    true,
                    crate::i18n::text("repo-workflows-repository_tools-show-7e381308"),
                );
                ui.radio_value(
                    &mut state.compatible,
                    false,
                    crate::i18n::text("repo-workflows-repository_tools-show-93fdd760"),
                );
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-repository_tools-show-16312ab3",
                    ))
                    .clicked()
                {
                    job(
                        acts,
                        v,
                        Job::Register(
                            state.filter.clone(),
                            state.query.clone(),
                            state.parent.clone(),
                            if state.compatible {
                                logic::History::Squash
                            } else {
                                logic::History::Full
                            },
                        ),
                    );
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("subtree-registry")
                .max_height(590.)
                .show(ui, |ui| {
                    for s in &v.state.sources {
                        ui.push_id(&s.id, |ui| {
                            ui.separator();
                            ui.strong(format!(
                                "{} ← {}/{} · {:?}",
                                s.prefix, s.remote, s.branch, s.history
                            ));
                            if let Some(oid) = &s.fetched {
                                ui.monospace(crate::i18n::message(
                                    "repo-workflows-repository_tools-show-e0e54b30",
                                    &[("oid", (oid).clone().into())],
                                ));
                            }
                            if let Some(oid) = &s.imported {
                                ui.monospace(crate::i18n::message(
                                    "repo-workflows-repository_tools-show-a6888267",
                                    &[("oid", (oid).clone().into())],
                                ));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-7432d850",
                                    ))
                                    .clicked()
                                {
                                    acts.push(Action::RepositoryTools(Act::Subtrees(s.id.clone())));
                                }
                                if ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-1e9112fe",
                                    ))
                                    .clicked()
                                {
                                    job(acts, v, Job::Fetch(s.id.clone()));
                                }
                                if s.imported.is_none()
                                    && ui
                                        .add_enabled(
                                            s.fetched.is_some() && state.acknowledged,
                                            egui::Button::new(crate::i18n::text(
                                                "repo-workflows-repository_tools-show-1a6e5e0d",
                                            )),
                                        )
                                        .clicked()
                                {
                                    job(
                                        acts,
                                        v,
                                        Job::Prepare(
                                            s.id.clone(),
                                            logic::Kind::Associate,
                                            sign_commits,
                                        ),
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        s.fetched.is_some(),
                                        egui::Button::new(crate::i18n::text(
                                            "repo-workflows-repository_tools-show-1f711390",
                                        )),
                                    )
                                    .clicked()
                                {
                                    job(
                                        acts,
                                        v,
                                        Job::Prepare(
                                            s.id.clone(),
                                            logic::Kind::Update,
                                            sign_commits,
                                        ),
                                    );
                                }
                                if ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-99d05df7",
                                    ))
                                    .clicked()
                                {
                                    job(
                                        acts,
                                        v,
                                        Job::Prepare(
                                            s.id.clone(),
                                            logic::Kind::Split,
                                            sign_commits,
                                        ),
                                    );
                                }
                            });
                            if let Some(l) = &v.local
                                && l.source == s.id
                            {
                                ui.label(match (&l.baseline, &s.imported) {
                                    (Some(base), Some(_)) => crate::i18n::message(
                                        "repo-workflows-repository_tools-show-ff1cf26d",
                                        &[("base", (base).clone().into())],
                                    ),
                                    (Some(base), None) => crate::i18n::message(
                                        "repo-workflows-repository_tools-show-f5b402c5",
                                        &[("base", (base).clone().into())],
                                    ),
                                    _ => crate::i18n::text(
                                        "repo-workflows-repository_tools-show-e8331aba",
                                    )
                                    .into(),
                                });
                                patch(
                                    ui,
                                    "repo-workflows-repository_tools-show-d3197597",
                                    crate::i18n::text(
                                        "repo-workflows-repository_tools-show-d3197597",
                                    ),
                                    &l.committed,
                                );
                                patch(
                                    ui,
                                    "repo-workflows-repository_tools-show-2250c14b",
                                    crate::i18n::text(
                                        "repo-workflows-repository_tools-show-2250c14b",
                                    ),
                                    &l.staged,
                                );
                                patch(
                                    ui,
                                    "repo-workflows-repository_tools-show-cb9b7596",
                                    crate::i18n::text(
                                        "repo-workflows-repository_tools-show-cb9b7596",
                                    ),
                                    &l.working,
                                );
                                if !l.untracked.is_empty() {
                                    ui.label(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-41251e4b",
                                    ));
                                    for p in &l.untracked {
                                        ui.monospace(p);
                                    }
                                }
                            }
                        });
                    }
                    for c in &v.state.candidates {
                        ui.push_id(&c.id, |ui| {
                            ui.separator();
                            ui.strong(format!("{} · {:?} · {:?}", c.id, c.kind, c.stage));
                            ui.monospace(crate::i18n::message(
                                "repo-workflows-repository_tools-show-8bb0f122",
                                &[
                                    ("c-branch-1", (c.branch).clone().into()),
                                    ("c-old-2", (c.old).clone().into()),
                                ],
                            ));
                            if let Some(oid) = &c.result {
                                ui.monospace(crate::i18n::message(
                                    "repo-workflows-repository_tools-show-4498585c",
                                    &[("oid", (oid).clone().into())],
                                ));
                            }
                            ui.label(&c.notice);
                            ui.monospace(c.worktree.display().to_string());
                            if c.identity.is_some()
                                && ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-05e1813b",
                                    ))
                                    .clicked()
                            {
                                acts.push(Action::OpenWorktree(c.worktree.clone()));
                            }
                            if let Some((id, diff)) = &v.candidate_diff
                                && id == &c.id
                            {
                                patch(
                                    ui,
                                    "repo-workflows-repository_tools-show-36c73d64",
                                    crate::i18n::text(
                                        "repo-workflows-repository_tools-show-36c73d64",
                                    ),
                                    diff,
                                );
                            }
                            ui.horizontal_wrapped(|ui| {
                                if c.kind != logic::Kind::Split
                                    && c.result.is_some()
                                    && ui
                                        .button(crate::i18n::text(
                                            "repo-workflows-repository_tools-show-66dfd7a6",
                                        ))
                                        .clicked()
                                {
                                    acts.push(Action::RepositoryTools(Act::Subtrees(c.id.clone())));
                                }
                                if c.kind == logic::Kind::Update
                                    && matches!(
                                        c.stage,
                                        logic::Stage::Conflict
                                            | logic::Stage::Running
                                            | logic::Stage::Paused
                                    )
                                    && ui
                                        .button(crate::i18n::text(
                                            "repo-workflows-repository_tools-show-9bb7e70d",
                                        ))
                                        .clicked()
                                {
                                    job(acts, v, Job::Continue(c.id.clone()));
                                }
                                if c.stage == logic::Stage::Accepting
                                    && ui
                                        .button(crate::i18n::text(
                                            "repo-workflows-repository_tools-show-b0a1bb3e",
                                        ))
                                        .clicked()
                                {
                                    job(acts, v, Job::Reconcile(c.id.clone()));
                                }
                                let reviewed =
                                    v.candidate_diff.as_ref().is_some_and(|(id, _)| id == &c.id);
                                if c.stage == logic::Stage::Ready
                                    && reviewed
                                    && ui
                                        .add_enabled(
                                            state.acknowledged,
                                            egui::Button::new(crate::i18n::text(
                                                "repo-workflows-repository_tools-show-b0e84ec1",
                                            )),
                                        )
                                        .clicked()
                                {
                                    job(acts, v, Job::Accept(c.id.clone()));
                                }
                                if c.kind == logic::Kind::Split
                                    && matches!(
                                        c.stage,
                                        logic::Stage::SplitReady
                                            | logic::Stage::Sending
                                            | logic::Stage::Paused
                                    )
                                {
                                    let renewed = c.stage != logic::Stage::SplitReady;
                                    if ui
                                        .add_enabled(
                                            !renewed || state.acknowledged,
                                            egui::Button::new(if renewed {
                                                crate::i18n::text(
                                                    "repo-workflows-repository_tools-show-3e04e021",
                                                )
                                            } else {
                                                crate::i18n::text(
                                                    "repo-workflows-repository_tools-show-c7879dfc",
                                                )
                                            }),
                                        )
                                        .clicked()
                                    {
                                        acts.push(Action::RepositoryTools(
                                            Act::SubtreePreparePush(
                                                Box::new(v.clone()),
                                                c.id.clone(),
                                                renewed,
                                            ),
                                        ));
                                    }
                                }
                                if !matches!(
                                    c.stage,
                                    logic::Stage::Closed
                                        | logic::Stage::Accepting
                                        | logic::Stage::Sending
                                ) && ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-fb535d0d",
                                    ))
                                    .clicked()
                                {
                                    job(acts, v, Job::Close(c.id.clone()));
                                }
                            });
                        });
                    }
                });
            ui.checkbox(
                &mut state.acknowledged,
                crate::i18n::text("repo-workflows-repository_tools-show-d5714f7d"),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-show-4601e33d",
            ));
        }
        Ok(Data::SubtreePush(p)) => {
            ui.strong(crate::i18n::message(
                "repo-workflows-repository_tools-show-9e153558",
                &[
                    ("p-plan-head-1", (p.plan.head).clone().into()),
                    ("p-plan-remote-2", (p.plan.remote).clone().into()),
                    ("p-plan-destination-3", (p.plan.destination).clone().into()),
                ],
            ));
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-show-ed38427e",
            ));
            for target in &p.plan.targets {
                ui.monospace(crate::git::redact(target, None));
            }
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-83e94e8a",
                &[("p-plan-commits-len-1", (p.plan.commits.len()).into())],
            ));
            for f in &p.plan.findings {
                ui.label(format!(
                    "{:?}: {} {} {}",
                    f.kind, f.commit, f.path, f.detail
                ));
            }
            ui.checkbox(
                &mut state.acknowledged,
                crate::i18n::text("repo-workflows-repository_tools-show-5dfdfed0"),
            );
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-07345f3f",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::SubtreeSend(p.clone())));
            }
        }
        Ok(_) => {}
    }
}
#[cfg(test)]
mod tests;
