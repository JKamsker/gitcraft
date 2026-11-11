use super::{Act, Data, State, Tool};
use crate::{
    git::repository_tools::outbox as logic,
    repo::{Action, RepoTab},
};

fn selection_current(
    context: &crate::connections::Context,
    profile: u64,
    choices: &crate::connections::Choices,
    generation: u64,
    actual_context: &crate::connections::Context,
    actual_profile: &crate::settings::profiles::UserProfile,
    actual_generation: u64,
) -> bool {
    context == actual_context
        && profile == actual_profile.id
        && choices == &actual_profile.connections
        && generation == actual_generation
}
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    let review = matches!(&act, Act::ReviewOutbox(_));
    match act {
        Act::QueuePush(view, automatic) => {
            let profile = crate::settings::Settings::get(ctx).profile();
            let context = crate::connections::current();
            if tab.connection_context != context {
                tab.status = Some(crate::repo::Notice::plain((
                    "Connection changed; reopen outbox".into(),
                    true,
                )));
                return None;
            }
            tab.repository_tool_mutation("Queue exact push", false, ctx, move |r| {
                context.run(|| logic::enqueue(r.repo, &view, automatic, &profile, r.credentials))
            })
        }
        Act::RemoveOutbox(view, id) => {
            tab.repository_tool_mutation("Remove outbox entry", false, ctx, move |r| {
                logic::remove(r.repo, &view, &id)
            })
        }
        Act::SendOutbox(p) => start_send(tab, p, false, ctx),
        Act::AutoOutbox(id) => {
            if tab.is_busy() || tab.echo_push_pending() {
                return None;
            }
            let (repo, credentials) = (tab.path.clone(), tab.credentials.clone());
            let profile = crate::settings::Settings::get(ctx).profile();
            let context = crate::connections::current();
            let generation = tab.https_generation;
            let scope = tab.repository_tool_scope(ctx);
            let read = super::effects::Read {
                tool: Tool::Outbox,
                query: String::new(),
            };
            let (mut owner, ticket) = State::open(&read, scope);
            tab.deferred.automatic = Some(owner.id);

            tab.then_or(
                ctx,
                false,
                Err,
                move || context.run(|| logic::prepare(&repo, &id, profile, credentials.as_ref())),
                move |tab, result, ctx| {
                    let current = tab.repository_tool_scope(ctx);
                    if tab.deferred.automatic != Some(owner.id) {
                        return;
                    }
                    tab.deferred.automatic = None;
                    let result = result.map(|mut p| {
                        p.ui_generation = Some(generation);
                        Data::PreparedOutbox(Box::new(p))
                    });
                    let effect = owner.complete(&ticket, &current, result);
                    super::actions::settlement(tab, effect);
                    match owner.result {
                        Some(Ok(Data::PreparedOutbox(p))) => start_send(tab, p, true, ctx),
                        Some(Err(error)) => {
                            tab.status = Some(crate::repo::Notice::plain((error, true)))
                        }
                        _ => {}
                    }
                },
            );
        }
        Act::PrepareOutbox(id) | Act::ReviewOutbox(id) => {
            if tab.is_busy() || crate::network::refused().is_some() {
                tab.status = Some(crate::repo::Notice::plain((
                    "Go online and wait for the current operation before preparing an outbox push"
                        .into(),
                    true,
                )));
                return None;
            }
            let repo = tab.path.clone();
            let profile = crate::settings::Settings::get(ctx).profile();
            let context = crate::connections::current();
            let generation = tab.https_generation;
            let credentials = tab.credentials.clone();
            let ticket = tab.prepare_repository_tool(Tool::Outbox, ctx);
            tab.then_or(
                ctx,
                false,
                Err,
                move || {
                    context
                        .run(|| {
                            if review {
                                logic::review_prepare(&repo, &id, profile, credentials.as_ref())
                            } else {
                                logic::prepare(&repo, &id, profile, credentials.as_ref())
                            }
                        })
                        .map(|mut p| {
                            p.ui_generation = Some(generation);
                            Data::PreparedOutbox(Box::new(p))
                        })
                },
                move |tab, result, ctx| tab.complete_repository_tool(&ticket, result, ctx),
            );
        }
        other => return Some(other),
    }
    None
}
fn start_send(tab: &mut RepoTab, p: Box<logic::Prepared>, automatic: bool, ctx: &egui::Context) {
    let selected = crate::settings::Settings::get(ctx).profile();
    if p.context != crate::connections::current()
        || !p.ui_generation.is_some_and(|generation| {
            selection_current(
                &p.context,
                p.consent.profile,
                &p.consent.choices,
                generation,
                &tab.connection_context,
                &selected,
                tab.https_generation,
            )
        })
    {
        tab.status = Some(crate::repo::Notice::plain((
            "Prepared outbox profile/sign-in changed; prepare and approve again".into(),
            true,
        )));
        return;
    }
    let plan = p.plan.clone();
    tab.reviewed_send(&plan, ctx, move |tab, reviewed, ctx| {
        tab.repository_tool_mutation(
            if automatic {
                "Autosend opted-in plain outbox push"
            } else {
                "Send exact queued push"
            },
            true,
            ctx,
            move |r| p.context.run(|| logic::send(r, &p, automatic, &reviewed)),
        );
    });
}
impl RepoTab {
    pub(in crate::repo) fn poll_outbox(&mut self, ctx: &egui::Context) {
        use std::time::{Duration, Instant};
        let scope = self.repository_tool_scope(ctx);
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
        let request = super::REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.deferred.outbox_poll = Some(request);
        self.then_or(
            ctx,
            false,
            Err,
            move || logic::read(&repo),
            move |tab, result, ctx| {
                if tab.repository_tool_scope(ctx) != scope
                    || tab.deferred.outbox_poll != Some(request)
                {
                    return;
                }
                tab.deferred.outbox_poll = None;
                tab.outbox_reading = false;
                if tab.is_busy() || tab.dialog.is_some() {
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
#[cfg(test)]
#[path = "outbox/consent_tests.rs"]
mod consent_tests;

#[cfg(test)]
#[path = "outbox/fluent_outbox_tests.rs"]
mod fluent_outbox_tests;

mod view;
#[cfg(test)]
use view::outbox_entry_summary;
pub(super) use view::show;
