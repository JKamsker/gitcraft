//! Modal dialogs: create a branch, confirm a destructive operation.

mod behaviour;
mod bisect;
pub(super) mod branch;
mod bundles;
mod cleanup;
mod confirm;
mod constructors;
mod custom;
#[cfg(test)]
mod enter_tests;
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
mod window;
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
