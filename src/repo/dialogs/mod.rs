//! Modal dialogs: create a branch, confirm a destructive operation.

mod behaviour;
mod bisect;
pub(super) mod branch;
mod bundles;
mod cleanup;
mod confirm;
mod custom;
mod fold;
mod gerrit;
mod interaction;
mod keep_detached;
mod linked_machines;
mod parallel_bisect;
mod preview;
mod push_error;
#[cfg(test)]
mod rebase_test_support;
mod rehearse;
mod remote;
mod stash;
mod test_commits;
mod upstream;
mod worktree;

use std::path::{Path, PathBuf};

use egui::{Key, RichText};

pub(crate) use preview::WhatItDoes;

pub use custom::CustomDialog;

pub use self::stash::Choice as StashChoice;
pub(crate) use self::stash::sentence;
use super::partial_stash::StagedSupport;
pub use parallel_bisect::ParallelForm;

use super::widgets;
use super::{Action, Refresh, RepoTab, Undo};
use crate::theme::{self, Tokens};

pub enum Dialog {
    PushLoading {
        id: u64,
        token: std::sync::Arc<()>,
    },
    PushPreflight {
        plan: Box<crate::push_preflight::Plan>,
        acknowledge: bool,
    },
    PushError(String),
    WorkingDraftLoading,
    WorkingDraftHistory {
        on: bool,
        versions: Vec<crate::git::drafts::Draft>,
        error: Option<String>,
        index: bool,
    },
    WorkingDraftRecovery {
        preview: Box<crate::git::drafts::RestorePreview>,
        selected: std::collections::BTreeSet<usize>,
    },
    ContentRecovery,
    RepositoryTools(Box<super::repository_tools::State>),
    GerritReview {
        remote: String,
        head: String,
        targets: Vec<(String, String)>,
        target: String,
        topic: String,
        skip_hooks: bool,
        web_warning: Option<String>,
    },
    Behaviour(Box<behaviour::Form>),
    LinkedMachines(Box<linked_machines::Form>),
    BundleLoading(u64),
    BundleExport {
        refs: Vec<crate::git::BundleRef>,
        selected: std::collections::BTreeSet<String>,
        base: String,
    },
    BundleIncoming(Box<bundles::Form>),
    CreateBranch {
        /// Commit the branch starts at.
        at: String,
        /// How to show `at` (branch name or short hash).
        at_label: String,
        name: String,
        checkout: bool,
        focus: bool,
    },
    AddWorktree {
        /// The main checkout, so relative roots stay the same from every worktree.
        repo: PathBuf,
        root: String,
        path: String,
        /// The user typed a path; stop suggesting one.
        path_edited: bool,
        /// Create a new branch instead of checking out an existing one.
        new_branch: bool,
        branch: String,
        new_name: String,
        /// Where a new branch starts (HEAD's commit, or the one it was asked for from).
        base: String,
        /// How to show `base`.
        base_label: String,
        focus: bool,
    },
    KeyboardDiscard {
        request: super::keyboard::Request,
    },
    Confirm {
        title: String,
        body: String,
        button: String,
        ops: Vec<Vec<String>>,
        /// An optional checkbox (label, ops to run instead when ticked), e.g. force delete.
        alternative: Option<(String, Vec<Vec<String>>)>,
        use_alternative: bool,
        refresh: Refresh,
        undo: Undo,
        /// What the operation changes, read when the dialog opens, and an action that runs
        /// in place of `ops`, if any.
        preview: Box<WhatItDoes>,
        /// Nothing is lost: the button is the accent colour, not the danger one.
        safe: bool,
    },
    Stash(stash::StashDialog),
    Tag {
        at: String,
        at_label: String,
        name: String,
        message: String,
        /// Remote to push the tag to, if any.
        remote: Option<String>,
        push: bool,
        release: bool,
        focus: bool,
    },
    RenameBranch {
        old: String,
        new: String,
        focus: bool,
    },
    /// Add a remote (`original` = None) or edit one's name and URL.
    Remote {
        original: Option<(String, String)>,
        name: String,
        url: String,
        focus: bool,
    },
    /// The remote branch `branch` tracks: `remote`/`name`. `undo` brings back the one before.
    SetUpstream {
        branch: String,
        remote: String,
        name: String,
        undo: Vec<Vec<String>>,
        focus: bool,
    },
    /// Username and password / token for an HTTPS remote that asked for them.
    Credentials {
        url: String,
        endpoints: Vec<String>,
        /// The last ones typed were refused.
        refused: bool,
        username: String,
        password: String,
        focus: bool,
    },
    /// Fold the staged changes into an older commit, as read just before.
    Fold(Box<super::fold::Plan>),
    CreatePr {
        noun: String,
        branch: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
        /// The owners of what it changes (CODEOWNERS), to ask for reviews.
        reviewers: Vec<super::owners::Suggested>,
        focus: bool,
    },
    /// A custom command's prompts, or a repository's commands to trust.
    Custom(Box<CustomDialog>),
    /// A checkout would leave the commits of a detached HEAD on no branch.
    KeepDetached(Box<super::detached::KeepDialog>),
    /// Select stale or merged branches…
    Cleanup(Box<cleanup::CleanupDialog>),
    /// Start a bisect from the commit `hash`, good or bad, with the other end `other`.
    StartBisect {
        hash: String,
        this_good: bool,
        other: String,
        focus: bool,
    },
    /// `git bisect run` with the command typed.
    BisectRun {
        command: String,
        focus: bool,
    },
    /// A parallel bisect: its ends, command, machine and limits.
    ParallelBisect(Box<ParallelForm>),
    /// Rehearse a rebase or merge of the checked-out branch with `other`.
    Rehearse {
        other: String,
        remote: bool,
        rebase: bool,
        compare: bool,
        command: String,
        focus: bool,
    },
    /// Look for the commits of two branches that fail together.
    FindInteraction {
        spec: crate::git::InteractionSpec,
        focus: bool,
    },
    /// Test a range of commits; `stored` is the repository's test command.
    TestCommits {
        start: crate::repo::test_lane::StartTests,
        stored: Option<String>,
        focus: bool,
    },
}

impl Dialog {
    pub fn create_branch(at: &str, at_label: &str) -> Self {
        Dialog::CreateBranch {
            at: at.to_owned(),
            at_label: at_label.to_owned(),
            name: String::new(),
            checkout: true,
            focus: true,
        }
    }

    pub fn confirm(
        title: &str,
        body: String,
        button: &str,
        ops: Vec<Vec<String>>,
        alternative: Option<(String, Vec<Vec<String>>)>,
    ) -> Self {
        Dialog::Confirm {
            title: title.to_owned(),
            body,
            button: button.to_owned(),
            ops,
            alternative,
            use_alternative: false,
            refresh: Refresh::All,
            undo: Undo::None,
            preview: Box::default(),
            safe: false,
        }
    }

    /// Has a confirmation show what its operation changes, read from git with `ask`; `heading`
    /// goes over the commits leaving.
    pub fn preview(mut self, ask: crate::git::PreviewAsk, heading: String) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            preview.ask = ask;
            preview.heading = heading;
        }
        self
    }

    /// What the preview calls the files lost (`Unstaged changes lost`).
    pub fn files_heading(mut self, heading: &str) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            heading.clone_into(&mut preview.files_heading);
        }
        self
    }

    /// [`Self::preview`] of an (ask, heading) pair.
    pub fn preview_of(self, (ask, heading): (crate::git::PreviewAsk, String)) -> Self {
        self.preview(ask, heading)
    }

    /// Has a confirmation take `action` rather than run its ops; `described` says what it runs.
    pub fn instead(mut self, action: Action, described: Vec<String>) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            preview.instead = Some(Instead { action, described });
        }
        self
    }

    /// A confirmation of something that loses nothing: its button is not the danger colour.
    pub fn safe(mut self) -> Self {
        if let Dialog::Confirm { safe, .. } = &mut self {
            *safe = true;
        }
        self
    }

    /// Makes a confirmed operation undoable.
    pub fn undo(mut self, how: Undo) -> Self {
        if let Dialog::Confirm { undo, .. } = &mut self {
            *undo = how;
        }
        self
    }

    /// The checklist of `candidates` to delete; `checking` while which are merged is still being
    /// read, `error` when reading it failed.
    pub fn cleanup(
        candidates: Vec<crate::repo::branch_cleanup::Candidate>,
        checking: bool,
        error: Option<String>,
    ) -> Self {
        Dialog::Cleanup(Box::new(cleanup::CleanupDialog::new(
            candidates, checking, error,
        )))
    }

    pub fn credentials(url: &str, refused: bool) -> Self {
        Dialog::Credentials {
            url: url.to_owned(),
            endpoints: vec![url.to_owned()],
            refused,
            username: String::new(),
            password: String::new(),
            focus: true,
        }
    }

    /// Stash everything (or the files picked in the changes, when there are some).
    pub fn stash() -> Self {
        Self::stash_as(StashChoice::Everything)
    }

    /// The stash dialog with `choice` chosen.
    pub fn stash_as(choice: StashChoice) -> Self {
        Dialog::Stash(stash::StashDialog::new(choice, Vec::new()))
    }

    /// The stash dialog for just `files`.
    pub fn stash_files(files: Vec<String>) -> Self {
        Dialog::Stash(stash::StashDialog::new(StashChoice::Files, files))
    }

    pub fn tag(at: &str, at_label: &str, remote: Option<String>) -> Self {
        Dialog::Tag {
            at: at.to_owned(),
            at_label: at_label.to_owned(),
            name: String::new(),
            message: String::new(),
            remote,
            push: false,
            release: false,
            focus: true,
        }
    }

    pub fn rename_branch(old: &str) -> Self {
        Dialog::RenameBranch {
            old: old.to_owned(),
            new: old.to_owned(),
            focus: true,
        }
    }

    pub fn remote(original: Option<(String, String)>) -> Self {
        let (name, url) = original.clone().unwrap_or_default();
        Dialog::Remote {
            original,
            name,
            url,
            focus: true,
        }
    }

    pub fn set_upstream(branch: &str, remote: String, name: &str, undo: Vec<Vec<String>>) -> Self {
        Dialog::SetUpstream {
            branch: branch.to_owned(),
            remote,
            name: name.to_owned(),
            undo,
            focus: true,
        }
    }

    pub fn create_pr(noun: &str, branch: &str, base: &str, title: &str, body: &str) -> Self {
        Dialog::CreatePr {
            noun: noun.to_owned(),
            branch: branch.to_owned(),
            base: base.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            draft: false,
            reviewers: Vec::new(),
            focus: true,
        }
    }

    /// Add a worktree, checking out `branch` (or a new branch from `base` when `None`).
    pub fn add_worktree(repo: &Path, branch: Option<&str>, base: &str) -> Self {
        let mut dialog = Dialog::AddWorktree {
            repo: repo.to_path_buf(),
            root: crate::remote::git_path(&crate::git::default_worktree_root(repo)),
            path: String::new(),
            path_edited: false,
            new_branch: branch.is_none(),
            branch: branch.unwrap_or_default().to_owned(),
            new_name: String::new(),
            base: base.to_owned(),
            base_label: format!("HEAD ({})", base.get(..8).unwrap_or(base)),
            focus: true,
        };
        dialog.suggest_path();
        dialog
    }

    /// A worktree on a new branch starting at the commit `hash`.
    pub fn worktree_at(repo: &Path, hash: &str) -> Self {
        let mut dialog = Self::add_worktree(repo, None, hash);
        if let Dialog::AddWorktree { base_label, .. } = &mut dialog {
            *base_label = hash.get(..8).unwrap_or(hash).to_owned();
        }
        dialog
    }

    /// Starts a bisect from `hash`: good with HEAD as the bad end, or bad with a good end to
    /// pick.
    pub fn start_bisect(hash: &str, this_good: bool) -> Self {
        Dialog::StartBisect {
            hash: hash.to_owned(),
            this_good,
            other: if this_good { "HEAD" } else { "" }.to_owned(),
            focus: true,
        }
    }

    /// Runs a command for the bisect, `last` (the one run before) offered.
    pub fn bisect_run(last: &str) -> Self {
        Dialog::BisectRun {
            command: last.to_owned(),
            focus: true,
        }
    }

    pub fn refresh(mut self, what: Refresh) -> Self {
        if let Dialog::Confirm { refresh, .. } = &mut self {
            *refresh = what;
        }
        self
    }
}

/// An action a confirmation takes in place of its ops, and the commands it is shown as.
pub(crate) struct Instead {
    pub(super) action: Action,
    pub(super) described: Vec<String>,
}

pub(super) enum Outcome {
    Open,
    Cancel,
    Done(Box<Action>),
}

impl RepoTab {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.history_activity_window(ctx, acts);
        self.atlas_window(ctx);
        self.sparse_window(ctx, acts);
        self.cleanup_window(ctx);
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
                    super::push_preflight::dialog(ui, plan, acknowledge)
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
                } => super::working_drafts::view::history(
                    ui,
                    on,
                    versions,
                    error.as_deref(),
                    index,
                    idle,
                    working_draft_undo,
                ),
                Dialog::WorkingDraftRecovery { preview, selected } => {
                    super::working_drafts::view::recovery(ui, preview, selected, idle)
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
                    super::repository_tools::show(ui, state, acts);
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
                } => {
                    branch::create_branch(ui, enter, &previous, at, at_label, name, checkout, focus)
                }
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
                Dialog::Stash(d) => stash::stash(ui, enter, d, work, staged_support),
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
                    ui, enter, at, at_label, name, message, remote, push, release, focus,
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
                Dialog::Fold(plan) => fold::fold(ui, enter, plan),
                Dialog::Custom(custom) => custom::custom(ui, enter, custom),
                Dialog::KeepDetached(keep) => keep_detached::keep_detached(ui, enter, keep, local),
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
                    enter,
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
                    enter,
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

/// A name, path or URL that can go to git as typed: something, and nothing git would read as
/// an option (`-D` as a new branch's name would make `git branch -D <start>` delete a branch).
fn usable(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !text.starts_with('-')
}

/// A dialog's heading and one line under it.
fn title(ui: &mut egui::Ui, heading: &str, sub: &str, t: &Tokens) {
    ui.label(RichText::new(heading).font(theme::semibold(15.0)));
    ui.label(RichText::new(sub).color(t.text_dim));
    ui.add_space(10.0);
}

/// A single-line field that takes the keyboard when the dialog opens.
fn field(ui: &mut egui::Ui, text: &mut String, hint: &str, focus: &mut bool) -> egui::Response {
    let edit = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if *focus {
        edit.request_focus();
        *focus = false;
    }
    edit
}

/// Spaces are never valid in a branch or tag name; typing one gives a dash.
fn dash_spaces(name: &mut String) {
    if name.contains(' ') {
        *name = name.replace(' ', "-");
    }
}

/// The primary button (also Enter, when `enter`) and Cancel.
fn buttons(
    ui: &mut egui::Ui,
    label: &str,
    valid: bool,
    enter: bool,
    done: impl FnOnce() -> Action,
) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if widgets::primary_button(
            ui,
            &crate::repo::messages::display_operation(label),
            valid,
            90.0,
        )
        .clicked()
            || (enter && valid)
        {
            outcome = Outcome::Done(Box::new(done()));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// Whether Cancel was clicked.
fn cancel_button(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new(crate::i18n::text("repo-core-0443")).min_size(egui::vec2(80.0, 30.0)))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_like_names_are_refused() {
        assert!(usable(" feature/x "));
        assert!(!usable("   "));
        assert!(!usable("-D"));
        assert!(!usable(" --upload-pack=x"));
    }
}
