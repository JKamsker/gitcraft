use super::{Act, Data, State};
use crate::{
    git::drafts::deferred as logic,
    repo::{Action, RepoTab},
};
use std::sync::atomic::Ordering;
#[derive(Default)]
pub(in crate::repo) struct Cache {
    last: Option<std::time::Instant>,
    epoch: u64,
    context: Option<super::owner::Scope>,
    pending: Option<u64>,
    pub(super) automatic: Option<u64>,
    pub(super) outbox_poll: Option<u64>,
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
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(super) fn context_changed(&self, current: &super::owner::Scope) -> bool {
        self.context.as_ref().is_some_and(|old| old != current)
    }
    pub(super) fn remember_context(&mut self, current: super::owner::Scope) {
        self.context = Some(current);
    }
    // Epoch exhaustion must fail closed rather than reuse an accepted lifetime.
    #[allow(clippy::expect_used)]
    pub(super) fn reset_requests(&mut self) {
        self.epoch = self
            .epoch
            .checked_add(1)
            .expect("repository tool epoch exhausted");
        self.invalidate();
        self.reading = false;
        self.pending = None;
        self.automatic = None;
        self.outbox_poll = None;
        self.markers.clear();
        self.context = None;
    }

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
            let repo = tab.path.clone();
            let old = tab
                .refs
                .local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect::<std::collections::BTreeMap<_, _>>();
            tab.stacks.preview = None;
            let ticket = tab.prepare_repository_tool(super::Tool::Deferred, ctx);
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
                move |tab, result, ctx| tab.complete_repository_tool(&ticket, result, ctx),
            );
        }
        other => return Some(other),
    }
    None
}
impl RepoTab {
    pub(in crate::repo) fn poll_deferred(&mut self, ctx: &egui::Context) {
        let scope = self.repository_tool_scope(ctx);
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
        let request = super::REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.deferred.pending = Some(request);
        self.then_or(
            ctx,
            false,
            Err,
            move || logic::read(&repo, &|| !crate::tasks::cancellation().is_cancelled()),
            move |tab, result, ctx| {
                if tab.repository_tool_scope(ctx) != scope || tab.deferred.pending != Some(request)
                {
                    return;
                }
                tab.deferred.reading = false;
                tab.deferred.pending = None;
                if let Ok(v) = result {
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
#[cfg(test)]
#[path = "deferred/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "deferred/fluent_r1_caption_tests.rs"]
mod fluent_r1_caption_tests;

mod view;
#[cfg(test)]
use view::candidate_comparison;
pub(super) use view::show;

#[cfg(test)]
#[path = "deferred/request_tests.rs"]
mod request_tests;
