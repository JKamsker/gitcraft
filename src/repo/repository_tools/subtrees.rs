use super::{Act, Data, State, Tool};
use crate::{
    git::repository_tools::subtrees as logic,
    repo::{Action, Dialog, RepoTab},
};
#[derive(Clone)]
pub(crate) enum Job {
    Register(String, String, String, logic::History),
    Fetch(String),
    Prepare(String, logic::Kind),
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
                Job::Prepare(id, kind) => logic::prepare(r, &view, &id, kind),
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
                            "Subtree preparation used a previous profile/sign-in; prepare again"
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
                tab.status = Some((
                    "Prepared subtree profile/sign-in changed; prepare and approve again".into(),
                    true,
                ));
                return None;
            }
            tab.repository_tool_mutation("Publish exact subtree split", true, ctx, move |r| {
                p.context.run(|| logic::send(r, &p))
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
fn patch(ui: &mut egui::Ui, label: &str, s: &str) {
    egui::CollapsingHeader::new(label)
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::both().max_height(210.).show(ui, |ui| {
                if s.is_empty() {
                    ui.label("No differences");
                } else {
                    ui.monospace(s);
                }
            });
        });
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Vendored folders and subtree publication");
    ui.label(logic::history_explanation());
    ui.label("For a plain vendored folder without subtree history: Fetch, inspect its differences, then explicitly Associate it with that fetched baseline. Association preserves every folder byte and records those differences as local edits; only future upstream changes are merged. Accept the reviewed association before subsequent updates. Existing Git-subtree imports can use Prepare update directly.");
    ui.label("Uses installed Git's contrib subtree helper only on Prepare/Continue. Fetch and Publish are explicit network actions. Candidates, application refs and recovery directories remain local and retained; do not mirror-push application refs. Native Unix checkout only; helper absence/failure leaves an inspectable proposal.");
    if ui.button("Refresh subtree registry").clicked() {
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
            ui.label(format!("Committed source: {} at {}", v.branch, v.head));
            ui.label(&v.helper);
            egui::CollapsingHeader::new("Register an existing or new vendored folder").show(ui,|ui|{
                ui.label("Choose an existing configured source remote; its fetch and push routes may differ. Registration records provenance only. A missing folder can be imported with Prepare update after an explicit Fetch.");
                ui.horizontal(|ui|{ui.label("Folder:");ui.text_edit_singleline(&mut state.filter);});
                if state.query.is_empty(){state.query=v.remotes.first().cloned().unwrap_or_default();}
                ui.horizontal(|ui|{ui.label("Source remote:");egui::ComboBox::from_id_salt("subtree-remote").selected_text(&state.query).show_ui(ui,|ui|{for remote in &v.remotes{ui.selectable_value(&mut state.query,remote.clone(),remote);}});});
                ui.horizontal(|ui|{ui.label("Source branch:");ui.text_edit_singleline(&mut state.parent);});
                ui.radio_value(&mut state.compatible,true,"Squashed import");ui.radio_value(&mut state.compatible,false,"Full upstream history");
                if ui.button("Register source locally").clicked(){job(acts,v,Job::Register(state.filter.clone(),state.query.clone(),state.parent.clone(),if state.compatible{logic::History::Squash}else{logic::History::Full}));}
            });
            egui::ScrollArea::vertical().id_salt("subtree-registry").max_height(590.).show(ui,|ui|{
                for s in &v.state.sources {
                    ui.push_id(&s.id,|ui|{
                        ui.separator();ui.strong(format!("{} ← {}/{} · {:?}",s.prefix,s.remote,s.branch,s.history));
                        if let Some(oid)=&s.fetched{ui.monospace(format!("Fetched source: {oid}"));}
                        if let Some(oid)=&s.imported{ui.monospace(format!("Last accepted import: {oid}"));}
                        ui.horizontal_wrapped(|ui|{
                            if ui.button("Inspect local edits").clicked(){acts.push(Action::RepositoryTools(Act::Subtrees(s.id.clone())));}
                            if ui.button("Fetch registered branch").clicked(){job(acts,v,Job::Fetch(s.id.clone()));}
                            if s.imported.is_none() && ui.add_enabled(s.fetched.is_some() && state.acknowledged,egui::Button::new("Associate existing folder with fetched baseline")).clicked(){job(acts,v,Job::Prepare(s.id.clone(),logic::Kind::Associate));}
                            if ui.add_enabled(s.fetched.is_some(),egui::Button::new("Prepare update in own worktree")).clicked(){job(acts,v,Job::Prepare(s.id.clone(),logic::Kind::Update));}
                            if ui.button("Split committed folder history").clicked(){job(acts,v,Job::Prepare(s.id.clone(),logic::Kind::Split));}
                        });
                        if let Some(l)=&v.local && l.source==s.id{
                            ui.label(match (&l.baseline,&s.imported){(Some(base),Some(_))=>format!("Committed local edits relative to last accepted upstream {base}"),(Some(base),None)=>format!("Folder versus fetched upstream {base}; original import baseline is unknown, so this also includes upstream differences"),_=>"Fetch source explicitly to compare committed folder contents".into()});
                            patch(ui,"Committed folder changes",&l.committed);patch(ui,"Staged folder changes",&l.staged);patch(ui,"Unstaged folder changes",&l.working);
                            if !l.untracked.is_empty(){ui.label("Untracked files (not included in split publication):");for p in &l.untracked{ui.monospace(p);}}
                        }
                    });
                }
                for c in &v.state.candidates {
                    ui.push_id(&c.id,|ui|{
                        ui.separator();ui.strong(format!("{} · {:?} · {:?}",c.id,c.kind,c.stage));ui.monospace(format!("{} at {}",c.branch,c.old));
                        if let Some(oid)=&c.result{ui.monospace(format!("Result: {oid}"));}
                        ui.label(&c.notice);ui.monospace(c.worktree.display().to_string());
                        if c.identity.is_some() && ui.button("Open retained candidate").clicked(){acts.push(Action::OpenWorktree(c.worktree.clone()));}
                        if let Some((id,diff))=&v.candidate_diff && id==&c.id{patch(ui,"Exact proposed branch patch",diff);}
                        ui.horizontal_wrapped(|ui|{
                            if c.kind!=logic::Kind::Split && c.result.is_some() && ui.button("Review candidate patch").clicked(){acts.push(Action::RepositoryTools(Act::Subtrees(c.id.clone())));}
                            if c.kind==logic::Kind::Update && matches!(c.stage,logic::Stage::Conflict|logic::Stage::Running|logic::Stage::Paused) && ui.button("Continue / inspect resolved candidate").clicked(){job(acts,v,Job::Continue(c.id.clone()));}
                            if c.stage==logic::Stage::Accepting && ui.button("Reconcile exact acceptance receipt").clicked(){job(acts,v,Job::Reconcile(c.id.clone()));}
                            let reviewed=v.candidate_diff.as_ref().is_some_and(|(id,_)|id==&c.id);
                            if c.stage==logic::Stage::Ready && reviewed && ui.add_enabled(state.acknowledged,egui::Button::new("Accept this subtree update")).clicked(){job(acts,v,Job::Accept(c.id.clone()));}
                            if c.kind==logic::Kind::Split && matches!(c.stage,logic::Stage::SplitReady|logic::Stage::Sending|logic::Stage::Paused) {
                                let renewed=c.stage!=logic::Stage::SplitReady;
                                if ui.add_enabled(!renewed||state.acknowledged,egui::Button::new(if renewed{"Prepare a renewed publication review"}else{"Review split publication"})).clicked(){acts.push(Action::RepositoryTools(Act::SubtreePreparePush(Box::new(v.clone()),c.id.clone(),renewed)));}
                            }
                            if !matches!(c.stage,logic::Stage::Closed|logic::Stage::Accepting|logic::Stage::Sending) && ui.button("Close proposal; retain all work").clicked(){job(acts,v,Job::Close(c.id.clone()));}
                        });
                    });
                }
            });
            ui.checkbox(&mut state.acknowledged,"I reviewed this exact candidate patch / fetched association baseline; for a renewed send I inspected the remote after any rejected/uncertain attempt");
            ui.label("Accept requires a completely clean original checkout, including untracked and ignored files, and the exact original branch/OID. Resolve conflicts and stage their resolutions in the candidate worktree. Uncertain acceptance is reconciled without rerunning it. No candidate is cleaned up automatically.");
        }
        Ok(Data::SubtreePush(p)) => {
            ui.strong(format!(
                "Exact split {} → {}/{}",
                p.plan.head, p.plan.remote, p.plan.destination
            ));
            ui.label("Only this split commit is sent. No later original or candidate edits join it. Multiple endpoint writes are not atomic. Rejection or missing acknowledgement pauses the proposal; no automatic retry.");
            for target in &p.plan.targets {
                ui.monospace(crate::git::redact(target, None));
            }
            ui.label(format!("{} outgoing commits", p.plan.commits.len()));
            for f in &p.plan.findings {
                ui.label(format!(
                    "{:?}: {} {} {}",
                    f.kind, f.commit, f.path, f.detail
                ));
            }
            ui.checkbox(&mut state.acknowledged,"I reviewed the exact split, all destination endpoints, selected profile/account and preflight findings");
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new("Send this exact subtree split"),
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
