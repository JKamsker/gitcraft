//! Modal window orchestration and dialog outcome dispatch.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn dialogs(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.history_activity_window(ctx, acts);
        self.atlas_window(ctx);
        self.sparse_window(ctx, acts);
        self.cleanup_window(ctx);
        self.worktree_cleanup_window(ctx);
        self.growth_window(ctx);
        self.postcard_window(ctx);
        if let Some(Dialog::Stash(d)) = &mut self.dialog {
            d.seed(self.stash_picks.picked.iter().cloned());
            d.keep_changed(&self.work);
            if !std::mem::replace(&mut d.opened, true)
                && matches!(self.stash_picks.staged, StagedSupport::Failed(_))
            {
                self.stash_picks.staged = StagedSupport::Unknown;
            }
            self.check_staged_stash(ctx);
        }
        // What the bisect dialogs say.
        let bisect_blocker = self.bisect_blocker();
        let machine = match crate::remote::parse(&self.path) {
            Some(target) => crate::i18n::message(
                "repo-core-1391",
                &[("label", target.host.label().to_string().into())],
            ),
            None => crate::i18n::text("repo-core-1392").to_owned(),
        };
        let is_remote = crate::remote::is_remote(&self.path);
        let current_branch = self.refs.current.clone();
        let working_draft_undo = self.working_draft_undo_available();
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let modal = egui::Modal::new(egui::Id::new("gitcraft-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110));

        let local = &self.refs.local;
        let work = &self.work;
        let staged_support = &self.stash_picks.staged;
        let worktrees = &self.worktrees;
        let remote_names: Vec<&str> = self.remotes.iter().map(|r| r.name.as_str()).collect();
        let remote_branches = &self.refs.remotes;
        let idle = self.busy_op.is_none();
        // What a new branch's undo checks out again.
        let previous = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| self.refs.head.clone());
        // The layers of stacks a renamed branch takes along.
        let stack_parents = &self.stacks.recorded;
        let response = modal.show(ctx, |ui| {
            ui.set_width(380.0);
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            match dialog {
                Dialog::PushLoading { .. } => {
                    ui.heading(crate::i18n::text("repo-core-1394"));
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(crate::i18n::text("repo-core-1395"));
                    if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
                        Outcome::Cancel
                    } else {
                        Outcome::Open
                    }
                }
                Dialog::PushPreflight { plan, acknowledge } => {
                    super::super::push_preflight::dialog(ui, plan, acknowledge)
                }
                Dialog::PushError(error) => push_error::show(ui, error),
                Dialog::WorkingDraftLoading => {
                    ui.heading(crate::i18n::text("repo-core-1397"));
                    ui.add(crate::spinner::Spinner::new());
                    if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
                        Outcome::Cancel
                    } else {
                        Outcome::Open
                    }
                }
                Dialog::WorkingDraftHistory {
                    on,
                    versions,
                    error,
                    index,
                } => super::super::working_drafts::view::history(
                    ui,
                    on,
                    versions,
                    error.as_deref(),
                    index,
                    idle,
                    working_draft_undo,
                ),
                Dialog::WorkingDraftRecovery { preview, selected } => {
                    super::super::working_drafts::view::recovery(ui, preview, selected, idle)
                }
                Dialog::ContentRecovery => self.content_recovery.show(ui, idle),
                Dialog::GerritReview {
                    remote,
                    head,
                    targets,
                    target,
                    topic,
                    skip_hooks,
                    web_warning,
                } => gerrit::review(
                    ui,
                    remote,
                    head,
                    targets,
                    target,
                    topic,
                    *skip_hooks,
                    web_warning.as_deref(),
                ),
                Dialog::RepositoryTools(state) => {
                    super::super::repository_tools::show(ui, state, acts);
                    if ui.button(crate::i18n::text("repo-core-0595")).clicked() {
                        Outcome::Cancel
                    } else {
                        Outcome::Open
                    }
                }
                Dialog::Behaviour(form) => form.show(ui),
                Dialog::LinkedMachines(form) => form.show(ui),
                Dialog::BundleLoading(_) => {
                    ui.heading(crate::i18n::text("repo-core-1398"));
                    ui.label(crate::i18n::text("repo-core-1399"));
                    ui.add(crate::spinner::Spinner::new());
                    if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
                        Outcome::Cancel
                    } else {
                        Outcome::Open
                    }
                }
                Dialog::BundleExport {
                    refs,
                    selected,
                    base,
                } => bundles::export(ui, refs, selected, base, idle),
                Dialog::BundleIncoming(form) => form.show(ui),
                Dialog::CreateBranch {
                    at,
                    at_label,
                    name,
                    checkout,
                    focus,
                } => branch::create_branch(ui, &previous, at, at_label, name, checkout, focus),
                Dialog::AddWorktree {
                    repo,
                    root,
                    path,
                    path_edited,
                    new_branch,
                    branch,
                    new_name,
                    base,
                    base_label,
                    focus,
                    ..
                } => {
                    let free = worktree::free_branches(local, worktrees);
                    worktree::add_worktree(
                        ui,
                        repo,
                        root,
                        enter,
                        &free,
                        path,
                        path_edited,
                        new_branch,
                        branch,
                        new_name,
                        (base, base_label),
                        focus,
                    )
                }
                Dialog::Stash(d) => stash::stash(ui, d, work, staged_support),
                Dialog::Tag {
                    at,
                    at_label,
                    name,
                    message,
                    remote,
                    push,
                    release,
                    focus,
                } => branch::tag(
                    ui, at, at_label, name, message, remote, push, release, focus,
                ),
                Dialog::RenameBranch { old, new, focus } => {
                    let children = crate::git::stack::children(stack_parents, old);
                    branch::rename_branch(ui, enter, old, new, focus, &children)
                }
                Dialog::Remote {
                    original,
                    name,
                    url,
                    focus,
                } => remote::remote(ui, enter, original, name, url, focus),
                Dialog::SetUpstream {
                    branch,
                    remote,
                    name,
                    undo,
                    focus,
                } => upstream::set_upstream(
                    ui,
                    enter,
                    branch,
                    remote,
                    name,
                    undo,
                    &remote_names,
                    remote_branches,
                    focus,
                ),
                Dialog::Credentials {
                    url,
                    endpoints,
                    refused,
                    username,
                    password,
                    focus,
                } => remote::credentials(
                    ui, enter, url, endpoints, *refused, username, password, focus,
                ),
                Dialog::CreatePr {
                    noun,
                    branch,
                    base,
                    title,
                    body,
                    draft,
                    reviewers,
                    focus,
                } => {
                    remote::create_pr(ui, noun, branch, base, title, body, draft, reviewers, focus)
                }
                Dialog::Fold(plan) => fold::fold(ui, plan),
                Dialog::Custom(custom) => custom::custom(ui, enter, custom),
                Dialog::KeepDetached(keep) => keep_detached::keep_detached(ui, enter, keep, local),
                Dialog::CheckoutBlocked(blocked) => checkout_blocked::checkout_blocked(ui, blocked),
                Dialog::Cleanup(dialog) => cleanup::cleanup(ui, dialog),
                Dialog::StartBisect {
                    hash,
                    this_good,
                    other,
                    focus,
                } => bisect::start_bisect(
                    ui,
                    enter,
                    hash,
                    this_good,
                    other,
                    focus,
                    bisect_blocker.as_deref(),
                ),
                Dialog::BisectRun { command, focus } => {
                    bisect::bisect_run(ui, enter, &machine, command, focus)
                }
                Dialog::ParallelBisect(form) => parallel_bisect::parallel_bisect(ui, enter, form),
                Dialog::Rehearse {
                    other,
                    remote,
                    rebase,
                    compare,
                    command,
                    focus,
                } => rehearse::rehearse(
                    ui,
                    &machine,
                    current_branch.as_deref(),
                    other,
                    *remote,
                    rebase,
                    compare,
                    command,
                    focus,
                ),
                Dialog::FindInteraction { spec, focus } => {
                    interaction::find_interaction(ui, enter, &machine, spec, focus)
                }
                Dialog::TestCommits {
                    start,
                    stored,
                    focus,
                } => test_commits::test_commits(
                    ui,
                    &machine,
                    is_remote,
                    start,
                    stored.as_deref(),
                    focus,
                ),
                Dialog::KeyboardDiscard { request } => {
                    let mut alternative = false;
                    match confirm::confirm(
                        ui,
                        confirm::Asked {
                            title: crate::i18n::text("repo-core-1400"),
                            body: crate::i18n::text("repo-core-1401"),
                            button: crate::i18n::text("repo-core-1402"),
                            ops: &[],
                            alternative: &None,
                            refresh: Refresh::Status,
                            undo: &Undo::None,
                        },
                        &mut alternative,
                        &mut WhatItDoes::default(),
                        false,
                    ) {
                        Outcome::Done(_) => {
                            Outcome::Done(Box::new(Action::Keyboard(request.clone())))
                        }
                        outcome => outcome,
                    }
                }
                Dialog::Confirm {
                    title,
                    body,
                    button,
                    ops,
                    alternative,
                    use_alternative,
                    refresh,
                    undo,
                    preview,
                    safe,
                } => confirm::confirm(
                    ui,
                    confirm::Asked {
                        title,
                        body,
                        button,
                        ops,
                        alternative,
                        refresh: *refresh,
                        undo,
                    },
                    use_alternative,
                    preview,
                    *safe,
                ),
            }
        });

        match response.inner {
            Outcome::Done(action) => {
                acts.push(*action);
                self.dialog = None;
            }
            Outcome::Cancel => self.dialog = None,
            Outcome::Open if response.should_close() => self.dialog = None,
            Outcome::Open => {}
        }
    }
}
