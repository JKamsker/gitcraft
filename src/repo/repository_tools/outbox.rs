use super::{Act, Data, State, Tool};
use crate::{
    git::repository_tools::outbox as logic,
    repo::{Action, Dialog, RepoTab},
};

pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    let review = matches!(&act, Act::ReviewOutbox(_));
    match act {
        Act::QueuePush(view, automatic) => {
            tab.repository_tool_mutation("Queue exact push", false, ctx, move |r| {
                logic::enqueue(r.repo, &view, automatic)
            })
        }
        Act::RemoveOutbox(view, id) => {
            tab.repository_tool_mutation("Remove outbox entry", false, ctx, move |r| {
                logic::remove(r.repo, &view, &id)
            })
        }
        Act::SendOutbox(p) => {
            tab.repository_tool_mutation("Send exact queued push", true, ctx, move |r| {
                logic::send(r, &p, false)
            })
        }
        Act::AutoOutbox(id) => {
            let profile = crate::settings::Settings::get(ctx).profile();
            tab.repository_tool_mutation(
                "Autosend opted-in plain outbox push",
                true,
                ctx,
                move |r| {
                    let p = logic::prepare(r.repo, &id, profile)?;
                    logic::send(r, &p, true)
                },
            );
        }
        Act::PrepareOutbox(id) | Act::ReviewOutbox(id) => {
            if tab.is_busy() || crate::network::refused().is_some() {
                tab.status = Some((
                    "Go online and wait for the current operation before preparing an outbox push"
                        .into(),
                    true,
                ));
                return None;
            }
            let repo = tab.path.clone();
            let expected = repo.clone();
            let profile = crate::settings::Settings::get(ctx).profile();
            let request = super::REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tab.dialog = Some(Dialog::RepositoryTools(Box::new(State {
                id: request,
                query: String::new(),
                result: None,
                filter: String::new(),
                tool: Tool::Outbox,
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
                    (if review {
                        logic::review_prepare(&repo, &id, profile)
                    } else {
                        logic::prepare(&repo, &id, profile)
                    })
                    .map(|p| Data::PreparedOutbox(Box::new(p)))
                },
                move |tab, result, _| {
                    if tab.path == expected
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
    pub(in crate::repo) fn poll_outbox(&mut self, ctx: &egui::Context) {
        use std::time::{Duration, Instant};
        if self.sleeping()
            || self.is_busy()
            || self.dialog.is_some()
            || self.outbox_reading
            || crate::network::refused().is_some()
            || self
                .outbox_last
                .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        self.outbox_reading = true;
        self.outbox_last = Some(Instant::now());
        let repo = self.path.clone();
        let expected = repo.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || logic::read(&repo),
            move |tab, result, ctx| {
                tab.outbox_reading = false;
                if tab.path != expected || tab.is_busy() || tab.dialog.is_some() {
                    return;
                }
                if let Ok(view) = result
                    && let Some(e) = view
                        .queue
                        .entries
                        .iter()
                        .find(|e| e.autosend && e.status == logic::Status::Ready)
                {
                    tab.repository_tools_act(Act::AutoOutbox(e.id.clone()), ctx);
                }
            },
        );
    }
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Push outbox");
    ui.label("Only the recorded commit is sent. Rewrites, destination changes and rejected or uncertain sends pause for review. Sending after a crash is uncertain and is never retried automatically. Queue data and protected commits are local application refs; do not mirror-push application refs.");
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(data) => data,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e.as_str());
            if ui.button("Read outbox again").clicked() {
                acts.push(Action::RepositoryTools(Act::Outbox));
            }
            return;
        }
    };
    match data {
        Data::Outbox(view) => {
            ui.label(format!("Current source: {}", view.branch));
            ui.checkbox(&mut state.compatible,"Opt in: automatically send this plain push when online, only if preflight has no findings");
            if ui
                .button("Queue this exact commit using its push route")
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::QueuePush(
                    Box::new(view.clone()),
                    state.compatible,
                )));
            }
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| {
                    for e in &view.queue.entries {
                        ui.separator();
                        ui.label(format!("{} → {} / {}", e.source, e.remote, e.destination));
                        ui.monospace(&e.commit);
                        ui.label(format!(
                            "{:?} · {} pinned endpoint(s) · autosend {}",
                            e.status,
                            e.endpoints.len(),
                            e.autosend
                        ));
                        if !e.receipt.is_empty() {
                            egui::CollapsingHeader::new("Receipt")
                                .id_salt(&e.id)
                                .show(ui, |ui| {
                                    ui.label(&e.receipt);
                                });
                        }
                        if matches!(e.status, logic::Status::Paused(_) | logic::Status::Sending)
                            && ui
                                .button("Review pinned commit and current destination…")
                                .clicked()
                        {
                            acts.push(Action::RepositoryTools(Act::ReviewOutbox(e.id.clone())));
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    e.status == logic::Status::Ready,
                                    egui::Button::new("Prepare selected send…"),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::PrepareOutbox(
                                    e.id.clone(),
                                )));
                            }
                            if ui
                                .add_enabled(
                                    e.status != logic::Status::Sending,
                                    egui::Button::new("Remove entry"),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::RemoveOutbox(
                                    Box::new(view.clone()),
                                    e.id.clone(),
                                )));
                            }
                        });
                    }
                });
        }
        Data::PreparedOutbox(p) => {
            if p.review {
                ui.colored_label(crate::theme::Tokens::get(ui.ctx()).warning,"Renewed approval keeps the ORIGINAL pinned commit and accepts the CURRENT destination and source history. Verify any uncertain prior outcome on the remote first. Autosend is disabled.");
            }
            ui.label(format!(
                "Exact commit {} → {} / {}",
                p.plan.head, p.plan.remote, p.plan.destination
            ));
            for target in &p.plan.targets {
                ui.label(target);
            }
            ui.label(format!(
                "{} outgoing commits inspected",
                p.plan.commits.len()
            ));
            egui::ScrollArea::vertical().max_height(300.0).show(ui,|ui|{
                if p.plan.findings.is_empty(){ui.label("No preflight findings in identity, signatures, messages, secrets, large blobs or remote history.");}
                for f in &p.plan.findings{ui.label(format!("{:?}: {} {} {}",f.kind,f.commit,f.path,f.detail));}
            });
            ui.checkbox(
                &mut state.acknowledged,
                "I reviewed this exact queued commit, every endpoint and preflight finding",
            );
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new("Send selected exact push"),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::SendOutbox(p.clone())));
            }
        }
        _ => {}
    }
}
