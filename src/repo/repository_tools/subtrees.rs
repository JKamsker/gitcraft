use super::{Act, Data, State, Tool};
use crate::{
    git::repository_tools::subtrees as logic,
    repo::{Action, RepoTab},
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
            let profile = crate::settings::Settings::get(ctx).profile();
            let context = crate::connections::current();
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
            let ticket = tab.prepare_repository_tool(Tool::Subtrees, ctx);
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
                move |tab, result, ctx| tab.complete_repository_tool(&ticket, result, ctx),
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
#[cfg(test)]
mod tests;

mod view;
pub(super) use view::show;
